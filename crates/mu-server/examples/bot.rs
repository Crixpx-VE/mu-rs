//! Headless test client: does the same as the game against the server.
//!
//!   cargo run -p mu-server --example bot -- <account> <password> <character> [connect=127.0.0.1:44405] [seconds=3]
//!                                           [M destination] [--decir <text>] [--susurrar <name> <text>]
//!                                           [--esperar <seconds before speaking>]
//!                                           [--puerta x,y,gate ...] [--accion <AT_*>] [--baul]
//!
//! Connect Server → Game Server → login → list → (creates the character if missing) → enters →
//! walks 3 steps → (with a destination: requests it like the `M` window, waits for `MapChanged` and replies
//! `ClientReadyAfterMapChange`, or the notice if it cannot) → (each `--puerta`: stands at (x, y)
//! as the client does when stepping on it and asks to cross it with `EnterGateRequest`, like the destination) →
//! (waits, says, whispers and plays the requested animation, e.g. 128 = sit) → waits
//! showing what arrives (other players, chat, whispers, notices, animations) → exits.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use mu_protocol::connect;
use mu_protocol::crypto::ClientCrypto;
use mu_protocol::game::{ClientMessage, LoginResult, ServerMessage, map_move_key};
use mu_protocol::packet::PacketBuffer;

struct Conn {
    sock: TcpStream,
    buf: PacketBuffer,
    out: ClientCrypto,
    inp: ClientCrypto,
}

impl Conn {
    fn recv(&mut self) -> std::io::Result<Option<ServerMessage>> {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(p) = self.buf.next_packet().map_err(|e| std::io::Error::other(format!("{:#04X}", e.0)))? {
                let plain = self.inp.decrypt(&p).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
                return Ok(ServerMessage::decode(&plain));
            }
            let n = self.sock.read(&mut chunk)?;
            if n == 0 {
                return Err(std::io::Error::other("conexión cerrada"));
            }
            self.buf.extend(&chunk[..n]);
        }
    }

    /// Like [`Conn::recv`], with the decrypted bytes.
    fn recv_raw(&mut self) -> std::io::Result<(Vec<u8>, Option<ServerMessage>)> {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(p) = self.buf.next_packet().map_err(|e| std::io::Error::other(format!("{:#04X}", e.0)))? {
                let plain = self.inp.decrypt(&p).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
                let m = ServerMessage::decode(&plain);
                return Ok((plain, m));
            }
            let n = self.sock.read(&mut chunk)?;
            if n == 0 {
                return Err(std::io::Error::other("conexión cerrada"));
            }
            self.buf.extend(&chunk[..n]);
        }
    }

    /// Shows (with their bytes) what arrives during `secs` seconds; returns the decoded messages.
    fn collect(&mut self, user: &str, secs: f32) -> std::io::Result<Vec<ServerMessage>> {
        let end = Instant::now() + Duration::from_secs_f32(secs);
        let mut out = Vec::new();
        while Instant::now() < end {
            match self.recv_raw() {
                Ok((raw, m)) => {
                    let hex: Vec<String> = raw.iter().map(|b| format!("{b:02X}")).collect();
                    println!("[{user}] <- {} = {m:?}", hex.join(" "));
                    out.extend(m);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e),
            }
        }
        Ok(out)
    }

    fn send(&mut self, m: ClientMessage) -> std::io::Result<()> {
        self.sock.write_all(&self.out.encrypt(&m.encode()))
    }

    /// Waits for a message matching `f` (the others are shown).
    fn wait<T>(&mut self, mut f: impl FnMut(&ServerMessage) -> Option<T>) -> std::io::Result<T> {
        loop {
            match self.recv()? {
                Some(m) => match f(&m) {
                    Some(t) => return Ok(t),
                    None => println!("  (llega {m:?})"),
                },
                None => println!("  (paquete sin traducir)"),
            }
        }
    }
}

fn cs_packet(s: &mut TcpStream, buf: &mut PacketBuffer) -> std::io::Result<connect::ServerMessage> {
    let mut chunk = [0u8; 1024];
    loop {
        if let Some(p) = buf.next_packet().map_err(|e| std::io::Error::other(format!("{:#04X}", e.0)))? {
            if let Some(m) = connect::ServerMessage::decode(&p) {
                return Ok(m);
            }
            continue;
        }
        let n = s.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::other("cs cerrado"));
        }
        buf.extend(&chunk[..n]);
    }
}

/// What arrives, in plain text for chat and notices.
fn show(user: &str, m: &ServerMessage) {
    match m {
        ServerMessage::ChatMessage { sender, message } => println!("[{user}] CHAT {sender} : {message}"),
        ServerMessage::WhisperMessage { sender, message } => println!("[{user}] SUSURRO de {sender} : {message}"),
        ServerMessage::WhisperResult { success } => println!("[{user}] SUSURRO entregado: {success}"),
        ServerMessage::Notice { kind, text } => println!("[{user}] AVISO ({kind}) {text}"),
        m => println!("[{user}] llega {m:?}"),
    }
}

/// `--baul`: talks to the Safety Guardian (NPC 240, it must be in view), moves the first inventory
/// grid item into vault slot 119 and back, deposits and withdraws 1 zen, asks for more than there is
/// and closes the vault, printing every packet's bytes (to capture OpenMU's vault packets).
fn vault_script(c: &mut Conn, user: &str, (x, y): (u8, u8)) -> std::io::Result<()> {
    c.sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let sent = |m: &ClientMessage| {
        let hex: Vec<String> = m.encode().iter().map(|b| format!("{b:02X}")).collect();
        println!("[{user}] -> {} = {m:?}", hex.join(" "));
    };
    let mut first = c.collect(user, 2.0)?;
    let find = |msgs: &[ServerMessage]| {
        msgs.iter().find_map(|m| match m {
            ServerMessage::AddMonstersToScope(list) => list.iter().find(|n| n.number == 240).map(|n| n.id & 0x7FFF),
            _ => None,
        })
    };
    if find(&first).is_none() {
        // Toward the Lorencia guardian at (147, 145), first south and then east, at most 15 steps.
        let (mut px, mut py) = (x as i32, y as i32);
        let mut directions = Vec::new();
        while directions.len() < mu_protocol::game::MAX_WALK_STEPS && (px.abs_diff(147) > 1 || py.abs_diff(145) > 2) {
            let (dx, dy) = if py != 140 { (0, (140 - py).signum()) } else { ((147 - px).signum(), 0) };
            let Some(d) = mu_protocol::game::DIRECTIONS.iter().position(|&(a, b)| (a as i32, b as i32) == (dx, dy)) else { break };
            directions.push(d as u8);
            (px, py) = (px + dx, py + dy);
        }
        let walk = ClientMessage::Walk { source_x: x, source_y: y, rotation: 5, directions };
        sent(&walk);
        c.send(walk)?;
        first.extend(c.collect(user, 3.0)?);
    }
    let guardian = find(&first);
    let item = first.iter().find_map(|m| match m {
        ServerMessage::InventoryList(list) => list.iter().find(|(s, _)| (12..76).contains(s)).copied(),
        _ => None,
    });
    let Some(id) = guardian else {
        println!("[{user}] el guardián del baúl (240) no está a la vista");
        return Ok(());
    };
    let step = |c: &mut Conn, m: ClientMessage, secs: f32| -> std::io::Result<()> {
        sent(&m);
        c.send(m)?;
        c.collect(user, secs).map(|_| ())
    };
    sent(&ClientMessage::TalkToNpc { id });
    c.send(ClientMessage::TalkToNpc { id })?;
    let opened = c.collect(user, 2.0)?;
    use mu_protocol::game::storage::{INVENTORY, VAULT};
    if let Some((slot, bytes)) = item {
        step(c, ClientMessage::ItemMove { from_storage: INVENTORY, from_slot: slot, item: bytes, to_storage: VAULT, to_slot: 119 }, 1.5)?;
        step(c, ClientMessage::ItemMove { from_storage: VAULT, from_slot: 119, item: bytes, to_storage: INVENTORY, to_slot: slot }, 1.5)?;
    }
    // The first vault item to the last inventory cell (slot 75) and back.
    let stored = opened.iter().find_map(|m| match m {
        ServerMessage::StoreItemList { items, .. } => items.first().copied(),
        _ => None,
    });
    if let Some((slot, bytes)) = stored {
        step(c, ClientMessage::ItemMove { from_storage: VAULT, from_slot: slot, item: bytes, to_storage: INVENTORY, to_slot: 75 }, 1.5)?;
        step(c, ClientMessage::ItemMove { from_storage: INVENTORY, from_slot: 75, item: bytes, to_storage: VAULT, to_slot: slot }, 1.5)?;
    }
    step(c, ClientMessage::VaultMoveMoney { withdraw: false, amount: 1 }, 1.0)?;
    step(c, ClientMessage::VaultMoveMoney { withdraw: true, amount: 1 }, 1.0)?;
    step(c, ClientMessage::VaultMoveMoney { withdraw: true, amount: 2_000_000_000 }, 1.0)?;
    step(c, ClientMessage::CloseVault, 1.0)?;
    c.send(ClientMessage::Logout { kind: 0 })?;
    println!("[{user}] fuera");
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    // Named options (removed from the positional ones).
    let mut say = None;
    let mut whisper = None;
    let mut pause = 0.0f32;
    let mut gates: Vec<(u8, u8, u16)> = Vec::new();
    let mut animation: Option<u8> = None;
    let mut vault = false;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--baul" => {
                vault = true;
                a.remove(i);
            }
            "--puerta" if i + 1 < a.len() => {
                let v: Vec<u16> = a.remove(i + 1).split(',').filter_map(|n| n.trim().parse().ok()).collect();
                if let [x, y, g] = v[..] {
                    gates.push((x as u8, y as u8, g));
                }
                a.remove(i);
            }
            "--accion" if i + 1 < a.len() => {
                animation = a.remove(i + 1).parse().ok();
                a.remove(i);
            }
            "--decir" if i + 1 < a.len() => {
                say = Some(a.remove(i + 1));
                a.remove(i);
            }
            "--susurrar" if i + 2 < a.len() => {
                let text = a.remove(i + 2);
                whisper = Some((a.remove(i + 1), text));
                a.remove(i);
            }
            "--esperar" if i + 1 < a.len() => {
                pause = a.remove(i + 1).parse().unwrap_or(0.0);
                a.remove(i);
            }
            _ => i += 1,
        }
    }
    let (user, pass, name) = (&a[0], &a[1], &a[2]);
    let cs_addr = a.get(3).map(String::as_str).unwrap_or("127.0.0.1:44405");
    let secs: u64 = a.get(4).and_then(|s| s.parse().ok()).unwrap_or(3);
    let warp: Option<u16> = a.get(5).and_then(|s| s.parse().ok());

    let mut cs = TcpStream::connect(cs_addr)?;
    cs.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = PacketBuffer::default();
    cs_packet(&mut cs, &mut buf)?;
    cs.write_all(&connect::ClientMessage::ServerListRequest.encode())?;
    let list = cs_packet(&mut cs, &mut buf)?;
    println!("[{user}] lista: {list:?}");
    cs.write_all(&connect::ClientMessage::ConnectionInfoRequest { server_id: 0 }.encode())?;
    let connect::ServerMessage::ConnectionInfo { ip, port } = cs_packet(&mut cs, &mut buf)? else {
        return Err("sin dirección".into());
    };

    let sock = TcpStream::connect((ip.as_str(), port))?;
    sock.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut c = Conn { sock, buf: PacketBuffer::default(), out: ClientCrypto::default(), inp: ClientCrypto::default() };
    let id = c.wait(|m| match m {
        ServerMessage::GameServerEntered { player_id, .. } => Some(*player_id),
        _ => None,
    })?;
    println!("[{user}] saludo, id {id}");
    c.send(ClientMessage::login(user, pass, 0))?;
    let r = c.wait(|m| match m {
        ServerMessage::LoginResponse(r) => Some(*r),
        _ => None,
    })?;
    println!("[{user}] login: {r:?}");
    if r != LoginResult::Okay {
        return Ok(());
    }
    c.send(ClientMessage::RequestCharacterList { language: 0 })?;
    let chars = c.wait(|m| match m {
        ServerMessage::CharacterList { characters, .. } => Some(characters.clone()),
        _ => None,
    })?;
    println!("[{user}] personajes: {:?}", chars.iter().map(|c| &c.name).collect::<Vec<_>>());
    for c in &chars {
        println!("[{user}]   {} apariencia {:?}", c.name, c.appearance);
    }
    if !chars.iter().any(|c| &c.name == name) {
        c.send(ClientMessage::CreateCharacter { name: name.clone(), class: mu_protocol::game::class_number(1) })?;
        let created = c.wait(|m| match m {
            ServerMessage::CharacterCreated { name, slot, .. } => Some(format!("creado {name} en el hueco {slot}")),
            ServerMessage::CharacterCreationFailed => Some("no se pudo crear".into()),
            _ => None,
        })?;
        println!("[{user}] {created}");
    }
    c.send(ClientMessage::SelectCharacter { name: name.clone() })?;
    let info = c.wait(|m| match m {
        ServerMessage::CharacterInformation(i) => Some(*i),
        _ => None,
    })?;
    println!("[{user}] dentro: mapa {} ({}, {}) vida {}/{} maná {}/{}", info.map, info.x, info.y, info.health, info.max_health, info.mana, info.max_mana);
    if vault {
        println!("[{user}] zen {}", info.money);
        return Ok(vault_script(&mut c, user, (info.x, info.y))?);
    }
    // Arrives right after the information (`ReceiveMoveMapChecksum`).
    let mut key = c.wait(|m| match m {
        ServerMessage::MapMoveChecksumKey { key } => Some(*key),
        _ => None,
    })?;
    println!("[{user}] clave de mover {key:#010X}, zen {}", info.money);
    c.send(ClientMessage::Walk { source_x: info.x, source_y: info.y, rotation: 3, directions: vec![3, 3, 3] })?;
    if let Some(index) = warp {
        // `GetMoveCommandKey` + `SendRequestMoveMap`.
        key = map_move_key(key);
        c.send(ClientMessage::WarpCommandRequest { key, index })?;
        let changed = c.wait(|m| match m {
            ServerMessage::MapChanged { map, x, y, .. } => Some(Ok((*map, *x, *y))),
            ServerMessage::Notice { text, .. } => Some(Err(text.clone())),
            _ => None,
        })?;
        match changed {
            Ok((map, x, y)) => {
                println!("[{user}] cambia al mapa {map} ({x}, {y})");
                // `SendRequestFinishLoading` once loaded.
                c.send(ClientMessage::ClientReadyAfterMapChange)?;
            }
            Err(text) => println!("[{user}] no puede mover: AVISO {text}"),
        }
    }
    for (x, y, gate) in gates {
        // The client walks to the gate: the server stores the destination tile.
        c.send(ClientMessage::Walk { source_x: x, source_y: y, rotation: 5, directions: vec![] })?;
        c.send(ClientMessage::EnterGate { gate, x: 0, y: 0 })?;
        println!("[{user}] pisa la puerta {gate} en ({x}, {y})");
        let mut notice = None;
        let changed = c.wait(|m| match m {
            ServerMessage::MapChanged { is_map_change, map, x, y, .. } => Some((*is_map_change, *map, *x, *y)),
            ServerMessage::Notice { text, .. } => {
                notice = Some(text.clone());
                None
            }
            _ => None,
        })?;
        if let Some(text) = notice {
            println!("[{user}] AVISO {text}");
        }
        match changed {
            (true, map, x, y) => {
                println!("[{user}] cruza la puerta {gate}: mapa {map} ({x}, {y})");
                c.send(ClientMessage::ClientReadyAfterMapChange)?;
            }
            (false, map, x, y) => println!("[{user}] no cruza la puerta {gate}: se queda en el mapa {map} ({x}, {y})"),
        }
    }
    c.sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let mut said = say.is_none() && whisper.is_none() && animation.is_none();
    let talk_at = Instant::now() + Duration::from_secs_f32(pause);
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        if !said && Instant::now() >= talk_at {
            said = true;
            if let Some(text) = &say {
                println!("[{user}] dice: {text}");
                c.send(ClientMessage::PublicChat { name: name.clone(), message: text.clone() })?;
            }
            if let Some((to, text)) = &whisper {
                println!("[{user}] susurra a {to}: {text}");
                c.send(ClientMessage::Whisper { target: to.clone(), message: text.clone() })?;
            }
            if let Some(action) = animation {
                println!("[{user}] hace la acción {action}");
                c.send(ClientMessage::Animation { rotation: 3, action })?;
            }
        }
        match c.recv() {
            Ok(Some(m)) => show(user, &m),
            Ok(None) => {}
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
            Err(e) => return Err(e.into()),
        }
    }
    c.send(ClientMessage::Logout { kind: 0 })?;
    println!("[{user}] fuera");
    Ok(())
}
