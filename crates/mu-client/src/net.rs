//! Server connection (Connect Server and Game Server), on separate threads so the game doesn't stall.
//!
//! - **Address**: `juego=host:port` from `mu-launcher.txt`; if missing, the same host as
//!   `servidor=` (the updates one) on port 44405; failing that, 127.0.0.1:44405.
//! - **Connect Server**: connects, asks for the list and, when a server is chosen, its address;
//!   then closes (like the original).
//! - **Game Server**: a thread reads, decrypts and decodes (`FromServer`); outgoing messages are
//!   encrypted on the main thread and written directly.
//!
//! Systems read `FromServer` / `NetStatus` and send with `Net::send`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use bevy::prelude::*;
use mu_protocol::connect;
use mu_protocol::crypto::ClientCrypto;
use mu_protocol::game::{ClientMessage, ServerMessage};
use mu_protocol::packet::PacketBuffer;

pub struct NetPlugin;

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = channel();
        app.insert_resource(Net { events_tx: tx, events: Mutex::new(rx), game: None, generation: 0, cs_busy: false })
            .add_message::<FromServer>()
            .add_message::<NetStatus>()
            .add_systems(PreUpdate, pump.in_set(NetSystems));
    }
}

/// Where incoming messages are dispatched (in `PreUpdate`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetSystems;

/// A message from the Game Server.
#[derive(Message, Clone, Debug)]
pub struct FromServer(pub ServerMessage);

/// Connection events.
#[derive(Message, Clone, Debug)]
pub enum NetStatus {
    /// Connect Server list: (id, load %).
    ServerList(Vec<(u16, u8)>),
    /// Could not talk to the Connect Server.
    ConnectFailed(String),
    /// Connected to the Game Server (`GameServerEntered` will follow).
    GameConnected,
    /// Could not connect to the Game Server, or it was dropped.
    GameLost(String),
}

/// What the threads send, tagged with the Game Server connection number it belongs to (data from
/// an already closed connection is discarded).
enum Event {
    Status(u32, NetStatus),
    Message(u32, ServerMessage),
    Connected(u32, TcpStream),
}

struct GameConn {
    stream: TcpStream,
    crypto: ClientCrypto,
}

#[derive(Resource)]
pub struct Net {
    events_tx: Sender<Event>,
    events: Mutex<Receiver<Event>>,
    game: Option<GameConn>,
    /// Number of the current Game Server connection.
    generation: u32,
    /// A Connect Server request is in progress.
    cs_busy: bool,
}

/// Connect Server address.
pub fn connect_server_address() -> String {
    if let Some(a) = crate::settings::saved_value("juego").filter(|a| !a.is_empty()) {
        return a;
    }
    let host = crate::settings::saved_value("servidor").and_then(|url| {
        let rest = url.split("://").nth(1).unwrap_or(&url).to_string();
        let host = rest.split(['/', ':']).next()?.to_string();
        (!host.is_empty()).then_some(host)
    });
    format!("{}:44405", host.unwrap_or_else(|| "127.0.0.1".into()))
}

fn resolve(addr: &str) -> std::io::Result<SocketAddr> {
    addr.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other(format!("no se encuentra {addr}")))
}

fn read_packet(sock: &mut TcpStream, buf: &mut PacketBuffer) -> std::io::Result<Vec<u8>> {
    let mut chunk = [0u8; 4096];
    loop {
        if let Some(p) = buf.next_packet().map_err(|e| std::io::Error::other(format!("byte inválido {:#04X}", e.0)))? {
            return Ok(p);
        }
        let n = sock.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::other("el servidor cerró la conexión"));
        }
        buf.extend(&chunk[..n]);
    }
}

/// A short exchange with the Connect Server: handshake, then `request` and its reply.
fn ask_connect_server(request: connect::ClientMessage) -> std::io::Result<connect::ServerMessage> {
    let addr = resolve(&connect_server_address())?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(5))?;
    s.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buf = PacketBuffer::default();
    // Handshake.
    read_packet(&mut s, &mut buf)?;
    s.write_all(&request.encode())?;
    loop {
        let p = read_packet(&mut s, &mut buf)?;
        if let Some(m) = connect::ServerMessage::decode(&p) {
            if !matches!(m, connect::ServerMessage::Hello) {
                return Ok(m);
            }
        }
    }
}

impl Net {
    /// `SendRequestServerList`: the list arrives as `NetStatus::ServerList`.
    pub fn request_server_list(&mut self) {
        if self.cs_busy {
            return;
        }
        self.cs_busy = true;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let ev = match ask_connect_server(connect::ClientMessage::ServerListRequest) {
                Ok(connect::ServerMessage::ServerList(list)) => {
                    NetStatus::ServerList(list.iter().map(|s| (s.id, s.load)).collect())
                }
                Ok(other) => NetStatus::ConnectFailed(format!("respuesta inesperada: {other:?}")),
                Err(e) => NetStatus::ConnectFailed(e.to_string()),
            };
            let _ = tx.send(Event::Status(0, ev));
        });
    }

    /// `SendRequestServerAddress` and connect to the Game Server it returns.
    pub fn connect_to_server(&mut self, server_id: u16) {
        self.close();
        self.generation += 1;
        let generation = self.generation;
        let tx = self.events_tx.clone();
        std::thread::spawn(move || {
            let lost = |why: String| Event::Status(generation, NetStatus::GameLost(why));
            let addr = match ask_connect_server(connect::ClientMessage::ConnectionInfoRequest { server_id }) {
                Ok(connect::ServerMessage::ConnectionInfo { ip, port }) => format!("{ip}:{port}"),
                Ok(other) => {
                    let _ = tx.send(lost(format!("respuesta inesperada: {other:?}")));
                    return;
                }
                Err(e) => {
                    let _ = tx.send(lost(e.to_string()));
                    return;
                }
            };
            let stream = match resolve(&addr).and_then(|a| TcpStream::connect_timeout(&a, Duration::from_secs(5))) {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(lost(format!("{addr}: {e}")));
                    return;
                }
            };
            let _ = stream.set_nodelay(true);
            let Ok(reader) = stream.try_clone() else { return };
            if tx.send(Event::Connected(generation, stream)).is_err() {
                return;
            }
            read_game(reader, tx, generation);
        });
    }

    /// Sends a message to the Game Server (if connected).
    pub fn send(&mut self, msg: ClientMessage) {
        let Some(conn) = &mut self.game else {
            warn!("sin conexión: no se manda {msg:?}");
            return;
        };
        let wire = conn.crypto.encrypt(&msg.encode());
        if let Err(e) = conn.stream.write_all(&wire) {
            warn!("no se pudo mandar al servidor: {e}");
        }
    }

    pub fn connected(&self) -> bool {
        self.game.is_some()
    }

    /// Without a connection (for tests: what is sent goes nowhere).
    #[cfg(test)]
    pub(crate) fn offline() -> Self {
        let (tx, rx) = channel();
        Net { events_tx: tx, events: Mutex::new(rx), game: None, generation: 0, cs_busy: false }
    }

    /// Closes the Game Server connection (anything still arriving from it is discarded).
    pub fn close(&mut self) {
        self.generation += 1;
        if let Some(conn) = self.game.take() {
            let _ = conn.stream.shutdown(std::net::Shutdown::Both);
        }
    }
}

/// Game Server reader thread.
fn read_game(mut stream: TcpStream, tx: Sender<Event>, generation: u32) {
    let mut crypto = ClientCrypto::default();
    let mut buf = PacketBuffer::default();
    loop {
        let packet = match read_packet(&mut stream, &mut buf) {
            Ok(p) => p,
            Err(e) => {
                let _ = tx.send(Event::Status(generation, NetStatus::GameLost(e.to_string())));
                return;
            }
        };
        let plain = match crypto.decrypt(&packet) {
            Ok(p) => p,
            Err(e) => {
                warn!("paquete del servidor sin descifrar: {e:?}");
                continue;
            }
        };
        match ServerMessage::decode(&plain) {
            Some(m) => {
                if tx.send(Event::Message(generation, m)).is_err() {
                    return;
                }
            }
            None => debug!("paquete del servidor sin traducir: {:02X?}", &plain[..plain.len().min(6)]),
        }
    }
}

/// Turns what arrived from the threads into Bevy messages.
fn pump(mut net: ResMut<Net>, mut from: MessageWriter<FromServer>, mut status: MessageWriter<NetStatus>) {
    let mut events = Vec::new();
    if let Ok(rx) = net.events.lock() {
        while let Ok(e) = rx.try_recv() {
            events.push(e);
        }
    }
    for e in events {
        let current = |g: u32| g == 0 || g == net.generation;
        match e {
            Event::Connected(g, stream) if current(g) => {
                net.game = Some(GameConn { stream, crypto: ClientCrypto::default() });
                status.write(NetStatus::GameConnected);
            }
            Event::Message(g, m) if current(g) => {
                from.write(FromServer(m));
            }
            Event::Status(g, s) if current(g) => {
                match &s {
                    NetStatus::ServerList(_) | NetStatus::ConnectFailed(_) => net.cs_busy = false,
                    NetStatus::GameLost(why) => {
                        warn!("conexión con el servidor perdida: {why}");
                        net.game = None;
                    }
                    NetStatus::GameConnected => {}
                }
                status.write(s);
            }
            // From an already closed connection.
            _ => {}
        }
    }
}
