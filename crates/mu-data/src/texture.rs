//! Textures: `.OZJ` (JPEG with a header) and `.OZT` (TGA with a header).

use std::path::Path;

use crate::{Error, Result, fs};

/// Decoded RGBA8 image, row 0 = top.
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// How a texture uses its alpha channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlphaKind {
    /// No transparency.
    Opaque,
    /// All-or-nothing alpha (leaves, grates): cut out.
    Cutout,
    /// Gradient alpha (shadows, smoke, glass): blended.
    Blend,
}

impl Texture {
    pub fn alpha_kind(&self) -> AlphaKind {
        let total = self.rgba.len() / 4;
        let (mut transparent, mut partial) = (0usize, 0usize);
        for a in self.rgba.iter().skip(3).step_by(4) {
            if *a < 250 {
                transparent += 1;
            }
            if (16..240).contains(a) {
                partial += 1;
            }
        }
        if transparent == 0 {
            AlphaKind::Opaque
        } else if partial * 20 < total {
            AlphaKind::Cutout
        } else {
            AlphaKind::Blend
        }
    }
}

/// `.OZJ`: 24-byte custom header followed by a plain JPEG.
/// Byte 17 says whether rows go top to bottom; if not, the image must be flipped.
pub fn read_ozj(buf: &[u8]) -> Result<Texture> {
    if buf.len() < 24 {
        return Err(Error::Format("OZJ demasiado corto".into()));
    }
    let top_down = buf[17] != 0;
    let img = image::load_from_memory_with_format(&buf[24..], image::ImageFormat::Jpeg)
        .map_err(|e| Error::Format(format!("OZJ: {e}")))?
        .to_rgba8();
    let (width, height) = img.dimensions();
    let mut tex = Texture { width, height, rgba: img.into_raw() };
    if !top_down {
        flip_vertical(&mut tex);
    }
    Ok(tex)
}

/// `.OZT`: 4-byte custom header followed by an uncompressed 32-bit TGA
/// (12-byte TGA header, width, height, depth, descriptor and BGRA pixels bottom to top).
pub fn read_ozt(buf: &[u8]) -> Result<Texture> {
    const HEADER: usize = 16;
    let bad = || Error::Format("OZT inválido".into());
    let w = u16::from_le_bytes([*buf.get(HEADER).ok_or_else(bad)?, buf[HEADER + 1]]) as u32;
    let h = u16::from_le_bytes([*buf.get(HEADER + 2).ok_or_else(bad)?, buf[HEADER + 3]]) as u32;
    let depth = *buf.get(HEADER + 4).ok_or_else(bad)?;
    let pixels = buf.get(HEADER + 6..HEADER + 6 + (w * h * 4) as usize).ok_or_else(bad)?;
    if depth != 32 {
        return Err(Error::Format(format!("OZT de {depth} bits, se esperaban 32")));
    }

    let row = (w * 4) as usize;
    let mut rgba = vec![0u8; pixels.len()];
    for y in 0..h as usize {
        let src = &pixels[y * row..(y + 1) * row];
        let dst = &mut rgba[(h as usize - 1 - y) * row..][..row];
        for (d, s) in dst.chunks_exact_mut(4).zip(src.chunks_exact(4)) {
            d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
        }
    }
    Ok(Texture { width: w, height: h, rgba })
}

/// Loads a texture by its extension (`.ozj`/`.jpg` or `.ozt`/`.tga`).
pub fn load(dir: &Path, name: &str) -> Result<Texture> {
    let buf = fs::read(dir, name)?;
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "ozj" | "jpg" => read_ozj(&buf),
        "ozt" | "tga" => read_ozt(&buf),
        _ => Err(Error::Format(format!("extensión de textura desconocida: {name}"))),
    }
}

fn flip_vertical(tex: &mut Texture) {
    let row = (tex.width * 4) as usize;
    let h = tex.height as usize;
    for y in 0..h / 2 {
        let (top, bottom) = tex.rgba.split_at_mut((h - 1 - y) * row);
        top[y * row..(y + 1) * row].swap_with_slice(&mut bottom[..row]);
    }
}
