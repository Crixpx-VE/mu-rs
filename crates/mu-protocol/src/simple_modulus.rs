//! "SimpleModulus" encryption of C3/C4 packets (Season 6 variant).
//!
//! The packet content (what follows the header) is split into 8-byte blocks.
//! Each block is encrypted into 11 bytes:
//! - 4 values of 18 bits (modular multiplication with the keys) = 9 bytes
//! - 1 byte with the block's real size and 1 checksum byte
//!
//! The first byte of the encrypted content is a counter (0..=255) that increases with each
//! packet; it detects repeated or lost packets.
//!
//! Translated from `referencias/OpenMU/src/Network/SimpleModulus/*.cs` (MIT license).

const DECRYPTED_BLOCK: usize = 8;
const ENCRYPTED_BLOCK: usize = 11;
const BLOCK_SIZE_XOR: u8 = 0x3D;
const BLOCK_CHECKSUM_XOR: u8 = 0xF8;
const BITS_PER_VALUE: usize = 18;

/// Keys: modulus, multiplier (encrypt or decrypt) and xor; 4 of each.
#[derive(Debug, Clone, Copy)]
pub struct Keys {
    pub modulus: [u32; 4],
    pub key: [u32; 4],
    pub xor: [u32; 4],
}

impl Keys {
    /// From the 12-number format of the `Enc*.dat`/`Dec*.dat` files.
    pub const fn from_array(k: [u32; 12]) -> Self {
        Self {
            modulus: [k[0], k[1], k[2], k[3]],
            key: [k[4], k[5], k[6], k[7]],
            xor: [k[8], k[9], k[10], k[11]],
        }
    }
}

/// Default keys (those of the official clients, also in OpenMU).
pub mod default_keys {
    use super::Keys;
    /// Server → client: the server encrypts with this one...
    pub const SERVER_ENCRYPT: Keys = Keys::from_array([
        73326, 109989, 98843, 171058, 13169, 19036, 35482, 29587, 62004, 64409, 35374, 64599,
    ]);
    /// ...and the client decrypts with this one.
    pub const CLIENT_DECRYPT: Keys = Keys::from_array([
        73326, 109989, 98843, 171058, 18035, 30340, 24701, 11141, 62004, 64409, 35374, 64599,
    ]);
    /// Client → server: the client encrypts with this one...
    pub const CLIENT_ENCRYPT: Keys = Keys::from_array([
        128079, 164742, 70235, 106898, 23489, 11911, 19816, 13647, 48413, 46165, 15171, 37433,
    ]);
    /// ...and the server decrypts with this one.
    pub const SERVER_DECRYPT: Keys = Keys::from_array([
        128079, 164742, 70235, 106898, 31544, 2047, 57011, 10183, 48413, 46165, 15171, 37433,
    ]);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecryptError {
    NotEncrypted,
    BadLength(usize),
    BlockSize(u8),
    Checksum { expected: u8, actual: u8 },
    Counter { expected: u8, actual: u8 },
}

impl std::fmt::Display for DecryptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for DecryptError {}

/// One direction of the connection: holds its keys and its counter.
#[derive(Debug, Clone)]
pub struct SimpleModulus {
    keys: Keys,
    counter: u8,
}

fn header_len(first: u8) -> usize {
    if first == 0xC2 || first == 0xC4 { 3 } else { 2 }
}

impl SimpleModulus {
    pub fn new(keys: Keys) -> Self {
        Self { keys, counter: 0 }
    }

    /// Encrypts a complete (plaintext) C3/C4 packet and returns the encrypted packet.
    /// C1/C2 packets are returned unchanged.
    pub fn encrypt(&mut self, packet: &[u8]) -> Vec<u8> {
        let first = packet[0];
        if first < 0xC3 {
            return packet.to_vec();
        }
        let hl = header_len(first);
        // The counter goes before the content.
        let mut content = Vec::with_capacity(packet.len() - hl + 1);
        content.push(self.counter);
        content.extend_from_slice(&packet[hl..]);

        let blocks = content.len().div_ceil(DECRYPTED_BLOCK);
        let total = hl + blocks * ENCRYPTED_BLOCK;
        let mut out = vec![0u8; total];
        out[0] = first;
        write_size(&mut out, hl, total);

        for (i, chunk) in content.chunks(DECRYPTED_BLOCK).enumerate() {
            let mut input = [0u8; DECRYPTED_BLOCK];
            input[..chunk.len()].copy_from_slice(chunk);
            let block = &mut out[hl + i * ENCRYPTED_BLOCK..hl + (i + 1) * ENCRYPTED_BLOCK];
            self.encrypt_block(&input, chunk.len(), block);
        }
        self.counter = self.counter.wrapping_add(1);
        out
    }

    /// Decrypts a complete C3/C4 packet. The result keeps the C3/C4 type
    /// (to tell it was encrypted), without the counter.
    pub fn decrypt(&mut self, packet: &[u8]) -> Result<Vec<u8>, DecryptError> {
        let first = *packet.first().ok_or(DecryptError::BadLength(0))?;
        if first < 0xC3 {
            return Err(DecryptError::NotEncrypted);
        }
        let hl = header_len(first);
        let body = packet.get(hl..).ok_or(DecryptError::BadLength(packet.len()))?;
        if body.is_empty() || body.len() % ENCRYPTED_BLOCK != 0 {
            return Err(DecryptError::BadLength(packet.len()));
        }

        let mut content = Vec::with_capacity(body.len() / ENCRYPTED_BLOCK * DECRYPTED_BLOCK);
        for block in body.chunks_exact(ENCRYPTED_BLOCK) {
            let (plain, size) = self.decrypt_block(block.try_into().unwrap())?;
            content.extend_from_slice(&plain[..size]);
        }
        let actual = *content.first().ok_or(DecryptError::BadLength(packet.len()))?;
        if actual != self.counter {
            return Err(DecryptError::Counter { expected: self.counter, actual });
        }
        self.counter = self.counter.wrapping_add(1);

        let total = hl + content.len() - 1;
        let mut out = vec![0u8; hl];
        out[0] = first;
        out.extend_from_slice(&content[1..]);
        write_size(&mut out, hl, total);
        Ok(out)
    }

    fn encrypt_block(&self, input: &[u8; DECRYPTED_BLOCK], size: usize, out: &mut [u8]) {
        let k = &self.keys;
        let word = |i: usize| u16::from_le_bytes([input[2 * i], input[2 * i + 1]]) as u32;

        let mut r = [0u32; 4];
        r[0] = (k.xor[0] ^ word(0)).wrapping_mul(k.key[0]) % k.modulus[0];
        for i in 1..4 {
            r[i] = (k.xor[i] ^ (word(i) ^ (r[i - 1] & 0xFFFF))).wrapping_mul(k.key[i]) % k.modulus[i];
        }
        for i in 0..3 {
            r[i] = r[i] ^ k.xor[i] ^ (r[i + 1] & 0xFFFF);
        }

        out.fill(0);
        for (i, &v) in r.iter().enumerate() {
            write_value(out, i, v);
        }

        let mut checksum = BLOCK_CHECKSUM_XOR;
        for &b in &input[..size] {
            checksum ^= b;
        }
        out[ENCRYPTED_BLOCK - 2] = (size as u8 ^ BLOCK_SIZE_XOR) ^ checksum;
        out[ENCRYPTED_BLOCK - 1] = checksum;
    }

    fn decrypt_block(&self, input: &[u8; ENCRYPTED_BLOCK]) -> Result<([u8; DECRYPTED_BLOCK], usize), DecryptError> {
        let k = &self.keys;
        let mut r = [0u32; 4];
        for (i, v) in r.iter_mut().enumerate() {
            *v = read_value(input, i);
        }
        for i in (1..4).rev() {
            r[i - 1] = r[i - 1] ^ k.xor[i - 1] ^ (r[i] & 0xFFFF);
        }

        let mut out = [0u8; DECRYPTED_BLOCK];
        for i in 0..4 {
            let mut v = k.xor[i] ^ (r[i].wrapping_mul(k.key[i]) % k.modulus[i]);
            if i > 0 {
                v ^= r[i - 1] & 0xFFFF;
            }
            out[2 * i..2 * i + 2].copy_from_slice(&(v as u16).to_le_bytes());
        }

        let size = input[ENCRYPTED_BLOCK - 2] ^ input[ENCRYPTED_BLOCK - 1] ^ BLOCK_SIZE_XOR;
        if size as usize > DECRYPTED_BLOCK {
            return Err(DecryptError::BlockSize(size));
        }
        let mut checksum = BLOCK_CHECKSUM_XOR;
        for &b in &out {
            checksum ^= b;
        }
        if input[ENCRYPTED_BLOCK - 1] != checksum {
            return Err(DecryptError::Checksum { expected: checksum, actual: input[ENCRYPTED_BLOCK - 1] });
        }
        Ok((out, size as usize))
    }
}

fn write_size(out: &mut [u8], hl: usize, total: usize) {
    if hl == 2 {
        out[1] = total as u8;
    } else {
        out[1..3].copy_from_slice(&(total as u16).to_be_bytes());
    }
}

// Each value takes 18 consecutive bits in the block: 16 "normal" bits (big-endian
// within the bit stream) and 2 extra bits with the modulus overflow.

fn bit_offset(i: usize) -> u32 {
    (i * BITS_PER_VALUE % 8) as u32
}

fn byte_offset(i: usize) -> usize {
    i * BITS_PER_VALUE / 8
}

fn remainder_mask(i: usize) -> u32 {
    let bo = bit_offset(i);
    ((0xFFu32 << (6 - bo)) & 0xFF) - ((0xFFu32 << (8 - bo)) & 0xFF)
}

fn write_value(out: &mut [u8], i: usize, value: u32) {
    let (b, bo) = (byte_offset(i), bit_offset(i));
    let first_mask = 0xFFu32 >> bo;
    let swapped = value.swap_bytes();
    out[b] |= ((swapped >> (24 + bo)) & first_mask) as u8;
    out[b + 1] = (swapped >> (16 + bo)) as u8;
    out[b + 2] = ((swapped >> (8 + bo)) & (0xFFu32 << (8 - bo))) as u8;
    let remainder = (value >> 16) << (6 - bo);
    out[b + 2] |= (remainder & remainder_mask(i)) as u8;
}

fn read_value(input: &[u8], i: usize) -> u32 {
    let (b, bo) = (byte_offset(i), bit_offset(i));
    let first_mask = 0xFFu32 >> bo;
    let mut r: u32 = ((input[b] as u32 & first_mask) << (24 + bo))
        .wrapping_add((input[b + 1] as u32) << (16 + bo))
        .wrapping_add((input[b + 2] as u32 & (0xFFu32 << (8 - bo))) << (8 + bo));
    r = r.swap_bytes();
    let remainder = input[b + 2] as u32 & remainder_mask(i);
    r.wrapping_add((remainder << 16) >> (6 - bo))
}

#[cfg(test)]
mod tests {
    use super::default_keys::*;
    use super::*;
    use crate::xor::{XOR32_KEY, xor32_decrypt, xor32_encrypt};

    /// Real "ping" packet captured from an official client (from the OpenMU tests).
    /// The client applies Xor32 and then SimpleModulus; the server undoes them in reverse order.
    #[test]
    fn descifra_ping_real_del_cliente() {
        let encrypted = [
            0xC3, 0x18, 0xBC, 0x9D, 0x35, 0xE2, 0xA2, 0x21, 0x22, 0x91, 0x5C, 0x2B, 0x1E, 0x18, 0x63, 0x47, 0x28,
            0xC3, 0x10, 0x20, 0xF1, 0xC4, 0x2A, 0x14,
        ];
        let expected = [0xC3, 12, 0x0E, 0x00, 0x01, 0x33, 0xFE, 0x27, 0x00, 0x00, 0x00, 0x00];

        let mut server = SimpleModulus::new(SERVER_DECRYPT);
        let mut p = server.decrypt(&encrypted).expect("descifrar");
        xor32_decrypt(&mut p, &XOR32_KEY);
        assert_eq!(p, expected);
    }

    #[test]
    fn el_cliente_cifra_como_el_oficial() {
        let official = [
            0xC3, 0x18, 0xBC, 0x9D, 0x35, 0xE2, 0xA2, 0x21, 0x22, 0x91, 0x5C, 0x2B, 0x1E, 0x18, 0x63, 0x47, 0x28,
            0xC3, 0x10, 0x20, 0xF1, 0xC4, 0x2A, 0x14,
        ];
        let plain = [0xC3, 12, 0x0E, 0x00, 0x01, 0x33, 0xFE, 0x27, 0x00, 0x00, 0x00, 0x00];
        let mut xored = plain.to_vec();
        xor32_encrypt(&mut xored, &XOR32_KEY);
        let encrypted = SimpleModulus::new(CLIENT_ENCRYPT).encrypt(&xored);

        // Header and first block (8 full bytes) identical to the official client.
        assert_eq!(encrypted[..2 + 11], official[..2 + 11]);
        // The last block only carries 3 useful bytes: the official client pads the rest
        // with whatever was in memory and we pad with zeros. Both are valid:
        // the server decrypts ours to the same packet.
        let mut back = SimpleModulus::new(SERVER_DECRYPT).decrypt(&encrypted).unwrap();
        xor32_decrypt(&mut back, &XOR32_KEY);
        assert_eq!(back, plain);
    }

    #[test]
    fn ida_y_vuelta_en_ambos_sentidos_con_contador() {
        for (enc, dec) in [(CLIENT_ENCRYPT, SERVER_DECRYPT), (SERVER_ENCRYPT, CLIENT_DECRYPT)] {
            let mut a = SimpleModulus::new(enc);
            let mut b = SimpleModulus::new(dec);
            // 300 packets: the counter wraps around (255 → 0).
            for n in 0..300usize {
                let len = 3 + n % 40;
                let mut p: Vec<u8> = (0..len).map(|i| (i * 7 + n) as u8).collect();
                p[0] = 0xC3;
                p[1] = len as u8;
                let c = a.encrypt(&p);
                assert_eq!(b.decrypt(&c).unwrap(), p, "paquete {n}");
            }
            // Large C4.
            let mut big = vec![0xC4, 0x01, 0x2C];
            big.extend((0..297).map(|i| i as u8));
            assert_eq!(b.decrypt(&a.encrypt(&big)).unwrap(), big);
        }
    }

    #[test]
    fn detecta_paquete_repetido_o_alterado() {
        let p = [0xC3, 0x06, 0xF1, 0x01, 0xAA, 0xBB];
        let mut a = SimpleModulus::new(CLIENT_ENCRYPT);
        let mut b = SimpleModulus::new(SERVER_DECRYPT);
        let c = a.encrypt(&p);
        assert!(b.decrypt(&c).is_ok());
        // Repeating the same packet (replay attack): the counter does not match.
        assert!(matches!(b.decrypt(&c), Err(DecryptError::Counter { .. })));
        // Tamper with one byte.
        let mut c2 = a.encrypt(&p);
        c2[5] ^= 0x40;
        assert!(b.decrypt(&c2).is_err());
    }
}
