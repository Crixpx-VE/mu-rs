//! Paquetes del Game Server: por ahora, la entrada y el login.
//!
//! ```text
//! servidor → cliente  GameServerEntered     C1 0C F1 00 <ok u8> <id jugador u16 BE> <versión 5>
//! cliente  → servidor Login                 C3 3C F1 01 <usuario 10, Xor3> <contraseña 20, Xor3>
//!                                                       <tick u32 BE> <versión 5> <serial 16>
//! servidor → cliente  LoginResponse         C1 05 F1 01 <resultado>
//! cliente  → servidor RequestCharacterList  C1 05 F3 00 <idioma>
//! cliente  → servidor SelectCharacter       C1 0E F3 03 <nombre 10>
//! ```
//!
//! Formatos sacados de `OpenMU/src/Network/Packets/*.xml` (licencia MIT).

use crate::packet::{Packet, c1};
use crate::xor::xor3;

/// Versión y serial del cliente Season 6 Ep3 que acepta OpenMU por defecto ("1.04d", GMO).
pub const CLIENT_VERSION: [u8; 5] = *b"10404";
pub const CLIENT_SERIAL: [u8; 16] = *b"k1Pk2jcET48mxL3b";

/// Resultado del login (`LoginResult` en OpenMU).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginResult {
    InvalidPassword,
    Okay,
    AccountInvalid,
    AccountAlreadyConnected,
    ServerIsFull,
    AccountBlocked,
    WrongVersion,
    ConnectionError,
    ConnectionClosed3Fails,
    TemporaryBlocked,
    Other(u8),
}

impl LoginResult {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0 => Self::InvalidPassword,
            1 => Self::Okay,
            2 => Self::AccountInvalid,
            3 => Self::AccountAlreadyConnected,
            4 => Self::ServerIsFull,
            5 => Self::AccountBlocked,
            6 => Self::WrongVersion,
            7 => Self::ConnectionError,
            8 => Self::ConnectionClosed3Fails,
            14 => Self::TemporaryBlocked,
            o => Self::Other(o),
        }
    }

    pub fn to_byte(self) -> u8 {
        match self {
            Self::InvalidPassword => 0,
            Self::Okay => 1,
            Self::AccountInvalid => 2,
            Self::AccountAlreadyConnected => 3,
            Self::ServerIsFull => 4,
            Self::AccountBlocked => 5,
            Self::WrongVersion => 6,
            Self::ConnectionError => 7,
            Self::ConnectionClosed3Fails => 8,
            Self::TemporaryBlocked => 14,
            Self::Other(o) => o,
        }
    }

    /// Texto para mostrar al jugador.
    pub fn message(self) -> &'static str {
        match self {
            Self::InvalidPassword => "Contraseña incorrecta",
            Self::Okay => "Conectado",
            Self::AccountInvalid => "La cuenta no existe",
            Self::AccountAlreadyConnected => "La cuenta ya está conectada",
            Self::ServerIsFull => "El servidor está lleno",
            Self::AccountBlocked => "La cuenta está bloqueada",
            Self::WrongVersion => "Versión del cliente incorrecta",
            Self::ConnectionError => "Error de conexión",
            Self::ConnectionClosed3Fails => "Conexión cerrada tras 3 intentos fallidos",
            Self::TemporaryBlocked => "La cuenta está bloqueada temporalmente",
            Self::Other(_) => "Error desconocido",
        }
    }
}

/// Lo que el cliente manda al Game Server (en claro; cifrar con `crypto::ClientCrypto`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Login { username: String, password: String, tick_count: u32, version: [u8; 5], serial: [u8; 16] },
    RequestCharacterList { language: u8 },
    SelectCharacter { name: String },
}

/// Lo que el Game Server manda al cliente (ya descifrado).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServerMessage {
    GameServerEntered { success: bool, player_id: u16, version: [u8; 5] },
    LoginResponse(LoginResult),
}

fn fixed<const N: usize>(s: &str) -> [u8; N] {
    let mut out = [0u8; N];
    let n = s.len().min(N);
    out[..n].copy_from_slice(&s.as_bytes()[..n]);
    out
}

fn unfixed(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

impl ClientMessage {
    pub fn login(username: &str, password: &str, tick_count: u32) -> Self {
        Self::Login {
            username: username.into(),
            password: password.into(),
            tick_count,
            version: CLIENT_VERSION,
            serial: CLIENT_SERIAL,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Login { username, password, tick_count, version, serial } => {
                let mut user = fixed::<10>(username);
                let mut pass = fixed::<20>(password);
                xor3(&mut user);
                xor3(&mut pass);
                let mut body = vec![0xF1, 0x01];
                body.extend_from_slice(&user);
                body.extend_from_slice(&pass);
                body.extend_from_slice(&tick_count.to_be_bytes());
                body.extend_from_slice(version);
                body.extend_from_slice(serial);
                body.push(0); // relleno: el paquete original mide 60 (el serial acaba en el 59)
                let mut p = c1(&body);
                p[0] = 0xC3; // se manda cifrado
                p
            }
            Self::RequestCharacterList { language } => c1(&[0xF3, 0x00, *language]),
            Self::SelectCharacter { name } => {
                let mut body = vec![0xF3, 0x03];
                body.extend_from_slice(&fixed::<10>(name));
                c1(&body)
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let b = p.body();
        match (p.code(), p.sub_code()?) {
            (0xF1, 0x01) if b.len() >= 2 + 10 + 20 + 4 + 5 + 16 => {
                let mut user: [u8; 10] = b[2..12].try_into().ok()?;
                let mut pass: [u8; 20] = b[12..32].try_into().ok()?;
                xor3(&mut user);
                xor3(&mut pass);
                Some(Self::Login {
                    username: unfixed(&user),
                    password: unfixed(&pass),
                    tick_count: u32::from_be_bytes(b[32..36].try_into().ok()?),
                    version: b[36..41].try_into().ok()?,
                    serial: b[41..57].try_into().ok()?,
                })
            }
            (0xF3, 0x00) => Some(Self::RequestCharacterList { language: *b.get(2).unwrap_or(&0) }),
            (0xF3, 0x03) => Some(Self::SelectCharacter { name: unfixed(b.get(2..12)?) }),
            _ => None,
        }
    }
}

impl ServerMessage {
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::GameServerEntered { success, player_id, version } => {
                let mut body = vec![0xF1, 0x00, *success as u8];
                body.extend_from_slice(&player_id.to_be_bytes());
                body.extend_from_slice(version);
                c1(&body)
            }
            Self::LoginResponse(r) => c1(&[0xF1, 0x01, r.to_byte()]),
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let p = Packet::parse(bytes)?;
        let b = p.body();
        match (p.code(), p.sub_code()?) {
            (0xF1, 0x00) => Some(Self::GameServerEntered {
                success: *b.get(2)? != 0,
                player_id: u16::from_be_bytes([*b.get(3)?, *b.get(4)?]),
                version: b.get(5..10)?.try_into().ok()?,
            }),
            (0xF1, 0x01) => Some(Self::LoginResponse(LoginResult::from_byte(*b.get(2)?))),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{ClientCrypto, ServerCrypto};

    #[test]
    fn tamanos_como_en_openmu() {
        let login = ClientMessage::login("test", "secreto", 12345).encode();
        assert_eq!(login.len(), 60, "LoginLongPassword mide 60");
        assert_eq!(&login[..4], &[0xC3, 60, 0xF1, 0x01]);
        // Usuario en el índice 4, contraseña en el 14, tick en el 34, versión en el 38, serial en el 43.
        assert_eq!(&login[38..43], b"10404");
        assert_eq!(&login[43..59], b"k1Pk2jcET48mxL3b");
        assert_eq!(u32::from_be_bytes(login[34..38].try_into().unwrap()), 12345);
        assert_ne!(&login[4..8], b"test", "el usuario va con Xor3");

        assert_eq!(ClientMessage::RequestCharacterList { language: 0 }.encode().len(), 5);
        assert_eq!(ClientMessage::SelectCharacter { name: "Pepe".into() }.encode().len(), 14);
        let entered = ServerMessage::GameServerEntered { success: true, player_id: 0x1234, version: CLIENT_VERSION };
        assert_eq!(entered.encode().len(), 12);
    }

    #[test]
    fn login_cifrado_de_punta_a_punta() {
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();

        let msg = ClientMessage::login("jugador", "contraseña", 987654);
        let wire = client.encrypt(&msg.encode());
        assert_eq!(wire[0], 0xC3);
        let plain = server.decrypt(&wire).unwrap();
        assert_eq!(ClientMessage::decode(&plain), Some(msg));

        let reply = ServerMessage::LoginResponse(LoginResult::Okay);
        assert_eq!(ServerMessage::decode(&client.decrypt(&server.encrypt(&reply.encode())).unwrap()), Some(reply));

        let list = ClientMessage::RequestCharacterList { language: 0 };
        assert_eq!(ClientMessage::decode(&server.decrypt(&client.encrypt(&list.encode())).unwrap()), Some(list));
    }
}
