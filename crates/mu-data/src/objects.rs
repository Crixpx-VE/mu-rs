//! Objetos colocados en un mapa (`EncTerrainN.obj`) y qué modelo usa cada tipo.

use std::path::{Path, PathBuf};

use crate::reader::Reader;
use crate::{Error, Result, crypt, fs};

#[derive(Debug, Clone)]
pub struct MapObject {
    /// Tipo de modelo (índice en la tabla de modelos del mapa).
    pub kind: i16,
    /// Posición en unidades de MU (100 = una casilla), Z hacia arriba.
    pub position: [f32; 3],
    /// Rotación en grados (x, y, z).
    pub angle: [f32; 3],
    pub scale: f32,
}

pub fn read(buf: &[u8]) -> Result<Vec<MapObject>> {
    let dec = crypt::map_file_decrypt(buf);
    let mut r = Reader::new(&dec);
    let version = r.u8()?;
    let _map_number = r.u8()?;
    let count = r.i16()?.max(0) as usize;

    // Cada versión añade bytes al final de cada objeto (datos que aún no usamos).
    let extra = match version {
        0 => 0,
        1 => 2,
        2 => 3,
        3 => 3 + 12,
        4 => 3 + 12 + 1,
        5 => 3 + 12 + 1 + 8,
        v => return Err(Error::Format(format!(".obj versión {v} no soportada"))),
    };

    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = r.i16()?;
        let position = r.vec3()?;
        let angle = r.vec3()?;
        let scale = r.f32()?;
        r.skip(extra)?;
        out.push(MapObject { kind, position, angle, scale });
    }
    Ok(out)
}

/// Carga los objetos de `data_dir/World{world}/EncTerrain{world}.obj`.
pub fn load(data_dir: &Path, world: u16) -> Result<Vec<MapObject>> {
    let dir = fs::find(data_dir, &format!("World{world}"))
        .ok_or_else(|| Error::NotFound(data_dir.join(format!("World{world}"))))?;
    read(&fs::read(&dir, &format!("EncTerrain{world}.obj"))?)
}

/// Modelos de Lorencia (World1): (primer tipo, nombre, cuántos).
/// El archivo es `Data/Object1/{nombre}{n:02}.bmd`. Sacado de `MapManager.cpp` del original.
const LORENCIA_MODELS: &[(i16, &str, i16)] = &[
    (0, "Tree", 13),
    (20, "Grass", 8),
    (30, "Stone", 5),
    (40, "StoneStatue", 3),
    (43, "SteelStatue", 1),
    (44, "Tomb", 3),
    (50, "FireLight", 2),
    (52, "Bonfire", 1),
    (55, "DoungeonGate", 1),
    (56, "MerchantAnimal", 2),
    (58, "TreasureDrum", 1),
    (59, "TreasureChest", 1),
    (60, "Ship", 1),
    (65, "SteelWall", 3),
    (68, "SteelDoor", 1),
    (69, "StoneWall", 6),
    (75, "StoneMuWall", 4),
    (80, "Bridge", 1),
    (81, "Fence", 4),
    (85, "BridgeStone", 1),
    (90, "StreetLight", 1),
    (91, "Cannon", 3),
    (95, "Curtain", 1),
    (96, "Sign", 2),
    (98, "Carriage", 4),
    (102, "Straw", 2),
    (105, "Waterspout", 1),
    (106, "Well", 4),
    (110, "Hanging", 1),
    (111, "Stair", 1),
    (115, "House", 5),
    (120, "Tent", 1),
    (121, "HouseWall", 6),
    (127, "HouseEtc", 3),
    (130, "Light", 3),
    (133, "PoseBox", 1),
    (140, "Furniture", 7),
    (150, "Candle", 1),
    (151, "Beer", 3),
];

/// Reglas especiales de dibujo de un tipo de objeto (del `ZzzObject.cpp` original).
#[derive(Debug, Clone, Copy, Default)]
pub struct ModelRules {
    /// No se dibuja: solo marca un punto (fuego, luz, sitio para sentarse...).
    pub hidden: bool,
    /// Malla que se dibuja con mezcla aditiva (luces, agua, fuego).
    pub additive_mesh: Option<usize>,
}

pub fn model_rules(world: u16, kind: i16) -> ModelRules {
    let hidden = |h| ModelRules { hidden: h, additive_mesh: None };
    let add = |m| ModelRules { hidden: false, additive_mesh: Some(m) };
    match (world, kind) {
        // Lorencia
        (1, 130..=132) => hidden(true), // Light01-03: solo crean fuego
        (1, 133) => hidden(true),       // PoseBox: sitio para sentarse
        (1, 52) => add(1),              // Bonfire
        (1, 90) => add(1),              // StreetLight
        (1, 98) => add(2),              // Carriage01
        (1, 105) => add(3),             // Waterspout
        (1, 117) => add(4),             // House03
        (1, 118) => add(8),             // House04
        (1, 119) => add(2),             // House05
        (1, 122) => add(4),             // HouseWall02
        (1, 150) => add(1),             // Candle
        _ => ModelRules::default(),
    }
}

/// Ruta del modelo (`.bmd`) de un tipo de objeto en un mapa, relativa a `Data/`.
///
/// Lorencia usa nombres propios; el resto de mapas usa `Object{mapa}/Object{tipo+1:02}.bmd`.
pub fn model_path(world: u16, kind: i16) -> Option<PathBuf> {
    if world == 1 {
        let &(first, name, _) =
            LORENCIA_MODELS.iter().find(|&&(first, _, n)| kind >= first && kind < first + n)?;
        Some(PathBuf::from("Object1").join(format!("{name}{:02}.bmd", kind - first + 1)))
    } else {
        Some(PathBuf::from(format!("Object{world}")).join(format!("Object{:02}.bmd", kind + 1)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn objetos_y_modelos_de_lorencia() {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        if !data.exists() {
            return;
        }
        let objs = load(&data, 1).expect("leer EncTerrain1.obj");
        assert!(objs.len() > 100, "pocos objetos: {}", objs.len());
        // Si la lectura estuviera desalineada saldrían valores absurdos (1e30, NaN...).
        // Algunos objetos reales quedan un poco fuera del borde, así que dejamos margen.
        let dentro = |v: f32| (-2000.0..=27600.0).contains(&v);
        for o in &objs {
            assert!(dentro(o.position[0]) && dentro(o.position[1]) && o.scale > 0.0 && o.scale < 50.0, "{o:?}");
        }

        let mut por_tipo: BTreeMap<i16, usize> = BTreeMap::new();
        for o in &objs {
            *por_tipo.entry(o.kind).or_default() += 1;
        }
        let mut fallos = vec![];
        let mut sin_textura = vec![];
        for (&kind, &n) in &por_tipo {
            let Some(rel) = model_path(1, kind) else {
                fallos.push(format!("tipo {kind} ({n}x): sin modelo en la tabla"));
                continue;
            };
            let dir = data.join(rel.parent().unwrap());
            let name = rel.file_name().unwrap().to_str().unwrap();
            match crate::fs::read(&dir, name).and_then(|b| crate::bmd::read(&b)) {
                Ok(m) => {
                    for (i, mesh) in m.meshes.iter().enumerate() {
                        let posed = m.pose(i, 0, 0);
                        assert_eq!(posed.positions.len() % 3, 0);
                        let tex = crate::bmd::texture_file_name(&mesh.texture);
                        if crate::fs::find(&dir, &tex).is_none() {
                            sin_textura.push(format!("{name}: {} → {tex}", mesh.texture));
                        }
                    }
                }
                Err(e) => fallos.push(format!("tipo {kind} {name}: {e}")),
            }
        }
        // Diagnóstico: mallas con textura casi negra (candidatas a mezcla aditiva).
        for (&kind, &n) in &por_tipo {
            let Some(rel) = model_path(1, kind) else { continue };
            let dir = data.join(rel.parent().unwrap());
            let name = rel.file_name().unwrap().to_str().unwrap();
            let Ok(m) = crate::fs::read(&dir, name).and_then(|b| crate::bmd::read(&b)) else { continue };
            for (i, mesh) in m.meshes.iter().enumerate() {
                let tex = crate::bmd::texture_file_name(&mesh.texture);
                if let Ok(t) = crate::texture::load(&dir, &tex) {
                    let sum: u64 = t.rgba.chunks_exact(4).map(|p| p[0] as u64 + p[1] as u64 + p[2] as u64).sum();
                    let avg = sum as f64 / (t.rgba.len() / 4) as f64 / 765.0;
                    if avg < 0.15 {
                        eprintln!("oscura: tipo {kind} {name} malla {i} {tex} brillo {avg:.2} ({n} objetos)");
                    }
                }
            }
        }
        eprintln!("{} objetos, {} tipos", objs.len(), por_tipo.len());
        eprintln!("texturas no encontradas: {sin_textura:#?}");
        assert!(fallos.is_empty(), "{fallos:#?}");
    }
}
