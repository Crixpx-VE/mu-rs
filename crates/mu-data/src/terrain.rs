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

/// Banderas de cada casilla en `EncTerrainN.att`.
pub mod attr {
    /// Zona segura (no se puede atacar).
    pub const SAFE_ZONE: u16 = 0x0001;
    /// Ocupada por un personaje (lo marca el juego en tiempo real, no el archivo).
    pub const CHARACTER: u16 = 0x0002;
    /// No se puede caminar.
    pub const NO_MOVE: u16 = 0x0004;
    /// Sin suelo (vacío, se cae).
    pub const NO_GROUND: u16 = 0x0008;
    pub const WATER: u16 = 0x0010;
    pub const ACTION: u16 = 0x0020;
    pub const HEIGHT: u16 = 0x0040;
    pub const CAMERA_UP: u16 = 0x0080;
}

/// `EncTerrainN.att`: descifrado de mapa + XOR de 3 bytes, cabecera de 4 bytes
/// (versión, mapa, 255, 255) y luego 1 o 2 bytes de banderas por casilla.
pub fn read_attributes(buf: &[u8]) -> Result<Vec<u16>> {
    if buf.starts_with(b"ATT\x01") {
        return Err(Error::Format("este .att usa el cifrado nuevo (Season 15+), no soportado todavía".into()));
    }
    let mut dec = crypt::map_file_decrypt(buf);
    const XOR3: [u8; 3] = [0xFC, 0xCF, 0xAB];
    for (i, b) in dec.iter_mut().enumerate() {
        *b ^= XOR3[i % 3];
    }
    let wide = match dec.len() {
        n if n == 4 + CELLS => false,
        n if n == 4 + CELLS * 2 => true,
        n => return Err(Error::Format(format!(".att de {n} bytes inesperado"))),
    };
    if dec[0] != 0 || dec[2] != 255 || dec[3] != 255 {
        return Err(Error::Format(format!("cabecera de .att rara: {:?}", &dec[..4])));
    }
    Ok((0..CELLS)
        .map(|i| if wide { u16::from_le_bytes([dec[4 + 2 * i], dec[5 + 2 * i]]) } else { dec[4 + i] as u16 })
        .collect())
}

/// Archivos de textura del suelo de `world` según su índice en las capas del `.map`
/// (`CMapManager::LoadWorld`, MapManager.cpp:1439-1583). Con carpeta (relativa a `Data`), la
/// textura no está en la del mapa. `world` es la carpeta (`WorldActive + 1`).
fn tile_textures(world: u16) -> Vec<(u8, Option<&'static str>, String)> {
    let mut v: Vec<(u8, Option<&'static str>, String)> =
        default_tile_textures().into_iter().map(|(i, n)| (i, None, n)).collect();
    let mut set = |i: usize, dir: Option<&'static str>, name: &str| v[i] = (i as u8, dir, name.to_string());
    match world {
        // Elbeland (`WD_51HOME_6TH_CHAR`): la 2 es `AlphaTileGround01.Tga` (1445-1454).
        52 => set(2, None, "AlphaTileGround01.ozt"),
        // Kanturu 3 (`WD_39KANTURU_3RD`): la 3 es `AlphaTileGround02.Tga` (1461-1465).
        40 => set(3, None, "AlphaTileGround02.ozt"),
        // Illusion Temple (`IsCursedTemple`): la 4 es `AlphaTileGround03.Tga` (1471-1475).
        46..=51 => set(4, None, "AlphaTileGround03.ozt"),
        // Login / selección (1493-1497) y Empire Guardian (1579-1583): la 10 es `AlphaTile01.Tga`.
        70..=75 => set(10, None, "AlphaTile01.ozt"),
        // Vulcanus (`IsPKField`) y Doppelganger 2: la 11 es la lava `Object64\song_lava1.jpg` (1506-1510).
        64 | 67 => set(11, Some("Object64"), "song_lava1.ozj"),
        // Karutan 1 y 2 (`IsKarutanMap`): la 12 es `AlphaTile01.Tga` (1516-1521).
        81 | 82 => set(12, None, "AlphaTile01.ozt"),
        _ => {}
    }
    v
}

/// Índice de textura que el original dibuja con prueba de alfa (`RenderFace`, ZzzLodTerrain.cpp:1330-1393):
/// la 3 en Kanturu 3, la 4 en Illusion Temple, la 2 en Elbeland, la 10 en los Empire Guardian (70-73),
/// el login (74) y la selección de personaje (75), y la 12 en Karutan.
pub fn alpha_tested_tile(world: u16) -> Option<u8> {
    match world {
        40 => Some(3),
        46..=51 => Some(4),
        52 => Some(2),
        70..=75 => Some(10),
        81 | 82 => Some(12),
        _ => None,
    }
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
    /// Banderas de cada casilla (ver [`attr`]). Todo 0 si el mapa no trae `.att`.
    pub attributes: Vec<u16>,
    /// Textura de cada índice de capa (`None` si el mapa no la usa o no existe).
    pub textures: Vec<Option<Texture>>,
}

impl Terrain {
    pub fn height(&self, x: usize, y: usize) -> f32 {
        self.heights[(y.min(SIZE - 1)) * SIZE + x.min(SIZE - 1)]
    }

    /// Altura interpolada en un punto cualquiera (en casillas), como `RequestTerrainHeight`.
    pub fn height_at(&self, x: f32, y: f32) -> f32 {
        let x = x.clamp(0.0, (SIZE - 1) as f32);
        let y = y.clamp(0.0, (SIZE - 1) as f32);
        let (xi, yi) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x - xi as f32, y - yi as f32);
        let h00 = self.height(xi, yi);
        let h10 = self.height(xi + 1, yi);
        let h01 = self.height(xi, yi + 1);
        let h11 = self.height(xi + 1, yi + 1);
        let top = h00 + (h10 - h00) * fx;
        let bottom = h01 + (h11 - h01) * fx;
        top + (bottom - top) * fy
    }

    pub fn attribute(&self, x: usize, y: usize) -> u16 {
        if x >= SIZE || y >= SIZE {
            return attr::NO_MOVE;
        }
        self.attributes[y * SIZE + x]
    }

    pub fn walkable(&self, x: usize, y: usize) -> bool {
        self.attribute(x, y) & (attr::NO_MOVE | attr::NO_GROUND) == 0
    }
}

/// Carpeta de datos (`WorldN` / `ObjectN`) de un mapa (número de carpeta = `WorldActive` + 1):
/// todos los niveles de Blood Castle usan los del primero, igual que Chaos Castle, Kalima e
/// Illusion Temple (`iMapWorld` de MapManager.cpp:1132-1149 y 1238-1255).
pub fn data_world(world: u16) -> u16 {
    match world {
        // Blood Castle 1-7 (`WD_11`-`WD_17`) y 8 (`WD_52`).
        12..=18 | 53 => 12,
        // Chaos Castle 1-6 (`WD_18`-`WD_23`) y 7 (`WD_53`).
        19..=24 | 54 => 19,
        // Kalima 1-6 (`WD_24`-`WD_29`) y 7 (`WD_36`).
        25..=30 | 37 => 25,
        // Illusion Temple 1-6 (`WD_45`-`WD_50`): `WD_45CURSEDTEMPLE_LV1 + 2`.
        46..=51 => 47,
        w => w,
    }
}

/// Solo las banderas de las casillas (ver [`attr`]) de `World{world}`, sin alturas ni texturas: lo que
/// necesita el servidor.
pub fn load_attributes(data_dir: &Path, world: u16) -> Result<Vec<u16>> {
    let folder = data_world(world);
    let dir = fs::find(data_dir, &format!("World{folder}"))
        .ok_or_else(|| Error::NotFound(data_dir.join(format!("World{folder}"))))?;
    read_attributes(&fs::read(&dir, &format!("EncTerrain{folder}.att"))?)
}

/// Carga el terreno de `data_dir/World{world}` (con la carpeta de [`data_world`]).
pub fn load(data_dir: &Path, world: u16) -> Result<Terrain> {
    let folder = data_world(world);
    let dir = fs::find(data_dir, &format!("World{folder}"))
        .ok_or_else(|| Error::NotFound(data_dir.join(format!("World{folder}"))))?;

    let heights = read_heights(&fs::read(&dir, "TerrainHeight.OZB")?)?;
    let mapping = read_mapping(&fs::read(&dir, &format!("EncTerrain{folder}.map"))?)?;

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
    for (index, folder, name) in tile_textures(world) {
        if used[index as usize] {
            let from = folder.and_then(|f| fs::find(data_dir, f)).unwrap_or_else(|| dir.clone());
            textures[index as usize] = texture::load(&from, &name).ok();
        }
    }

    let attributes = fs::read(&dir, &format!("EncTerrain{folder}.att"))
        .and_then(|b| read_attributes(&b))
        .unwrap_or_else(|_| vec![0; CELLS]);

    Ok(Terrain { world, dir, heights, light, mapping, attributes, textures })
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

    #[test]
    fn carpetas_de_datos() {
        // Blood Castle 3, Chaos Castle 7, Kalima 7 e Illusion Temple 1 van a la carpeta del primero.
        assert_eq!(data_world(14), 12);
        assert_eq!(data_world(54), 19);
        assert_eq!(data_world(37), 25);
        assert_eq!(data_world(46), 47);
        assert_eq!(data_world(1), 1);
        assert_eq!(data_world(52), 52);
    }

    #[test]
    fn texturas_del_suelo_por_mapa() {
        let name = |world: u16, i: usize| tile_textures(world)[i].2.clone();
        assert_eq!(name(1, 10), "TileRock04.ozj");
        assert_eq!(name(52, 2), "AlphaTileGround01.ozt");
        assert_eq!(name(40, 3), "AlphaTileGround02.ozt");
        assert_eq!(name(47, 4), "AlphaTileGround03.ozt");
        assert_eq!(name(71, 10), "AlphaTile01.ozt");
        assert_eq!(name(81, 12), "AlphaTile01.ozt");
        assert_eq!(tile_textures(64)[11].1, Some("Object64"));
        assert_eq!(alpha_tested_tile(40), Some(3));
        assert_eq!(alpha_tested_tile(52), Some(2));
        assert_eq!(alpha_tested_tile(1), None);
    }

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
        // La plaza de Lorencia (~135,128) es zona segura y caminable; el foso no.
        assert!(t.attribute(135, 128) & attr::SAFE_ZONE != 0, "plaza: {:#06X}", t.attribute(135, 128));
        assert!(t.walkable(135, 128));
        let bloqueadas = t.attributes.iter().filter(|&&a| a & attr::NO_MOVE != 0).count();
        assert!(bloqueadas > 1000 && bloqueadas < CELLS / 2 + CELLS / 4, "casillas bloqueadas: {bloqueadas}");
        let alt = t.heights.iter().cloned().fold(0.0, f32::max);
        eprintln!(
            "Lorencia: .map v{} n{}, capa1 usa {usadas:?}, altura máx {alt}",
            t.mapping.version, t.mapping.map_number
        );
    }
}
