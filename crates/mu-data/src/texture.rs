//! Texturas: `.OZJ` (JPEG con cabecera) y `.OZT` (TGA con cabecera).

use std::path::Path;

use crate::{Error, Result, fs};

/// Imagen decodificada en RGBA8, fila 0 = arriba.
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// `.OZJ`: 24 bytes de cabecera propia y luego un JPEG normal.
/// El byte 17 indica si las filas van de arriba abajo; si no, hay que voltearla.
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

/// `.OZT`: 4 bytes de cabecera propia y luego un TGA de 32 bits sin comprimir
/// (12 bytes de cabecera TGA, ancho, alto, profundidad, descriptor y los píxeles BGRA de abajo arriba).
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

/// Carga una textura según su extensión (`.ozj`/`.jpg` o `.ozt`/`.tga`).
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
