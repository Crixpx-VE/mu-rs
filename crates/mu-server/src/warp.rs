//! Map change through the `M` window (`WarpCommandRequest`): the destination list, their arrival
//! gates and the rules for being allowed to go.
//!
//! - The list is the same one the client shows (`mu_data::warps::load`: the original
//!   `movereq_eng.bmd` or MuMain's `Maps/Warps.json`): index → level, zen and gate. The original
//!   GameServer reads its own (`MoveReq.txt`) with the same fields; OpenMU copies it in
//!   `CreateWarpEntries` (VersionSeasonSix/Gates.cs).
//! - The gate (`Gate.bmd`) gives the map, the arrival rectangle and the facing direction.
//! - The character lands on a random walkable tile of the rectangle (`EncTerrainN.att`), like
//!   OpenMU's `GetRandomPoint` (ExitGateExtensions.cs) plus the terrain check of the original
//!   GameServer.
//!
//! Walk-in gates (`EnterGateRequest`, [`Warps::enter_gate`]) follow OpenMU's `WarpGateHandlerPlugIn`
//! and `WarpGateAction.IsWarpLegitAsync`: an entrance gate (`Flag` 1, the one `CheckGate` checks,
//! ZzzInterface.cpp:7033) of the current map, that gate's level (with the class discount) and
//! being within 12 tiles of its rectangle (`GateEntryTolerance`: the server stores the destination
//! tile of each walk, not the current position). No zen. Leads to its arrival gate.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use mu_data::terrain::{self, attr};
use mu_data::warps::{Gate, MoveReq};

use crate::classes::ClassIndex;

pub struct Warps {
    list: Vec<MoveReq>,
    gates: Vec<Gate>,
    /// Terrain attributes of each arrival map (`WorldActive` → 256 × 256).
    attributes: HashMap<u8, Vec<u16>>,
    seed: AtomicU64,
}

/// Where the character lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Landing {
    pub map: u8,
    pub x: u8,
    pub y: u8,
    /// `GATE_ATTRIBUTE::Angle` as is (the client rotates `(angle - 1) * 45`).
    pub angle: u8,
}

/// Why the character cannot go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    UnknownWarp,
    Level { needed: i32 },
    Money { needed: i32 },
    /// Icarus (map 10) requires wings, Dinorant or Fenrir (`SettingCanMoveMap`,
    /// NewUIMoveCommandWindow.cpp:354-380); there is no equipment yet.
    NeedsWings,
    /// The gate does not exist in `Gate.bmd`.
    BadGate,
    /// Not an entrance gate of the current map.
    NotEntrance,
    /// Too far from the gate (more than [`GATE_ENTRY_TOLERANCE`] tiles).
    TooFar,
}

/// OpenMU's `WarpGateAction.GateEntryTolerance`: tiles the character may be outside the rectangle.
pub const GATE_ENTRY_TOLERANCE: i32 = 12;

/// Classes with the level reduced to 2/3 (`SettingCanMoveMap`, NewUIMoveCommandWindow.cpp:337-345):
/// Magic Gladiator (3), Dark Lord (4) and Rage Fighter (6), except level-400 destinations.
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
        // Arrival maps of the list and of the entrance gates.
        let targets = list.iter().map(|m| m.gate as usize).chain(gates.iter().filter(|g| g.flag == 1).map(|g| g.target as usize));
        for target in targets {
            let Some(g) = gates.get(target) else { continue };
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

    /// Random number (xorshift64).
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

    /// The destination with that name or sub-name, case-insensitive (`GetMapIndexFromMovereq`,
    /// NewUIMoveCommandWindow.cpp:953-970, which uses `stricmp`).
    pub fn find_by_name(&self, name: &str) -> Option<&MoveReq> {
        self.list.iter().find(|m| m.name.eq_ignore_ascii_case(name) || m.sub_name.eq_ignore_ascii_case(name))
    }

    /// Checks whether a character can go to `index` and, if so, where it lands and how much it pays.
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

    /// `EnterGateRequest`: can someone on `map` (`pos`, the tile the server knows) pass through gate
    /// `index`? If so, where they land (its arrival gate).
    pub fn enter_gate(&self, index: u16, map: u8, pos: (u8, u8), class: ClassIndex, level: u16) -> Result<Landing, Refused> {
        let gate = *self.gates.get(index as usize).ok_or(Refused::BadGate)?;
        // `player.SelectedCharacter.CurrentMap.EnterGates` (WarpGateHandlerPlugIn.cs).
        if gate.flag != 1 || gate.map != map {
            return Err(Refused::NotEntrance);
        }
        // `GetEffectiveMoveLevelRequirement(enterGate.LevelRequirement)`.
        let needed = required_level(class, gate.level as i32);
        if (level as i32) < needed {
            return Err(Refused::Level { needed });
        }
        let target = *self.gates.get(gate.target as usize).ok_or(Refused::BadGate)?;
        if target.x2 == 0 && target.y2 == 0 {
            return Err(Refused::BadGate);
        }
        // `TargetGate.Map.TryGetRequirementError`: Icarus requires wings (no equipment yet).
        if target.map == ICARUS {
            return Err(Refused::NeedsWings);
        }
        let (x1, y1, x2, y2) = gate.area();
        let t = GATE_ENTRY_TOLERANCE;
        let near = |p: u8, lo: u8, hi: u8| (p as i32) >= lo as i32 - t && (p as i32) <= hi as i32 + t;
        if !(near(pos.0, x1, x2) && near(pos.1, y1, y2)) {
            return Err(Refused::TooFar);
        }
        Ok(self.land(&target))
    }

    /// A walkable tile of the gate: 50 random tries and, failing that, the first one.
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
        // Lorencia (2): level 10, 2000 zen, gate 17 (map 0, (133,118)-(151,135)).
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
        // Noria (3): map 3.
        assert_eq!(w.check(3, 1, 10, 2000).unwrap().0.map, 3);
        // `/move`: by name, case-insensitive.
        assert_eq!(w.find_by_name("lorencia").map(|m| m.index), Some(2));
        assert_eq!(w.find_by_name("NORIA").map(|m| m.index), Some(3));
        assert!(w.find_by_name("No existe").is_none());
    }

    #[test]
    fn puertas_a_pie() {
        let w = Warps::load(&data());
        if w.gates.is_empty() {
            return;
        }
        // Lorencia → Dungeon: gate 1 (map 0, (121,232)-(123,233), level 20) leads to gate 2 (map 1,
        // (107,247)-(110,247), facing 1).
        let l = w.enter_gate(1, 0, (122, 232), 1, 20).unwrap();
        assert_eq!((l.map, l.angle), (1, 1));
        assert!((107..=110).contains(&l.x) && l.y == 247, "{l:?}");
        assert_eq!(w.enter_gate(1, 0, (122, 232), 1, 19), Err(Refused::Level { needed: 20 }));
        assert!(w.enter_gate(1, 0, (122, 232), 4, 13).is_ok(), "Dark Lord: 2/3 de 20 = 13");
        assert_eq!(w.enter_gate(1, 3, (122, 232), 1, 50), Err(Refused::NotEntrance), "otro mapa");
        assert_eq!(w.enter_gate(2, 1, (108, 247), 1, 50), Err(Refused::NotEntrance), "puerta de llegada");
        assert_eq!(w.enter_gate(1, 0, (122, 200), 1, 50), Err(Refused::TooFar));
        assert!(w.enter_gate(1, 0, (122, 245), 1, 50).is_ok(), "a 12 casillas aún vale");
        assert_eq!(w.enter_gate(600, 0, (0, 0), 1, 50), Err(Refused::BadGate));
        // And back: gate 3 (Dungeon (108,248)-(109,248), no level) to Lorencia's gate 4.
        let l = w.enter_gate(3, 1, (108, 248), 1, 1).unwrap();
        assert_eq!(l.map, 0);
        assert!((121..=123).contains(&l.x) && l.y == 231, "{l:?}");
        // The Dungeon has terrain loaded (to land on a walkable tile).
        assert!(w.attributes.contains_key(&1) || w.attributes.is_empty());
    }
}
