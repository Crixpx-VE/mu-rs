//! Uso:
//!   cargo run -p mu-connect -- [--listen 127.0.0.1:44405] [--server id=ip:puerto ...]
//!
//! Por defecto escucha solo en local y anuncia el Game Server de OpenMU en 127.0.0.1:55901.
//! Para que entren jugadores de fuera: `--listen 0.0.0.0:44405` y la IP pública en `--server`.

use mu_connect::{Config, GameServer};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut listen = "127.0.0.1:44405".to_string();
    let mut config = Config::default();
    let mut custom_servers = vec![];

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--listen" => listen = args.next().expect("--listen <ip:puerto>"),
            "--server" => custom_servers.push(parse_server(&args.next().expect("--server id=ip:puerto"))),
            otro => eprintln!("argumento desconocido: {otro}"),
        }
    }
    if !custom_servers.is_empty() {
        config.servers = custom_servers;
    }

    let listener = tokio::net::TcpListener::bind(&listen).await?;
    eprintln!("[connect] escuchando en {listen}");
    for s in &config.servers {
        eprintln!("[connect] servidor {} → {}:{}", s.id, s.ip, s.port);
    }
    mu_connect::serve(listener, config).await
}

fn parse_server(s: &str) -> GameServer {
    let (id, addr) = s.split_once('=').expect("formato: id=ip:puerto");
    let (ip, port) = addr.rsplit_once(':').expect("formato: id=ip:puerto");
    GameServer {
        id: id.parse().expect("id numérico"),
        ip: ip.to_string(),
        port: port.parse().expect("puerto numérico"),
        load: 0,
    }
}
