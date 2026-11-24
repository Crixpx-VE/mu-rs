//! End-to-end test: a real TCP client talks to the Connect Server.

use std::time::Duration;

use mu_connect::{Config, GameServer};
use mu_protocol::connect::{ClientMessage, ServerLoad, ServerMessage};
use mu_protocol::packet::PacketBuffer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

async fn recibir(sock: &mut TcpStream, buf: &mut PacketBuffer) -> ServerMessage {
    let mut chunk = [0u8; 512];
    loop {
        if let Some(p) = buf.next_packet().unwrap() {
            return ServerMessage::decode(&p).expect("paquete del servidor válido");
        }
        let n = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut chunk)).await.unwrap().unwrap();
        assert!(n > 0, "el servidor cerró la conexión");
        buf.extend(&chunk[..n]);
    }
}

#[tokio::test]
async fn lista_de_servidores_y_conexion() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let config = Config {
        servers: vec![
            GameServer { id: 0, ip: "127.0.0.1".into(), port: 55901, load: 10 },
            GameServer { id: 1, ip: "10.0.0.5".into(), port: 55902, load: 80 },
        ],
        ..Config::default()
    };
    tokio::spawn(mu_connect::serve(listener, config));

    let mut sock = TcpStream::connect(addr).await.unwrap();
    let mut buf = PacketBuffer::default();

    assert_eq!(recibir(&mut sock, &mut buf).await, ServerMessage::Hello);

    sock.write_all(&ClientMessage::ServerListRequest.encode()).await.unwrap();
    assert_eq!(
        recibir(&mut sock, &mut buf).await,
        ServerMessage::ServerList(vec![ServerLoad { id: 0, load: 10 }, ServerLoad { id: 1, load: 80 }])
    );

    // Two requests glued into one send: the server must split them.
    let mut both = ClientMessage::ConnectionInfoRequest { server_id: 1 }.encode();
    both.extend(ClientMessage::ConnectionInfoRequest { server_id: 0 }.encode());
    sock.write_all(&both).await.unwrap();
    assert_eq!(
        recibir(&mut sock, &mut buf).await,
        ServerMessage::ConnectionInfo { ip: "10.0.0.5".into(), port: 55902 }
    );
    assert_eq!(
        recibir(&mut sock, &mut buf).await,
        ServerMessage::ConnectionInfo { ip: "127.0.0.1".into(), port: 55901 }
    );
}

#[tokio::test]
async fn corta_si_recibe_basura() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(mu_connect::serve(listener, Config::default()));

    let mut sock = TcpStream::connect(addr).await.unwrap();
    let mut buf = PacketBuffer::default();
    assert_eq!(recibir(&mut sock, &mut buf).await, ServerMessage::Hello);

    sock.write_all(b"GET / HTTP/1.1\r\n\r\n").await.unwrap();
    let mut chunk = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(5), sock.read(&mut chunk)).await.unwrap().unwrap_or(0);
    assert_eq!(n, 0, "debería cerrar la conexión");
}
