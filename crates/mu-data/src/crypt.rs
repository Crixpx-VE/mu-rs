//! Cifrados de archivos del cliente.

const MAP_XOR_KEY: [u8; 16] = [
    0xD1, 0x73, 0x52, 0xF6, 0xD2, 0x9A, 0xCB, 0x27, 0x3E, 0xAF, 0x59, 0x31, 0x37, 0xB3, 0xE7, 0xA2,
];

/// Descifra los archivos `EncTerrain*.map`, `*.att` y `*.obj` (el "MapFileDecrypt" del cliente original).
///
/// Cada byte se hace XOR con una clave de 16 bytes y se le resta una clave que
/// va cambiando según el byte cifrado anterior.
pub fn map_file_decrypt(src: &[u8]) -> Vec<u8> {
    let mut key: u8 = 0x5E;
    src.iter()
        .enumerate()
        .map(|(i, &b)| {
            let out = (b ^ MAP_XOR_KEY[i % 16]).wrapping_sub(key);
            key = b.wrapping_add(0x3D);
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cifrado inverso, solo para comprobar que el descifrado es correcto.
    fn map_file_encrypt(src: &[u8]) -> Vec<u8> {
        let mut key: u8 = 0x5E;
        src.iter()
            .enumerate()
            .map(|(i, &b)| {
                let enc = b.wrapping_add(key) ^ MAP_XOR_KEY[i % 16];
                key = enc.wrapping_add(0x3D);
                enc
            })
            .collect()
    }

    #[test]
    fn ida_y_vuelta() {
        let datos: Vec<u8> = (0..=255).chain(0..=255).collect();
        assert_eq!(map_file_decrypt(&map_file_encrypt(&datos)), datos);
    }
}
