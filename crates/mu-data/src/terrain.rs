//! Terreno de un mapa (`Data/WorldN/`).
//!
//! Un mapa de MU es una rejilla de 256 x 256 casillas. Por cada casilla hay:
//! - una altura (`TerrainHeight.OZB`)
//! - un color de luz precalculado (`TerrainLight.OZJ`)
//! - dos capas de textura y la transparencia de la segunda (`EncTerrainN.map`)
//!
//! Coordenadas como en el cliente original: X e Y en el plano, Z hacia arriba,
//! índice de casilla = `y * 256 + x`.

use std::path::{Path, PathBuf};

use crate::texture::{self, Texture};
use crate::{Error, Result, crypt, fs};

pub const SIZE: usize = 256;
const CELLS: usize = SIZE * SIZE;

/// El cliente original multiplica el byte de altura por 1.5.
pub const HEIGHT_SCALE: f32 = 1.5;

/// En la capa 2, este valor significa "sin textura".
pub const NO_TEXTURE: u8 = 255;

/// Contenido de `EncTerrainN.map` ya descifrado.
#[derive(Debug, Clone)]
pub struct TerrainMapping {
    pub version: u8,
    pub map_number: u8,
    /// Textura base de cada casilla.
    pub layer1: Vec<u8>,
    /// Textura que se mezcla encima (o `NO_TEXTURE`).
    pub layer2: Vec<u8>,
    /// Opacidad de la capa 2 en cada vértice (0..=255).
    pub alpha: Vec<u8>,
}

pub fn read_mapping(buf: &[u8]) -> Result<TerrainMapping> {
    if buf.starts_with(b"MAP\x01") {
        return Err(Error::Format(
            "este .map usa el cifrado nuevo (Season 15+), no soportado todavía".into(),
        ));
    }
    let dec = crypt::map_file_decrypt(buf);
    if dec.len() < 2 + CELLS * 3 {
        return Err(Error::Format(format!(".map de {} bytes, demasiado corto", dec.len())));
    }
    let layer = |n: usize| dec[2 + n * CELLS..2 + (n + 1) * CELLS].to_vec();
    Ok(TerrainMapping {
        version: dec[0],
        map_number: dec[1],
        layer1: layer(0),
        layer2: layer(1),
        alpha: layer(2),
    })
}

/// `TerrainHeight.OZB`: una cabecera BMP de 8 bits y luego 1 byte de altura por casilla.
/// Nos quedamos con los últimos 256*256 bytes, como hace el cliente original.
pub fn read_heights(buf: &[u8]) -> Result<Vec<f32>> {
    let start = buf
        .len()
        .checked_sub(CELLS)
        .ok_or_else(|| Error::Format("TerrainHeight demasiado corto".into()))?;
    Ok(buf[start..].iter().map(|&h| h as f32 * HEIGHT_SCALE).collect())
}

/// Archivos de textura del suelo según su índice en las capas del `.map`.
fn default_tile_textures() -> Vec<(u8, String)> {
    let mut v: Vec<(u8, String)> = [
        "TileGrass01.ozj",
        "TileGrass02.ozj",
        "TileGround01.ozj",
        "TileGround02.ozj",
        "TileGround03.ozj",
        "TileWater01.ozj",
        "TileWood01.ozj",
        "TileRock01.ozj",
        "TileRock02.ozj",
        "TileRock03.ozj",
        "TileRock04.ozj",
        "TileRock05.ozj",
        "TileRock06.ozj",
        "TileRock07.ozj",
    ]
    .iter()
    .enumerate()
    .map(|(i, n)| (i as u8, n.to_string()))
    .collect();
    // 14..=29: texturas extra que algunos mapas añaden.
    v.extend((1..=16u8).map(|i| (13 + i, format!("ExtTile{i:02}.ozj"))));
    v
}

/// Terreno listo para dibujar.
pub struct Terrain {
    pub world: u16,
    pub dir: PathBuf,
    /// Altura de cada vértice, ya multiplicada por `HEIGHT_SCALE`.
    pub heights: Vec<f32>,
    /// Luz final de cada vértice (RGB 0..1): mapa de luz × sombreado por la pendiente.
    pub light: Vec<[f32; 3]>,
    pub mapping: TerrainMapping,
    /// Textura de cada índice de capa (`None` si el mapa no la usa o no existe).
    pub textures: Vec<Option<Texture>>,
}

impl Terrain {
    pub fn height(&self, x: usize, y: usize) -> f32 {
        self.heights[(y.min(SIZE - 1)) * SIZE + x.min(SIZE - 1)]
    }
}

/// Carga el terreno de `data_dir/World{world}`.
pub fn load(data_dir: &Path, world: u16) -> Result<Terrain> {
    let dir = fs::find(data_dir, &format!("World{world}"))
        .ok_or_else(|| Error::NotFound(data_dir.join(format!("World{world}"))))?;

    let heights = read_heights(&fs::read(&dir, "TerrainHeight.OZB")?)?;
    let mapping = read_mapping(&fs::read(&dir, &format!("EncTerrain{world}.map"))?)?;

    // Si falta el mapa de luz, todo blanco (como el original).
    let base_light: Vec<[f32; 3]> = match texture::load(&dir, "TerrainLight.OZJ") {
        Ok(t) if t.width as usize == SIZE && t.height as usize == SIZE => t
            .rgba
            .chunks_exact(4)
            .map(|p| [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0])
            .collect(),
        _ => vec![[1.0; 3]; CELLS],
    };
    let shade = slope_shading(&heights);
    let light = base_light
        .iter()
        .zip(&shade)
        .map(|(c, s)| [c[0] * s, c[1] * s, c[2] * s])
        .collect();

    // Solo cargamos las texturas que el mapa usa de verdad.
    let mut used = [false; 256];
    for &t in mapping.layer1.iter().chain(&mapping.layer2) {
        used[t as usize] = true;
    }
    let mut textures: Vec<Option<Texture>> = vec![None; 256];
    for (index, name) in default_tile_textures() {
        if used[index as usize] {
            textures[index as usize] = texture::load(&dir, &name).ok();
        }
    }

    Ok(Terrain { world, dir, heights, light, mapping, textures })
}

/// Sombreado según la pendiente, como `CreateTerrainLight` del original:
/// `clamp(normal · luz + 0.5, 0, 1)` con la luz en (0.5, -0.5, 0.5).
fn slope_shading(heights: &[f32]) -> Vec<f32> {
    let h = |x: usize, y: usize| heights[(y % SIZE) * SIZE + (x % SIZE)] / HEIGHT_SCALE;
    let light = normalize([0.5, -0.5, 0.5]);
    let mut out = vec![0.0; CELLS];
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Normal de la casilla a partir de sus dos triángulos (escala 100 por casilla, como el original).
            let v1 = [100.0, 0.0, h(x + 1, y)];
            let v2 = [100.0, 100.0, h(x + 1, y + 1)];
            let v3 = [0.0, 100.0, h(x, y + 1)];
            let v4 = [0.0, 0.0, h(x, y)];
            let n1 = face_normal(v1, v2, v3);
            let n2 = face_normal(v3, v4, v1);
            let n = normalize([n1[0] + n2[0], n1[1] + n2[1], n1[2] + n2[2]]);
            let d = n[0] * light[0] + n[1] * light[1] + n[2] * light[2];
            out[y * SIZE + x] = (d + 0.5).clamp(0.0, 1.0);
        }
    }
    out
}

fn face_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    normalize([u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]])
}

fn normalize(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if l == 0.0 { [0.0, 0.0, 1.0] } else { [v[0] / l, v[1] / l, v[2] / l] }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prueba con los datos reales si están en `referencias/` (si no, se salta).
    #[test]
    fn carga_lorencia() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        if !data.exists() {
            eprintln!("sin datos en {}, se salta", data.display());
            return;
        }
        let t = load(&data, 1).expect("cargar Lorencia");
        assert_eq!(t.heights.len(), CELLS);
        assert!(t.textures[0].is_some(), "TileGrass01 cargada");
        // Si el descifrado estuviera mal, saldrían índices de textura aleatorios (casi todos distintos).
        let mut usadas: Vec<u8> = t.mapping.layer1.clone();
        usadas.sort();
        usadas.dedup();
        assert!(usadas.len() <= 30, "índices de capa 1 raros: {usadas:?}");
        let alt = t.heights.iter().cloned().fold(0.0, f32::max);
        eprintln!(
            "Lorencia: .map v{} n{}, capa1 usa {usadas:?}, altura máx {alt}",
            t.mapping.version, t.mapping.map_number
        );
    }
}
