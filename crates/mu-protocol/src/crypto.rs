//! Cifrado completo de una conexión con el Game Server, para cada lado.
//!
//! - Cliente → servidor: Xor32 en **todos** los paquetes y, además, SimpleModulus en los C3/C4.
//! - Servidor → cliente: solo SimpleModulus en los C3/C4 (C1/C2 van en claro).
//!
//! Igual que `PipelinedEncryptor`/`PipelinedDecryptor` de OpenMU.

use crate::simple_modulus::{DecryptError, SimpleModulus, default_keys};
use crate::xor::{XOR32_KEY, xor32_decrypt, xor32_encrypt};

/// Lado del cliente: cifra lo que manda y descifra lo que recibe.
#[derive(Debug, Clone)]
pub struct ClientCrypto {
    encrypt: SimpleModulus,
    decrypt: SimpleModulus,
}

impl Default for ClientCrypto {
    fn default() -> Self {
        Self {
            encrypt: SimpleModulus::new(default_keys::CLIENT_ENCRYPT),
            decrypt: SimpleModulus::new(default_keys::CLIENT_DECRYPT),
        }
    }
}

impl ClientCrypto {
    /// Paquete en claro → bytes listos para mandar al servidor.
    pub fn encrypt(&mut self, packet: &[u8]) -> Vec<u8> {
        let mut p = packet.to_vec();
        xor32_encrypt(&mut p, &XOR32_KEY);
        self.encrypt.encrypt(&p)
    }

    /// Paquete completo recibido del servidor → en claro.
    pub fn decrypt(&mut self, packet: &[u8]) -> Result<Vec<u8>, DecryptError> {
        if packet.first().is_some_and(|&b| b >= 0xC3) {
            self.decrypt.decrypt(packet)
        } else {
            Ok(packet.to_vec())
        }
    }
}

/// Lado del servidor: descifra lo que manda el cliente y cifra las respuestas C3/C4.
#[derive(Debug, Clone)]
pub struct ServerCrypto {
    encrypt: SimpleModulus,
    decrypt: SimpleModulus,
}

impl Default for ServerCrypto {
    fn default() -> Self {
        Self {
            encrypt: SimpleModulus::new(default_keys::SERVER_ENCRYPT),
            decrypt: SimpleModulus::new(default_keys::SERVER_DECRYPT),
        }
    }
}

impl ServerCrypto {
    pub fn encrypt(&mut self, packet: &[u8]) -> Vec<u8> {
        self.encrypt.encrypt(packet)
    }

    pub fn decrypt(&mut self, packet: &[u8]) -> Result<Vec<u8>, DecryptError> {
        let mut p = if packet.first().is_some_and(|&b| b >= 0xC3) {
            self.decrypt.decrypt(packet)?
        } else {
            packet.to_vec()
        };
        xor32_decrypt(&mut p, &XOR32_KEY);
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversacion_completa() {
        let mut client = ClientCrypto::default();
        let mut server = ServerCrypto::default();
        let pedidos: [&[u8]; 3] = [
            &[0xC1, 0x05, 0xF3, 0x00, 0x00],
            &[0xC3, 0x08, 0xF1, 0x01, 1, 2, 3, 4],
            &[0xC1, 0x06, 0xD4, 0x10, 0x20, 0x30],
        ];
        for p in pedidos {
            assert_eq!(server.decrypt(&client.encrypt(p)).unwrap(), p);
        }
        let respuestas: [&[u8]; 2] = [&[0xC1, 0x05, 0xF1, 0x01, 0x01], &[0xC3, 0x06, 0xF3, 0x03, 9, 9]];
        for r in respuestas {
            assert_eq!(client.decrypt(&server.encrypt(r)).unwrap(), r);
        }
    }

    #[test]
    fn el_servidor_descifra_el_ping_real() {
        let encrypted = [
            0xC3, 0x18, 0xBC, 0x9D, 0x35, 0xE2, 0xA2, 0x21, 0x22, 0x91, 0x5C, 0x2B, 0x1E, 0x18, 0x63, 0x47, 0x28,
            0xC3, 0x10, 0x20, 0xF1, 0xC4, 0x2A, 0x14,
        ];
        let plain = ServerCrypto::default().decrypt(&encrypted).unwrap();
        assert_eq!(plain, [0xC3, 12, 0x0E, 0x00, 0x01, 0x33, 0xFE, 0x27, 0x00, 0x00, 0x00, 0x00]);
    }
}
