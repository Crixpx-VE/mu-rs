//! Connect Server packets (port 44405 by default).
//!
//! It is the first thing the client contacts: it asks for the server list and then
//! the IP/port of the chosen Game Server. Unencrypted.
//!
//! ```text
//! server → client   Hello                C1 04 00 01
//! client → server   ServerListRequest    C1 04 F4 06
//! server → client   ServerListResponse   C2 LL LL F4 06 <n u16 BE> [id u16 LE, load u8, 0]...
//! client → server   ConnectionInfoReq    C1 06 F4 03 <id u16 LE>
//! server → client   ConnectionInfo       C1 16 F4 03 <ip 16 bytes> <port u16 LE>
//! ```

use crate::packet::{Packet, c1, c2};

/// A Game Server in the list, with its load (0-100 %).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerLoad {
    pub id: u16,
    pub load: u8,
}

/// What the client sends to the Connect Server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    ServerListRequest,
    ConnectionInfoRequest { server_id: u16 },
    /// Version check done by launchers.
    PatchCheckRequest { major: u8, minor: u8, patch: u8 },
}

/// What the Connect Server sends to the client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    Hello,
    ServerList(Vec<ServerLoad>),
    ConnectionInfo { ip: String, port: u16 },
    PatchVersionOkay,
}

impl ClientMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::ServerListRequest => c1(&[0xF4, 0x06]),
            Self::ConnectionInfoRequest { server_id } => {
                let id = server_id.to_le_bytes();
                c1(&[0xF4, 0x03, id[0], id[1]])
            }
            Self::PatchCheckRequest { major, minor, patch } => c1(&[0x02, *major, *minor, *patch]),
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let b = p.body();
        match (p.code(), p.sub_code()) {
            (0xF4, Some(0x06)) => Some(Self::ServerListRequest),
            (0xF4, Some(0x03)) => {
                // 0.75 clients send the id in 1 byte; modern ones in 2.
                let server_id = match b.len() {
                    3 => b[2] as u16,
                    _ => u16::from_le_bytes([b[2], *b.get(3)?]),
                };
                Some(Self::ConnectionInfoRequest { server_id })
            }
            (0x02, _) if b.len() >= 4 => {
                Some(Self::PatchCheckRequest { major: b[1], minor: b[2], patch: b[3] })
            }
            _ => None,
        }
    }
}

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Hello => c1(&[0x00, 0x01]),
            Self::PatchVersionOkay => c1(&[0x02]),
            Self::ServerList(servers) => {
                let mut body = vec![0xF4, 0x06];
                body.extend_from_slice(&(servers.len() as u16).to_be_bytes());
                for s in servers {
                    body.extend_from_slice(&s.id.to_le_bytes());
                    body.push(s.load);
                    body.push(0);
                }
                c2(&body)
            }
            Self::ConnectionInfo { ip, port } => {
                let mut body = vec![0xF4, 0x03];
                let mut ip_field = [0u8; 16];
                let n = ip.len().min(15);
                ip_field[..n].copy_from_slice(&ip.as_bytes()[..n]);
                body.extend_from_slice(&ip_field);
                body.extend_from_slice(&port.to_le_bytes());
                c1(&body)
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let b = p.body();
        match (p.code(), p.sub_code()) {
            (0x00, Some(0x01)) => Some(Self::Hello),
            (0x02, _) => Some(Self::PatchVersionOkay),
            (0xF4, Some(0x06)) => {
                let count = u16::from_be_bytes([*b.get(2)?, *b.get(3)?]) as usize;
                let list = b.get(4..4 + count * 4)?;
                Some(Self::ServerList(
                    list.chunks_exact(4)
                        .map(|s| ServerLoad { id: u16::from_le_bytes([s[0], s[1]]), load: s[2] })
                        .collect(),
                ))
            }
            (0xF4, Some(0x03)) => {
                let ip_field = b.get(2..18)?;
                let end = ip_field.iter().position(|&c| c == 0).unwrap_or(16);
                let ip = String::from_utf8_lossy(&ip_field[..end]).into_owned();
                let port = u16::from_le_bytes([*b.get(18)?, *b.get(19)?]);
                Some(Self::ConnectionInfo { ip, port })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_como_los_documenta_openmu() {
        assert_eq!(ServerMessage::Hello.encode(), [0xC1, 0x04, 0x00, 0x01]);
        assert_eq!(ClientMessage::ServerListRequest.encode(), [0xC1, 0x04, 0xF4, 0x06]);
        assert_eq!(
            ClientMessage::ConnectionInfoRequest { server_id: 0x0102 }.encode(),
            [0xC1, 0x06, 0xF4, 0x03, 0x02, 0x01]
        );
        let info = ServerMessage::ConnectionInfo { ip: "127.0.0.1".into(), port: 55901 }.encode();
        assert_eq!(info.len(), 22);
        assert_eq!(&info[..4], &[0xC1, 0x16, 0xF4, 0x03]);
        assert_eq!(&info[20..], &55901u16.to_le_bytes());
    }

    #[test]
    fn ida_y_vuelta() {
        let msgs = [
            ServerMessage::Hello,
            ServerMessage::PatchVersionOkay,
            ServerMessage::ServerList(vec![ServerLoad { id: 0, load: 12 }, ServerLoad { id: 20, load: 100 }]),
            ServerMessage::ConnectionInfo { ip: "192.168.1.10".into(), port: 55901 },
        ];
        for m in msgs {
            assert_eq!(ServerMessage::decode(&m.encode()), Some(m));
        }
        for m in [
            ClientMessage::ServerListRequest,
            ClientMessage::ConnectionInfoRequest { server_id: 3 },
            ClientMessage::PatchCheckRequest { major: 1, minor: 4, patch: 4 },
        ] {
            assert_eq!(ClientMessage::decode(&m.encode()), Some(m));
        }
        // Old format (0.75): 1-byte id.
        assert_eq!(
            ClientMessage::decode(&[0xC1, 0x05, 0xF4, 0x03, 0x07]),
            Some(ClientMessage::ConnectionInfoRequest { server_id: 7 })
        );
    }
}
