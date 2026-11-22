//! Mover de mapa con la ventana `M` (`WarpCommandRequest`): la lista de destinos, sus puertas de
//! llegada y las reglas para poder ir.
//!
//! - La lista es la misma que enseña el cliente (`mu_data::warps::load`: `movereq_eng.bmd` del
//!   original o `Maps/Warps.json` de MuMain): índice → nivel, zen y puerta. El GameServer original
//!   lee la suya (`MoveReq.txt`) con los mismos campos; OpenMU la copia en `CreateWarpEntries`
//!   (VersionSeasonSix/Gates.cs).
//! - La puerta (`Gate.bmd`) da el mapa, el rectángulo de llegada y hacia dónde se mira.
//! - Se cae en una casilla al azar del rectángulo que se pueda pisar (`EncTerrainN.att`), como
//!   `GetRandomPoint` de OpenMU (ExitGateExtensions.cs) más la comprobación del terreno del
//!   GameServer original.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use mu_data::terrain::{self, attr};
use mu_data::warps::{Gate, MoveReq};

use crate::classes::ClassIndex;

pub struct Warps {
    list: Vec<MoveReq>,
    gates: Vec<Gate>,
    /// Atributos del terreno de cada mapa de llegada (`WorldActive` → 256 × 256).
    attributes: HashMap<u8, Vec<u16>>,
    seed: AtomicU64,
}

/// A dónde se llega.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub map: u8,
    pub x: u8,
    pub y: u8,
    /// `GATE_ATTRIBUTE::Angle` tal cual (el cliente gira `(ángulo - 1) * 45`).
    pub angle: u8,
}

/// Por qué no se puede ir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    UnknownWarp,
    Level { needed: i32 },
    Money { needed: i32 },
    /// Icarus (mapa 10) pide alas, Dinorant o Fenrir (`SettingCanMoveMap`,
    /// NewUIMoveCommandWindow.cpp:354-380); sin equipo no hay.
    NeedsWings,
    /// La puerta no existe en `Gate.bmd`.
    BadGate,
}

/// Clases con el nivel rebajado a 2/3 (`SettingCanMoveMap`, NewUIMoveCommandWindow.cpp:337-345):
/// Magic Gladiator (3), Dark Lord (4) y Rage Fighter (6), salvo los destinos de nivel 400.
pub fn required_level(class: ClassIndex, level: i32) -> i32 {
    if matches!(class, 3 | 4 | 6) && level != 400 { (level as f32 * 2.0 / 3.0) as i32 } else { level }
}

/// `WD_10HEAVEN`.
const ICARUS: u8 = 10;

impl Warps {
    pub fn load(data: &Path) -> Self {
        let list = mu_data::warps::load(data).unwrap_or_else(|e| {
            eprintln!("[warp] sin lista de destinos ({e}): no se puede mover con M");
            Vec::new()
        });
        let gates = mu_data::warps::load_gates(data).unwrap_or_else(|e| {
            eprintln!("[warp] sin Gate.bmd ({e})");
            Vec::new()
        });
        let mut attributes = HashMap::new();
        for m in &list {
            let Some(g) = gates.get(m.gate as usize) else { continue };
            if attributes.contains_key(&g.map) {
                continue;
            }
            match load_attributes(data, g.map) {
                Ok(a) => {
                    attributes.insert(g.map, a);
                }
                Err(e) => eprintln!("[warp] mapa {}: sin terreno ({e}), se cae en cualquier casilla de la puerta", g.map),
            }
        }
        eprintln!("[warp] {} destinos, {} mapas", list.len(), attributes.len());
        let seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(7, |d| d.as_nanos() as u64) | 1;
        Self { list, gates, attributes, seed: AtomicU64::new(seed) }
    }

    /// Número al azar (xorshift64).
    pub fn random(&self) -> u64 {
        let mut x = self.seed.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.seed.store(x, Ordering::Relaxed);
        x
    }

    pub fn find(&self, index: u16) -> Option<&MoveReq> {
        self.list.iter().find(|m| m.index == index as i32)
    }

    /// El destino con ese nombre o subnombre, sin mirar mayúsculas (`GetMapIndexFromMovereq`,
    /// NewUIMoveCommandWindow.cpp:953-970, que usa `stricmp`).
    pub fn find_by_name(&self, name: &str) -> Option<&MoveReq> {
        self.list.iter().find(|m| m.name.eq_ignore_ascii_case(name) || m.sub_name.eq_ignore_ascii_case(name))
    }

    /// Comprueba si un personaje puede ir a `index` y, si puede, dónde cae y cuánto paga.
    pub fn check(&self, index: u16, class: ClassIndex, level: u16, money: u32) -> Result<(Landing, u32), Refused> {
        let m = self.find(index).ok_or(Refused::UnknownWarp)?;
        let needed = required_level(class, m.level);
        if (level as i32) < needed {
            return Err(Refused::Level { needed });
        }
        if (money as i64) < m.zen as i64 {
            return Err(Refused::Money { needed: m.zen });
        }
        let gate = *self.gates.get(m.gate as usize).ok_or(Refused::BadGate)?;
        if gate.map == ICARUS {
            return Err(Refused::NeedsWings);
        }
        Ok((self.land(&gate), m.zen.max(0) as u32))
    }

    /// Una casilla de la puerta que se pueda pisar: 50 intentos al azar y, si no, la primera.
    fn land(&self, gate: &Gate) -> Landing {
        let (x1, y1, x2, y2) = gate.area();
        let walkable = |x: u8, y: u8| {
            self.attributes.get(&gate.map).is_none_or(|a| a[y as usize * terrain::SIZE + x as usize] & (attr::NO_MOVE | attr::NO_GROUND) == 0)
        };
        let (w, h) = (x2 as u64 - x1 as u64 + 1, y2 as u64 - y1 as u64 + 1);
        let random = (0..50).map(|_| {
            let r = self.random();
            (x1 + (r % w) as u8, y1 + ((r / w) % h) as u8)
        });
        let all = (y1..=y2).flat_map(|y| (x1..=x2).map(move |x| (x, y)));
        let (x, y) = random.chain(all).find(|&(x, y)| walkable(x, y)).unwrap_or((x1, y1));
        Landing { map: gate.map, x, y, angle: gate.angle }
    }
}

fn load_attributes(data: &Path, map: u8) -> mu_data::Result<Vec<u16>> {
    let folder = terrain::data_world(map as u16 + 1);
    let dir = mu_data::fs::find(data, &format!("World{folder}")).ok_or_else(|| mu_data::Error::NotFound(data.join(format!("World{folder}"))))?;
    terrain::read_attributes(&mu_data::fs::read(&dir, &format!("EncTerrain{folder}.att"))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data")
    }

    #[test]
    fn nivel_rebajado() {
        assert_eq!(required_level(1, 170), 170, "Dark Knight");
        assert_eq!(required_level(3, 170), 113, "Magic Gladiator: 2/3");
        assert_eq!(required_level(4, 10), 6, "Dark Lord");
        assert_eq!(required_level(6, 400), 400, "nivel 400 sin rebaja");
    }

    #[test]
    fn reglas_y_llegada() {
        let w = Warps::load(&data());
        if w.list.is_empty() || w.gates.is_empty() {
            return;
        }
        // Lorencia (2): nivel 10, 2000 de zen, puerta 17 (mapa 0, (133,118)-(151,135)).
        assert_eq!(w.check(2, 1, 9, 5000), Err(Refused::Level { needed: 10 }));
        assert_eq!(w.check(2, 1, 10, 1999), Err(Refused::Money { needed: 2000 }));
        assert_eq!(w.check(2, 1, 10, 0), Err(Refused::Money { needed: 2000 }), "sin zen no se va");
        assert_eq!(w.check(2, 4, 7, 2000).map(|(_, zen)| zen), Ok(2000), "Dark Lord: 2/3 de 10 = 6");
        assert_eq!(w.check(23, 1, 400, 1_000_000), Err(Refused::NeedsWings));
        assert_eq!(w.check(999, 1, 400, 1_000_000), Err(Refused::UnknownWarp));
        let attrs = w.attributes.get(&0).cloned();
        for _ in 0..200 {
            let (l, _) = w.check(2, 1, 10, 2000).unwrap();
            assert_eq!(l.map, 0);
            assert!((133..=151).contains(&l.x) && (118..=135).contains(&l.y), "{l:?}");
            if let Some(a) = &attrs {
                assert_eq!(a[l.y as usize * 256 + l.x as usize] & (attr::NO_MOVE | attr::NO_GROUND), 0, "{l:?} no se pisa");
            }
        }
        // Noria (3): mapa 3.
        assert_eq!(w.check(3, 1, 10, 2000).unwrap().0.map, 3);
        // `/move`: por nombre, sin mirar mayúsculas.
        assert_eq!(w.find_by_name("lorencia").map(|m| m.index), Some(2));
        assert_eq!(w.find_by_name("NORIA").map(|m| m.index), Some(3));
        assert!(w.find_by_name("No existe").is_none());
    }
}
