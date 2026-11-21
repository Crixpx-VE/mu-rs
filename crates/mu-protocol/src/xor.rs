//! Cifrados XOR simples.
//!
//! - **Xor32**: lo aplica el cliente a los paquetes que manda al Game Server
//!   (antes de SimpleModulus si el paquete es C3/C4).
//! - **Xor3**: lo usan algunos campos sueltos (usuario y contraseña del login).

/// Clave Xor32 por defecto (la de los clientes oficiales; OpenMU usa la misma).
pub const XOR32_KEY: [u8; 32] = [
    0xAB, 0x11, 0xCD, 0xFE, 0x18, 0x23, 0xC5, 0xA3, 0xCA, 0x33, 0xC1, 0xCC, 0x66, 0x67, 0x21, 0xF3,
    0x32, 0x12, 0x15, 0x35, 0x29, 0xFF, 0xFE, 0x1D, 0x44, 0xEF, 0xCD, 0x41, 0x26, 0x3C, 0x4E, 0x4D,
];

/// Clave Xor3 por defecto.
pub const XOR3_KEY: [u8; 3] = [0xFC, 0xCF, 0xAB];

/// Cifra en sitio un paquete completo (cabecera incluida). El código no se toca:
/// cada byte desde el siguiente se mezcla con el anterior ya cifrado.
pub fn xor32_encrypt(packet: &mut [u8], key: &[u8; 32]) {
    let start = header_len(packet) + 1;
    for i in start..packet.len() {
        packet[i] ^= packet[i - 1] ^ key[i % 32];
    }
}

/// Descifra en sitio (recorre al revés porque cada byte depende del anterior cifrado).
pub fn xor32_decrypt(packet: &mut [u8], key: &[u8; 32]) {
    let start = header_len(packet) + 1;
    for i in (start..packet.len()).rev() {
        packet[i] ^= packet[i - 1] ^ key[i % 32];
    }
}

/// Cifra/descifra (es simétrico) un campo con la clave de 3 bytes.
pub fn xor3(data: &mut [u8]) {
    for (i, b) in data.iter_mut().enumerate() {
        *b ^= XOR3_KEY[i % 3];
    }
}

fn header_len(packet: &[u8]) -> usize {
    match packet.first() {
        Some(0xC2 | 0xC4) => 3,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xor32_ida_y_vuelta() {
        let original: Vec<u8> = [0xC1, 0x10, 0xF1, 0x01].into_iter().chain(0..12).collect();
        let mut p = original.clone();
        xor32_encrypt(&mut p, &XOR32_KEY);
        assert_ne!(p, original);
        assert_eq!(&p[..3], &original[..3], "cabecera y código no se cifran");
        xor32_decrypt(&mut p, &XOR32_KEY);
        assert_eq!(p, original);
    }

    #[test]
    fn xor3_es_simetrico() {
        let mut d = *b"usuario";
        xor3(&mut d);
        xor3(&mut d);
        assert_eq!(&d, b"usuario");
    }
}
