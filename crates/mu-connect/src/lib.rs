//! MU Connect Server in Rust.
//!
//! It is the first server the client talks to:
//! 1. On connect, it greets the client (`Hello`).
//! 2. The client asks for the list of Game Servers and their load.
//! 3. The client picks one and asks for its IP and port, then connects there.
//!
//! No database needed: the server list comes from the configuration.

use std::sync::Arc;
use std::time::Duration;

use mu_protocol::connect::{ClientMessage, ServerLoad, ServerMessage};
use mu_protocol::packet::PacketBuffer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A Game Server the Connect Server can send players to.
#[derive(Debug, Clone)]
pub struct GameServer {
    pub id: u16,
    /// IP handed to the client (the one it must use to reach the Game Server).
    pub ip: String,
    pub port: u16,
    pub load: u8,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub servers: Vec<GameServer>,
    /// The connection is dropped if the client sends nothing for this long.
    pub idle_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Local OpenMU Game Server (its default port).
            servers: vec![GameServer { id: 0, ip: "127.0.0.1".into(), port: 55901, load: 0 }],
            idle_timeout: Duration::from_secs(120),
        }
    }
}

/// Accepts connections forever.
pub async fn serve(listener: TcpListener, config: Config) -> std::io::Result<()> {
    let config = Arc::new(config);
    loop {
        let (socket, addr) = listener.accept().await?;
        let config = config.clone();
        tokio::spawn(async move {
            match handle_client(socket, &config).await {
                Ok(()) => eprintln!("[connect] {addr} desconectado"),
                Err(e) => eprintln!("[connect] {addr} desconectado: {e}"),
            }
        });
    }
}

async fn handle_client(mut socket: TcpStream, config: &Config) -> std::io::Result<()> {
    socket.write_all(&ServerMessage::Hello.encode()).await?;

    let mut buffer = PacketBuffer::default();
    let mut chunk = [0u8; 1024];
    loop {
        let n = tokio::time::timeout(config.idle_timeout, socket.read(&mut chunk))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "sin actividad"))??;
        if n == 0 {
            return Ok(());
        }
        buffer.extend(&chunk[..n]);

        while let Some(packet) = buffer
            .next_packet()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("byte inválido {:#04X}", e.0)))?
        {
            let Some(msg) = ClientMessage::decode(&packet) else {
                eprintln!("[connect] paquete desconocido: {packet:02X?}");
                continue;
            };
            if let Some(reply) = respond(&msg, config) {
                socket.write_all(&reply.encode()).await?;
            }
        }
    }
}

/// Pure Connect Server logic: what to answer to each message.
pub fn respond(msg: &ClientMessage, config: &Config) -> Option<ServerMessage> {
    match msg {
        ClientMessage::ServerListRequest => Some(ServerMessage::ServerList(
            config.servers.iter().map(|s| ServerLoad { id: s.id, load: s.load }).collect(),
        )),
        ClientMessage::ConnectionInfoRequest { server_id } => config
            .servers
            .iter()
            .find(|s| s.id == *server_id)
            .map(|s| ServerMessage::ConnectionInfo { ip: s.ip.clone(), port: s.port }),
        ClientMessage::PatchCheckRequest { .. } => Some(ServerMessage::PatchVersionOkay),
    }
}
