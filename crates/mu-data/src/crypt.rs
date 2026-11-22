//! Cifrados de archivos del cliente.

const MAP_XOR_KEY: [u8; 16] = [
    0xD1, 0x73, 0x52, 0xF6, 0xD2, 0x9A, 0xCB, 0x27, 0x3E, 0xAF, 0x59, 0x31, 0x37, 0xB3, 0xE7, 0xA2,
];

/// LEA-256 en modo ECB de los BMD versión 15 (clientes Season 12+; clave de
/// `xulek-muonline/Client.Data/LEACrypto.cs`). Lo que no llena un bloque de 16 queda igual.
pub fn lea_decrypt(src: &[u8]) -> Vec<u8> {
    use lea::Lea256;
    use lea::prelude::*;
    const KEY: [u8; 32] = [
        0xcc, 0x50, 0x45, 0x13, 0xc2, 0xa6, 0x57, 0x4e, 0xd6, 0x9a, 0x45, 0x89, 0xbf, 0x2f, 0xbc, 0xd9, 0x39, 0xb3,
        0xb3, 0xbd, 0x50, 0xbd, 0xcc, 0xb6, 0x85, 0x46, 0xd1, 0xd6, 0x16, 0x54, 0xe0, 0x87,
    ];
    let cipher = Lea256::new(GenericArray::from_slice(&KEY));
    let mut out = src.to_vec();
    for block in out.chunks_exact_mut(16) {
        cipher.decrypt_block(GenericArray::from_mut_slice(block));
    }
    out
}

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
