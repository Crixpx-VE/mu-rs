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

/// Velocidad de animación por defecto de los objetos (`o->Velocity` en `CreateObject`).
pub const DEFAULT_VELOCITY: f32 = 0.16;
/// Actualizaciones por segundo de la lógica del original: `Velocity` son fotogramas por actualización.
pub const TICKS_PER_SECOND: f32 = 25.0;

/// Velocidad de la animación de un objeto.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Velocity {
    /// Fotogramas de animación por actualización (25 por segundo).
    Fixed(f32),
    /// `k / escala`: los árboles grandes se mecen más despacio.
    PerScale(f32),
}

impl Velocity {
    /// Fotogramas de animación por segundo para un objeto de escala `scale`.
    pub fn fps(self, scale: f32) -> f32 {
        let v = match self {
            Velocity::Fixed(v) => v,
            Velocity::PerScale(k) => k / scale.max(0.01),
        };
        v * TICKS_PER_SECOND
    }
}

/// Brillo de la malla aditiva (`BlendMeshLight`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Glow {
    Steady(f32),
    /// Parpadeo de fuego: un valor al azar entre `min` y `max` en cada actualización.
    Flicker { min: f32, max: f32 },
    /// Pulso suave: `sin(t · freq) · amp + base`, con `t` en segundos.
    Pulse { freq: f32, amp: f32, base: f32 },
}

/// Reglas especiales de dibujo de un tipo de objeto.
///
/// Sacadas de `CreateObject`, `MoveObject` y `UpdateObjectVisual` (`ZzzObject.cpp` del original).
/// Lo que crea partículas o luces (fuego, humo, chispas) no está aquí.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelRules {
    /// No se dibuja: solo marca un punto (fuego, humo, sitio para sentarse...).
    pub hidden: bool,
    /// Malla que se dibuja con mezcla aditiva (luces, agua, fuego): `BlendMesh`.
    pub additive_mesh: Option<usize>,
    pub glow: Glow,
    /// Malla cuya textura se desplaza (`BlendMesh` o `StreamMesh`) y cuánto por segundo (u, v).
    pub scroll: Option<(usize, [f32; 2])>,
    pub velocity: Velocity,
    /// Techo: se desvanece cuando el personaje entra en la casa (ver [`indoor_tile`]).
    pub fade_indoors: bool,
}

/// Si la textura de suelo `tile` (capa 1) bajo el personaje es "dentro de una casa"
/// (`HeroTile` en `MoveObject`): entonces los techos se desvanecen.
pub fn indoor_tile(world: u16, tile: u8) -> bool {
    match world {
        1 => tile == 4,                // Lorencia
        3 => tile == 3 || tile >= 10,  // Devias
        _ => false,
    }
}

impl Default for ModelRules {
    fn default() -> Self {
        Self {
            hidden: false,
            additive_mesh: None,
            glow: Glow::Steady(1.0),
            scroll: None,
            velocity: Velocity::Fixed(DEFAULT_VELOCITY),
            fade_indoors: false,
        }
    }
}

impl ModelRules {
    fn hidden() -> Self {
        Self { hidden: true, ..Self::default() }
    }
    fn add(mesh: usize) -> Self {
        Self { additive_mesh: Some(mesh), ..Self::default() }
    }
    /// Desplaza la textura de la malla aditiva.
    fn scroll(mut self, u: f32, v: f32) -> Self {
        self.scroll = Some((self.additive_mesh.unwrap_or(0), [u, v]));
        self
    }
    fn flicker(mut self, min: f32, max: f32) -> Self {
        self.glow = Glow::Flicker { min, max };
        self
    }
    /// `sin(WorldTime · freq_ms) · amp + base`, con la frecuencia del original (por milisegundo).
    fn pulse(mut self, freq_ms: f32, amp: f32, base: f32) -> Self {
        self.glow = Glow::Pulse { freq: freq_ms * 1000.0, amp, base };
        self
    }
    fn speed(mut self, v: f32) -> Self {
        self.velocity = Velocity::Fixed(v);
        self
    }
}

/// Reglas de un tipo de objeto. `world` es el número de carpeta (`World1` = Lorencia).
///
/// Desplazamientos de textura: el original usa `WorldTime % periodo · k`, que avanza
/// `k · 1000` unidades por segundo; aquí van ya en unidades por segundo.
pub fn model_rules(world: u16, kind: i16) -> ModelRules {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    let roof = || ModelRules { fade_indoors: true, ..r() };
    match (world, kind) {
        // Techos que desaparecen al entrar en la casa.
        (1, 125 | 126) => roof(),                // Lorencia: HouseWall05-06
        (3, 81 | 82 | 96 | 98 | 99) => roof(),   // Devias

        // Lorencia (WD_0LORENCIA)
        (1, 0 | 1) => ModelRules { velocity: Velocity::PerScale(0.4), ..r() }, // Tree01-02 (se mecen)
        (1, 52) => add(1).flicker(0.4, 0.9),                                   // Bonfire
        (1, 59) => r().speed(0.0),                                             // TreasureChest
        (1, 90) => add(1).speed(0.3),                                          // StreetLight
        (1, 96 | 97) => r().speed(0.3),                                        // Sign01-02
        (1, 98) => add(2),                                                     // Carriage01
        (1, 105) => add(3).scroll(0.0, -1.0),                                  // Waterspout (agua que cae)
        (1, 117) => add(4).flicker(0.4, 0.7),                                  // House03 (fragua)
        (1, 118) => add(8).scroll(0.0, -1.0),                                  // House04
        (1, 119) => add(2).scroll(0.0, -1.0),                                  // House05 (molino)
        (1, 122) => add(4).flicker(0.4, 0.7),                                  // HouseWall02
        (1, 130..=133) => hidden(),                                            // Light01-03 (fuego), PoseBox
        (1, 150) => add(1).speed(0.3),                                         // Candle

        // Dungeon (WD_1DUNGEON)
        (2, 22..=24) => ModelRules { scroll: Some((1, [0.0, -1.0])), ..r() },
        (2, 39 | 40 | 51 | 52 | 60) => hidden(),

        // Devias (WD_2DEVIAS)
        (3, 19 | 92 | 93) => add(0),
        (3, 54 | 56) => add(1),
        (3, 78) => add(3).flicker(0.4, 0.7),
        (3, 91 | 100) => hidden(),

        // Noria (WD_3NORIA)
        (4, 1) => add(1),
        (4, 9) => add(3),
        (4, 17 | 19 | 37) => add(0),
        (4, 18) => add(2).scroll(0.0, 1.0),
        (4, 38) => hidden(), // sitio para sentarse
        (4, 39) => add(1),
        (4, 41) => add(0).scroll(0.0, 0.5),
        (4, 42) => ModelRules { scroll: Some((0, [-2.0, 0.0])), ..r() },
        (4, 43) => ModelRules { scroll: Some((0, [2.0, 0.0])), ..r() },

        // Lost Tower (WD_4LOSTTOWER)
        (5, 18 | 23) => add(1),
        (5, 19 | 20) => add(4).scroll(-1.0, 0.0),
        (5, 24 | 25) => hidden(),

        // Exile (WD_5UNKNOWN)
        (6, 2) => add(0),
        (6, 3) => add(0).flicker(0.6, 0.9),

        // Arena (WD_6STADIUM)
        (7, 21) => add(3).scroll(0.0, -1.0),
        (7, 38) => hidden(),

        // Atlans (WD_7ATLANSE)
        (8, 22 | 39) => hidden(),
        (8, 23) => add(0).pulse(0.002, 0.3, 0.5),
        (8, 32 | 34) => add(1).pulse(0.004, 0.5, 0.5),
        (8, 38) => add(0),
        (8, 40) => add(0).pulse(0.004, 0.3, 0.5).speed(0.05),

        // Tarkan (WD_8TARKAN)
        (9, 2) => add(0).scroll(-1.0, 0.0),
        (9, 4) => add(0).pulse(0.002, 0.35, 0.65).scroll(0.0, -0.1),
        (9, 7) => add(0).pulse(0.002, 0.35, 0.65),
        (9, 8) => r().speed(DEFAULT_VELOCITY * 4.0),
        (9, 60 | 63 | 64 | 70 | 76 | 83) => hidden(),
        (9, 61 | 65 | 66) => add(1).scroll(0.0, -1.0).pulse(0.002, 0.35, 0.65),
        (9, 72) => add(0).scroll(0.0, -0.2),
        (9, 82) => add(0),

        // Heaven (WD_10HEAVEN): nubes
        (11, 0..=5) => hidden(),

        _ => r(),
    }
}

/// Qué suelta un emisor de partículas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitKind {
    /// `CreateFire(0)`: llamas (`BITMAP_FIRE`) con luz naranja.
    Fire,
    /// `CreateFire(1)`: humo claro (`BITMAP_SMOKE` subtipo 0).
    Smoke,
    /// `CreateFire(2)`: humo oscuro (`BITMAP_SMOKE` subtipo 2, se dibuja restando).
    DarkSmoke,
}

/// Un punto de un objeto que suelta partículas en cada actualización (25 por segundo).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    pub kind: EmitKind,
    /// Desplazamiento desde el objeto, en sus ejes (unidades de MU).
    pub offset: [f32; 3],
    /// Si va pegado a un hueso del modelo (que puede estar animado), cuál.
    pub bone: Option<usize>,
    /// Desplazamiento al azar en X y Z de ± este valor (la fuente salpica en un área).
    pub spread: f32,
}

impl Emitter {
    const fn at(kind: EmitKind, x: f32, y: f32, z: f32) -> Self {
        Self { kind, offset: [x, y, z], bone: None, spread: 0.0 }
    }
}

/// Emisores de partículas de un tipo de objeto (`MoveObject` / `UpdateObjectVisual` del original).
pub fn emitters(world: u16, kind: i16) -> Vec<Emitter> {
    use EmitKind::*;
    const fn fire(x: f32, y: f32, z: f32) -> Emitter {
        Emitter::at(Fire, x, y, z)
    }
    match (world, kind) {
        // Lorencia
        (1, 50) => vec![fire(0.0, 0.0, 200.0)],                                 // FireLight01
        (1, 51) => vec![fire(0.0, -30.0, 60.0)],                                // FireLight02
        (1, 52) => vec![fire(0.0, 0.0, 60.0)],                                  // Bonfire
        (1, 55) => vec![fire(-150.0, -150.0, 140.0), fire(150.0, -150.0, 140.0)], // DungeonGate
        (1, 80) => vec![fire(90.0, -200.0, 30.0), fire(90.0, 200.0, 30.0)],     // Bridge
        (1, 105) => vec![
            // Waterspout: salpicaduras pegadas a los huesos 1 y 4 del chorro (1 de cada 2 ticks).
            Emitter { kind: Smoke, offset: [0.0, -20.0, 0.0], bone: Some(1), spread: 16.0 },
            Emitter { kind: Smoke, offset: [0.0, -80.0, 0.0], bone: Some(4), spread: 16.0 },
        ],
        (1, 130) => vec![fire(0.0, 0.0, 0.0)],                // Light01
        (1, 131) => vec![Emitter::at(Smoke, 0.0, 0.0, 0.0)],     // Light02
        (1, 132) => vec![Emitter::at(DarkSmoke, 0.0, 0.0, 0.0)], // Light03
        // Dungeon
        (2, 41) => vec![fire(0.0, -30.0, 240.0)],
        (2, 42) => vec![fire(0.0, 0.0, 190.0)],
        // Devias
        (3, 66) => vec![fire(0.0, 0.0, 50.0)],
        _ => vec![],
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
