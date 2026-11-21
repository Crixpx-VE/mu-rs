//! Connect Server de MU en Rust.
//!
//! Es el primer servidor al que habla el cliente:
//! 1. Al conectar, le saluda (`Hello`).
//! 2. El cliente pide la lista de Game Servers y su carga.
//! 3. El cliente elige uno y pide su IP y puerto; luego se conecta allí.
//!
//! No necesita base de datos: la lista de servidores viene de la configuración.

use std::sync::Arc;
use std::time::Duration;

use mu_protocol::connect::{ClientMessage, ServerLoad, ServerMessage};
use mu_protocol::packet::PacketBuffer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// Un Game Server al que el Connect Server puede mandar jugadores.
#[derive(Debug, Clone)]
pub struct GameServer {
    pub id: u16,
    /// IP que se le da al cliente (la que él debe usar para llegar al Game Server).
    pub ip: String,
    pub port: u16,
    pub load: u8,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub servers: Vec<GameServer>,
    /// Se corta la conexión si el cliente no manda nada en este tiempo.
    pub idle_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Game Server de OpenMU en local (su puerto por defecto).
            servers: vec![GameServer { id: 0, ip: "127.0.0.1".into(), port: 55901, load: 0 }],
            idle_timeout: Duration::from_secs(120),
        }
    }
}

/// Acepta conexiones para siempre.
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

/// Lógica pura del Connect Server: qué contestar a cada mensaje.
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
