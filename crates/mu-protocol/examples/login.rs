//! Prueba de login de punta a punta contra un servidor real (OpenMU o el nuestro).
//!
//!   cargo run -p mu-protocol --example login -- <usuario> <contraseña> [connect_server=127.0.0.1:44405]
//!
//! Hace lo mismo que el cliente: pide la lista al Connect Server, se conecta al primer
//! Game Server, espera el saludo, manda el login cifrado y muestra la respuesta.
//! (OpenMU crea por defecto cuentas de prueba como test0/test0 ... test9/test9.)

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use mu_protocol::connect;
use mu_protocol::crypto::ClientCrypto;
use mu_protocol::game::{self, LoginResult};
use mu_protocol::packet::PacketBuffer;

fn next_packet(sock: &mut TcpStream, buf: &mut PacketBuffer) -> std::io::Result<Vec<u8>> {
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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(user), Some(pass)) = (args.first(), args.get(1)) else {
        eprintln!("uso: login <usuario> <contraseña> [connect_server=127.0.0.1:44405]");
        std::process::exit(2);
    };
    let cs_addr = args.get(2).map(String::as_str).unwrap_or("127.0.0.1:44405");

    // 1. Connect Server
    println!("→ Connect Server {cs_addr}");
    let mut cs = TcpStream::connect(cs_addr)?;
    cs.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buf = PacketBuffer::default();
    println!("  {:?}", connect::ServerMessage::decode(&next_packet(&mut cs, &mut buf)?));
    cs.write_all(&connect::ClientMessage::ServerListRequest.encode())?;
    let Some(connect::ServerMessage::ServerList(list)) = connect::ServerMessage::decode(&next_packet(&mut cs, &mut buf)?)
    else {
        return Err("respuesta inesperada a la lista de servidores".into());
    };
    println!("  servidores: {list:?}");
    let first = list.first().ok_or("no hay servidores")?;
    cs.write_all(&connect::ClientMessage::ConnectionInfoRequest { server_id: first.id }.encode())?;
    let Some(connect::ServerMessage::ConnectionInfo { ip, port }) =
        connect::ServerMessage::decode(&next_packet(&mut cs, &mut buf)?)
    else {
        return Err("respuesta inesperada a la petición de conexión".into());
    };
    drop(cs);

    // 2. Game Server
    println!("→ Game Server {ip}:{port}");
    let mut gs = TcpStream::connect((ip.as_str(), port))?;
    gs.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut crypto = ClientCrypto::default();
    let mut buf = PacketBuffer::default();

    // Esperar el saludo (el servidor puede mandar otros paquetes antes; se ignoran).
    loop {
        let p = crypto.decrypt(&next_packet(&mut gs, &mut buf)?)?;
        if let Some(m @ game::ServerMessage::GameServerEntered { .. }) = game::ServerMessage::decode(&p) {
            println!("  {m:?}");
            break;
        }
    }

    // 3. Login
    let tick = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u32;
    gs.write_all(&crypto.encrypt(&game::ClientMessage::login(user, pass, tick).encode()))?;
    loop {
        let p = crypto.decrypt(&next_packet(&mut gs, &mut buf)?)?;
        if let Some(game::ServerMessage::LoginResponse(r)) = game::ServerMessage::decode(&p) {
            println!("  login: {r:?} ({})", r.message());
            if r == LoginResult::Okay {
                gs.write_all(&crypto.encrypt(&game::ClientMessage::RequestCharacterList { language: 0 }.encode()))?;
                let p = crypto.decrypt(&next_packet(&mut gs, &mut buf)?)?;
                println!("  lista de personajes (sin decodificar todavía): {} bytes, código {:02X?}", p.len(), &p[..p.len().min(5)]);
            }
            return Ok(());
        }
    }
}
