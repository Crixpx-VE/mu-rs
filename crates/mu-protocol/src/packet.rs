//! Formato básico de los paquetes.
//!
//! Todo paquete empieza por una cabecera:
//! - `C1 <tamaño u8> <código> ...`       (C3 igual pero cifrado)
//! - `C2 <tamaño u16 BE> <código> ...`   (C4 igual pero cifrado)
//!
//! Muchos paquetes llevan además un subcódigo justo después del código.

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

    /// Bytes de cabecera antes del código: tipo + tamaño.
    pub fn header_len(self) -> usize {
        1 + self.size_field_len()
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

/// Vista de un paquete completo ya recibido.
#[derive(Debug, Clone, Copy)]
pub struct Packet<'a> {
    pub header: PacketHeader,
    pub bytes: &'a [u8],
}

impl<'a> Packet<'a> {
    /// Interpreta `bytes` como un paquete completo (comprueba cabecera y tamaño).
    pub fn parse(bytes: &'a [u8]) -> Option<Self> {
        let header = PacketHeader::from_byte(*bytes.first()?)?;
        let len = packet_len(bytes)?;
        if len != bytes.len() || len <= header.header_len() {
            return None;
        }
        Some(Self { header, bytes })
    }

    pub fn code(&self) -> u8 {
        self.bytes[self.header.header_len()]
    }

    /// Byte siguiente al código (si existe).
    pub fn sub_code(&self) -> Option<u8> {
        self.bytes.get(self.header.header_len() + 1).copied()
    }

    /// Todo lo que va después de la cabecera de tamaño (código incluido).
    pub fn body(&self) -> &'a [u8] {
        &self.bytes[self.header.header_len()..]
    }
}

/// Construye un paquete C1 con `body` = código, subcódigo y datos.
pub fn c1(body: &[u8]) -> Vec<u8> {
    let len = body.len() + 2;
    assert!(len <= 255, "paquete C1 demasiado grande ({len} bytes), usa C2");
    let mut out = Vec::with_capacity(len);
    out.push(0xC1);
    out.push(len as u8);
    out.extend_from_slice(body);
    out
}

/// Construye un paquete C2 con `body` = código, subcódigo y datos.
pub fn c2(body: &[u8]) -> Vec<u8> {
    let len = body.len() + 3;
    assert!(len <= u16::MAX as usize, "paquete demasiado grande ({len} bytes)");
    let mut out = Vec::with_capacity(len);
    out.push(0xC2);
    out.extend_from_slice(&(len as u16).to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// Acumula bytes de un socket y va sacando paquetes completos.
#[derive(Debug, Default)]
pub struct PacketBuffer {
    buf: Vec<u8>,
}

/// El otro extremo mandó algo que no es un paquete de MU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidData(pub u8);

impl PacketBuffer {
    pub fn extend(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Saca el siguiente paquete completo, si ya ha llegado entero.
    pub fn next_packet(&mut self) -> Result<Option<Vec<u8>>, InvalidData> {
        let Some(&first) = self.buf.first() else { return Ok(None) };
        let header = PacketHeader::from_byte(first).ok_or(InvalidData(first))?;
        let Some(len) = packet_len(&self.buf) else { return Ok(None) };
        if len <= header.header_len() {
            return Err(InvalidData(first));
        }
        if self.buf.len() < len {
            return Ok(None);
        }
        Ok(Some(self.buf.drain(..len).collect()))
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

    #[test]
    fn construye_paquetes() {
        assert_eq!(c1(&[0x00, 0x01]), [0xC1, 0x04, 0x00, 0x01]);
        assert_eq!(c2(&[0xF4, 0x06]), [0xC2, 0x00, 0x05, 0xF4, 0x06]);
    }

    #[test]
    fn buffer_separa_paquetes_partidos() {
        let mut b = PacketBuffer::default();
        b.extend(&[0xC1, 0x04, 0x00]);
        assert_eq!(b.next_packet(), Ok(None));
        b.extend(&[0x01, 0xC1, 0x04, 0xF4, 0x06, 0xC1]);
        assert_eq!(b.next_packet(), Ok(Some(vec![0xC1, 0x04, 0x00, 0x01])));
        assert_eq!(b.next_packet(), Ok(Some(vec![0xC1, 0x04, 0xF4, 0x06])));
        assert_eq!(b.next_packet(), Ok(None));
        let mut basura = PacketBuffer::default();
        basura.extend(&[0x47, 0x45, 0x54]); // "GET"
        assert_eq!(basura.next_packet(), Err(InvalidData(0x47)));
    }
}
