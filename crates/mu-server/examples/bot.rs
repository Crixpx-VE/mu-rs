//! Cliente de prueba sin gráficos: hace lo mismo que el juego contra el servidor.
//!
//!   cargo run -p mu-server --example bot -- <cuenta> <contraseña> <personaje> [connect=127.0.0.1:44405] [segundos=3]
//!                                           [destino de M] [--decir <texto>] [--susurrar <nombre> <texto>]
//!                                           [--esperar <segundos antes de hablar>]
//!
//! Connect Server → Game Server → login → lista → (crea el personaje si no está) → entra →
//! camina 3 pasos → (con destino: lo pide como la ventana `M`, espera `MapChanged` y contesta
//! `ClientReadyAfterMapChange`, o el aviso si no puede) → (espera, dice y susurra lo que se le
//! pida) → espera mostrando lo que llega (otros jugadores, chat, susurros, avisos) → sale.

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

    fn send(&mut self, m: ClientMessage) -> std::io::Result<()> {
        self.sock.write_all(&self.out.encrypt(&m.encode()))
    }

    /// Espera un mensaje que cumpla `f` (los demás se enseñan).
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

/// Lo que llega, en claro para el chat y los avisos.
fn show(user: &str, m: &ServerMessage) {
    match m {
        ServerMessage::ChatMessage { sender, message } => println!("[{user}] CHAT {sender} : {message}"),
        ServerMessage::WhisperMessage { sender, message } => println!("[{user}] SUSURRO de {sender} : {message}"),
        ServerMessage::WhisperResult { success } => println!("[{user}] SUSURRO entregado: {success}"),
        ServerMessage::Notice { kind, text } => println!("[{user}] AVISO ({kind}) {text}"),
        m => println!("[{user}] llega {m:?}"),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    // Opciones con nombre (se quitan de las de posición).
    let mut say = None;
    let mut whisper = None;
    let mut pause = 0.0f32;
    let mut i = 0;
    while i < a.len() {
        match a[i].as_str() {
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
    // Llega justo después de la información (`ReceiveMoveMapChecksum`).
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
                // `SendRequestFinishLoading` cuando ha cargado.
                c.send(ClientMessage::ClientReadyAfterMapChange)?;
            }
            Err(text) => println!("[{user}] no puede mover: AVISO {text}"),
        }
    }
    c.sock.set_read_timeout(Some(Duration::from_millis(200)))?;
    let mut said = say.is_none() && whisper.is_none();
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
