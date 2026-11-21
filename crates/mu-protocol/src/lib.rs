//! Protocolo de red de MU Online (Season 6, contenido limitado a Season 3).
//!
//! Referencia: `referencias/OpenMU/src/Network/Packets/*.xml` (licencia MIT).

/// Cabeceras de paquete de MU. El primer byte indica el tipo de paquete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketHeader {
    /// Tamaño en 1 byte, sin cifrar.
    C1,
    /// Tamaño en 2 bytes, sin cifrar.
    C2,
    /// Tamaño en 1 byte, cifrado con SimpleModulus.
    C3,
    /// Tamaño en 2 bytes, cifrado con SimpleModulus.
    C4,
}

impl PacketHeader {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0xC1 => Some(Self::C1),
            0xC2 => Some(Self::C2),
            0xC3 => Some(Self::C3),
            0xC4 => Some(Self::C4),
            _ => None,
        }
    }

    /// Bytes que ocupa el campo de tamaño (1 para C1/C3, 2 para C2/C4).
    pub fn size_field_len(self) -> usize {
        match self {
            Self::C1 | Self::C3 => 1,
            Self::C2 | Self::C4 => 2,
        }
    }

    pub fn is_encrypted(self) -> bool {
        matches!(self, Self::C3 | Self::C4)
    }
}

/// Lee el tamaño total de un paquete a partir de sus primeros bytes.
/// Devuelve `None` si faltan bytes o la cabecera no es válida.
pub fn packet_len(buf: &[u8]) -> Option<usize> {
    let header = PacketHeader::from_byte(*buf.first()?)?;
    match header.size_field_len() {
        1 => buf.get(1).map(|&n| n as usize),
        _ => Some(u16::from_be_bytes([*buf.get(1)?, *buf.get(2)?]) as usize),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_tamano_c1_y_c2() {
        assert_eq!(packet_len(&[0xC1, 0x04, 0xF1, 0x00]), Some(4));
        assert_eq!(packet_len(&[0xC2, 0x01, 0x00, 0xF4]), Some(256));
        assert_eq!(packet_len(&[0xAA, 0x04]), None);
        assert_eq!(packet_len(&[0xC2, 0x01]), None);
    }
}
