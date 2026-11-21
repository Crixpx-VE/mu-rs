//! Servidor de MU (Connect Server + Game Server en un solo programa).
//!
//!   mu-server [--data <Data del cliente>] [--db <cuentas.json>]
//!             [--connect 0.0.0.0:44405] [--game 0.0.0.0:55901] [--public-ip IP]
//!
//! El Connect Server da la lista con un solo Game Server, en `--public-ip` (la IP con la que los
//! clientes llegan a esta máquina; por defecto 127.0.0.1) y el puerto de `--game`.

mod classes;
mod db;
mod game;
mod spawn;

use std::path::PathBuf;
use std::sync::Arc;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let mut connect_addr = "0.0.0.0:44405".to_string();
    let mut game_addr = "0.0.0.0:55901".to_string();
    let mut public_ip = "127.0.0.1".to_string();
    let mut data = PathBuf::from("../dist/cliente/Data");
    let mut db_path = PathBuf::from("cuentas.json");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut value = || args.next().unwrap_or_else(|| panic!("falta el valor de {a}"));
        match a.as_str() {
            "--connect" => connect_addr = value(),
            "--game" => game_addr = value(),
            "--public-ip" => public_ip = value(),
            "--data" => data = PathBuf::from(value()),
            "--db" => db_path = PathBuf::from(value()),
            otro => eprintln!("argumento desconocido: {otro}"),
        }
    }
    let game_port: u16 = game_addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok()).expect("--game ip:puerto");

    let db = db::Db::open(&db_path)?;
    eprintln!("[db] {}", db_path.display());
    let shared = Arc::new(game::Shared::new(db, spawn::Spawns::load(&data)));

    let connect_listener = tokio::net::TcpListener::bind(&connect_addr).await?;
    let game_listener = tokio::net::TcpListener::bind(&game_addr).await?;
    eprintln!("[connect] escuchando en {connect_addr} → Game Server {public_ip}:{game_port}");
    eprintln!("[game] escuchando en {game_addr}");
    let config = mu_connect::Config {
        servers: vec![mu_connect::GameServer { id: 0, ip: public_ip, port: game_port, load: 0 }],
        ..Default::default()
    };
    tokio::select! {
        r = mu_connect::serve(connect_listener, config) => r,
        r = game::serve(game_listener, shared) => r,
    }
}
