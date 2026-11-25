//! Headless test client: does the same as the game against the server.
//!
//!   cargo run -p mu-server --example bot -- <account> <password> <character> [connect=127.0.0.1:44405] [seconds=3]
//!                                           [M destination] [--decir <text>] [--susurrar <name> <text>]
//!                                           [--esperar <seconds before speaking>]
//!                                           [--puerta x,y,gate ...] [--accion <AT_*>] [--baul] [--suelo]
//!
//! Connect Server → Game Server → login → list → (creates the character if missing) → enters →
//! walks 3 steps → (with a destination: requests it like the `M` window, waits for `MapChanged` and replies
//! `ClientReadyAfterMapChange`, or the notice if it cannot) → (each `--puerta`: stands at (x, y)
//! as the client does when stepping on it and asks to cross it with `EnterGateRequest`, like the destination) →
//! (waits, says, whispers and plays the requested animation, e.g. 128 = sit) → waits
//! showing what arrives (other players, chat, whispers, notices, animations) → exits.
//!
//! NPC dialogs: `--npc <number>` talks to the first NPC with that monster number that comes into
//! view; then each `--npc-paso <step>` is sent in order, one second apart:
//! `lista` (AvailableQuestsRequest), `buff` (NpcBuffRequest), `unirse:<gens>` (GensJoin), `dejar`
//! (GensLeave), `premio:<gens>` (GensReward), `mision:<quest hex>:<line>` (QuestSelect),
//! `estado:<quest>:<state>` (LegacyQuestStateSet), `cerrar` (CloseNpc).
//! `--hex` also prints the raw bytes of every packet that arrives; `--andar x,y` first walks in a
//! straight line to that tile (8 steps per `Walk`).

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
    /// `--hex`: print the raw bytes of what arrives.
    hex: bool,
    /// Every monster or NPC that came into view: (scope id, number, x, y).
    seen: Vec<(u16, u16, u8, u8)>,
}

impl Conn {
    fn recv(&mut self) -> std::io::Result<Option<ServerMessage>> {
        let mut chunk = [0u8; 4096];
        loop {
            if let Some(p) = self.buf.next_packet().map_err(|e| std::io::Error::other(format!("{:#04X}", e.0)))? {
                let plain = self.inp.decrypt(&p).map_err(|e| std::io::Error::other(format!("{e:?}")))?;
                if self.hex {
                    let bytes: Vec<String> = plain.iter().map(|b| format!("{b:02X}")).collect();
                    println!("  < {}", bytes.join(" "));
                }
                let m = ServerMessage::decode(&plain);
                if let Some(ServerMessage::AddMonstersToScope(list)) = &m {
                    self.seen.extend(list.iter().map(|s| (s.id & 0x7FFF, s.number, s.x, s.y)));
                }
                return Ok(m);
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

/// `--suelo`: drops and picks up ground items, printing every packet's bytes (to capture OpenMU's
/// ground item packets): picks up an id that does not exist, drops the first inventory grid item on
/// tile (0, 0) (not walkable), drops it on its own tile and picks it up, drops it 4 tiles east,
/// tries to pick it up from there, walks next to it and picks it up again.
fn ground_script(c: &mut Conn, user: &str, (x, y): (u8, u8)) -> std::io::Result<()> {
    c.sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let sent = |m: &ClientMessage| {
        let hex: Vec<String> = m.encode().iter().map(|b| format!("{b:02X}")).collect();
        println!("[{user}] -> {} = {m:?}", hex.join(" "));
    };
    let step = |c: &mut Conn, m: ClientMessage, secs: f32| -> std::io::Result<Vec<ServerMessage>> {
        sent(&m);
        c.send(m)?;
        c.collect(user, secs)
    };
    let first = c.collect(user, 2.0)?;
    // Not a box or a potion (OpenMU opens a box when it is dropped): a weapon or armor, also an
    // equipped one (it comes back to the grid and is put on again at the end).
    let item = first.iter().find_map(|m| match m {
        ServerMessage::InventoryList(list) => list.iter().find(|(s, b)| *s < 76 && (b[5] >> 4) < 12).copied(),
        _ => None,
    });
    let equipped = item.filter(|(s, _)| *s < 12);
    step(c, ClientMessage::PickUpItem { id: 0x7FFE }, 1.0)?;
    let Some((slot, _)) = item else {
        println!("[{user}] no item in the inventory grid to drop");
        return Ok(());
    };
    // The drop of our item at (tx, ty) among what arrived.
    let dropped_at = |msgs: &[ServerMessage], tx: u8, ty: u8| {
        msgs.iter().find_map(|m| match m {
            ServerMessage::ItemsDropped(list) => list.iter().find(|d| (d.x, d.y) == (tx, ty)).map(|d| d.id),
            _ => None,
        })
    };
    step(c, ClientMessage::DropItem { x: 0, y: 0, slot }, 1.0)?;
    let mut got = step(c, ClientMessage::DropItem { x, y, slot }, 1.5)?;
    if let Some(id) = dropped_at(&got, x, y) {
        got = step(c, ClientMessage::PickUpItem { id }, 1.5)?;
    }
    // Where it went back to (the server picks the slot).
    let slot = got
        .iter()
        .find_map(|m| match m {
            ServerMessage::ItemPickedUp { slot, .. } => Some(*slot),
            _ => None,
        })
        .unwrap_or(slot);
    let far = (x.saturating_add(4), y);
    let got = step(c, ClientMessage::DropItem { x: far.0, y: far.1, slot }, 1.5)?;
    if let Some(id) = dropped_at(&got, far.0, far.1) {
        step(c, ClientMessage::PickUpItem { id }, 1.5)?;
        // Two steps east: then it is 2 tiles away (OpenMU's `MaximumPickupDistance` is 3).
        let dirs: Vec<u8> = [(x, y), (x + 1, y), (x + 2, y)].windows(2).filter_map(|w| mu_protocol::game::direction_between(w[0], w[1])).collect();
        step(c, ClientMessage::Walk { source_x: x, source_y: y, rotation: dirs[0], directions: dirs }, 1.5)?;
        let got = step(c, ClientMessage::PickUpItem { id }, 1.5)?;
        if let (Some((to, bytes)), Some(from)) = (
            equipped,
            got.iter().find_map(|m| match m {
                ServerMessage::ItemPickedUp { slot, .. } => Some(*slot),
                _ => None,
            }),
        ) {
            use mu_protocol::game::storage::INVENTORY;
            step(c, ClientMessage::ItemMove { from_storage: INVENTORY, from_slot: from, item: bytes, to_storage: INVENTORY, to_slot: to }, 1.0)?;
        }
    }
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
    let mut npc: Option<u16> = None;
    let mut npc_steps: Vec<String> = Vec::new();
    let mut hex = false;
    let mut walk_to: Option<(u8, u8)> = None;
    let mut vault = false;
    let mut ground = false;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
            "--andar" if i + 1 < a.len() => {
                let v: Vec<u8> = a.remove(i + 1).split(',').filter_map(|n| n.trim().parse().ok()).collect();
                if let [x, y] = v[..] {
                    walk_to = Some((x, y));
                }
                a.remove(i);
            }
            "--npc" if i + 1 < a.len() => {
                npc = a.remove(i + 1).parse().ok();
                a.remove(i);
            }
            "--npc-paso" if i + 1 < a.len() => {
                npc_steps.push(a.remove(i + 1));
                a.remove(i);
            }
            "--hex" => {
                hex = true;
                a.remove(i);
            }
            "--baul" => {
                vault = true;
                a.remove(i);
            }
            "--suelo" => {
                ground = true;
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
    let mut c = Conn { sock, buf: PacketBuffer::default(), out: ClientCrypto::default(), inp: ClientCrypto::default(), hex, seen: Vec::new() };
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
    if ground {
        println!("[{user}] zen {}", info.money);
        return Ok(ground_script(&mut c, user, (info.x, info.y))?);
    }
    // Arrives right after the information (`ReceiveMoveMapChecksum`).
    // (Not always sent: without it only the `M` destination fails.)
    let mut key = match c.wait(|m| match m {
        ServerMessage::MapMoveChecksumKey { key } => Some(*key),
        _ => None,
    }) {
        Ok(key) => key,
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => 0,
        Err(e) => return Err(e.into()),
    };
    println!("[{user}] clave de mover {key:#010X}, zen {}", info.money);
    if walk_to.is_none() {
        c.send(ClientMessage::Walk { source_x: info.x, source_y: info.y, rotation: 3, directions: vec![3, 3, 3] })?;
    }
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
    if let Some((tx, ty)) = walk_to {
        // Straight toward the tile, 8 steps per `Walk` (a bit over a second each).
        let (mut x, mut y) = (info.x, info.y);
        c.sock.set_read_timeout(Some(Duration::from_millis(100)))?;
        while (x, y) != (tx, ty) {
            let mut dirs = Vec::new();
            let (sx, sy) = (x, y);
            while dirs.len() < 8 && (x, y) != (tx, ty) {
                let next = ((x as i16 + (tx as i16 - x as i16).signum()) as u8, (y as i16 + (ty as i16 - y as i16).signum()) as u8);
                dirs.extend(mu_protocol::game::direction_between((x, y), next));
                (x, y) = next;
            }
            let rotation = *dirs.last().unwrap_or(&0);
            c.send(ClientMessage::Walk { source_x: sx, source_y: sy, rotation, directions: dirs })?;
            let until = Instant::now() + Duration::from_millis(1300);
            while Instant::now() < until {
                match c.recv() {
                    Ok(_) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        println!("[{user}] camina hasta ({tx}, {ty})");
        c.sock.set_read_timeout(Some(Duration::from_secs(5)))?;
    }
    if let Some(number) = npc {
        // The NPC's scope id and tile, from the `AddMonstersToScope` that shows it.
        let found = |c: &Conn| c.seen.iter().find(|s| s.1 == number).map(|s| (s.0, s.2, s.3));
        let (id, x, y) = match found(&c) {
            Some(f) => f,
            None => c.wait(|m| match m {
                ServerMessage::AddMonstersToScope(list) => {
                    list.iter().find(|s| s.number == number).map(|s| (s.id & 0x7FFF, s.x, s.y))
                }
                _ => None,
            })?,
        };
        println!("[{user}] NPC {number}: id {id} en ({x}, {y})");
        // OpenMU does not check the distance to the NPC (`TalkNpcHandlerPlugInBase`).
        c.send(ClientMessage::TalkToNpc { id })?;
        println!("[{user}] habla con el NPC {id}");
        c.sock.set_read_timeout(Some(Duration::from_millis(200)))?;
        let mut steps = npc_steps.iter();
        let mut next = Instant::now() + Duration::from_secs(1);
        let until = Instant::now() + Duration::from_secs(1 + npc_steps.len() as u64 + 2);
        while Instant::now() < until {
            if Instant::now() >= next {
                next += Duration::from_secs(1);
                if let Some(step) = steps.next() {
                    let parts: Vec<&str> = step.split(':').collect();
                    let n = |i: usize| parts.get(i).and_then(|s| s.parse::<u8>().ok()).unwrap_or(0);
                    let msg = match parts[0] {
                        "lista" => Some(ClientMessage::AvailableQuestsRequest),
                        "buff" => Some(ClientMessage::NpcBuffRequest),
                        "unirse" => Some(ClientMessage::GensJoin { gens: n(1) }),
                        "dejar" => Some(ClientMessage::GensLeave),
                        "premio" => Some(ClientMessage::GensReward { gens: n(1) }),
                        "mision" => parts
                            .get(1)
                            .and_then(|q| u32::from_str_radix(q.trim_start_matches("0x"), 16).ok())
                            .map(|quest| ClientMessage::QuestSelect { quest, selected: n(2) }),
                        "estado" => Some(ClientMessage::LegacyQuestState { quest: n(1), state: n(2) }),
                        "cerrar" => Some(ClientMessage::CloseNpc),
                        _ => None,
                    };
                    match msg {
                        Some(m) => {
                            let bytes: Vec<String> = m.encode().iter().map(|b| format!("{b:02X}")).collect();
                            println!("[{user}] manda {m:?} ({})", bytes.join(" "));
                            c.send(m)?;
                        }
                        None => println!("[{user}] paso desconocido: {step}"),
                    }
                }
            }
            match c.recv() {
                Ok(Some(m)) => show(user, &m),
                Ok(None) => println!("  (paquete sin traducir)"),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {}
                Err(e) => return Err(e.into()),
            }
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
