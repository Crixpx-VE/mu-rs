//! Move window list (`M`, `CNewUIMoveCommandWindow`) and gates (`Gate.bmd`).
//!
//! # The list (`MOVEREQINFO`)
//!
//! The S6 client reads it from `Data/Local/<lang>/movereq_<lang>.bmd` (ZzzOpenData.cpp:5424,
//! `CMoveCommandData::Create`, MoveCommandData.cpp:23-42):
//!
//! ```text
//! <n i32> n × MOVEREQINFO (84 bytes, #pragma pack(1), each one with BuxConvert):
//!   <index i32> <name 32> <short name 32> <level i32> <max level i32> <zen i32> <gate i32>
//! ```
//!
//! `BuxConvert` (MoveCommandData.cpp:51-56) XORs each byte with `FC CF AB` (position % 3,
//! counting from the start of each record).
//!
//! The `dist` data (MuMain's) does not include that file: MuMain replaced it with
//! `Data/Maps/Warps.json` ("follows Webzen's Season 6", `docs/warp-list.md`), with the same fields
//! (`name.en` = name, `command` = short name, `level`, `zen`, `gate`) and in window
//! order. [`load`] looks for the original `.bmd` first and, if missing, the `.json`.
//!
//! # The gates (`GATE_ATTRIBUTE`)
//!
//! `Data/Gate.bmd` (`OpenGateScript`, ZzzInfomation.cpp:160-180): `MAX_GATES` = 512 records of
//! 14 bytes (unpacked struct, _struct.h), each one with `BuxConvert`:
//!
//! ```text
//! <flag u8> <map u8> <x1 u8> <y1 u8> <x2 u8> <y2 u8> <target u16> <angle u8> <padding>
//! <level u16> <max level u16>
//! ```
//!
//! The gate of a `MOVEREQINFO` is an arrival gate (`flag` 0 or 2): the server places the character
//! on a tile of the rectangle, facing `angle`.

use std::path::Path;

use crate::{Error, Result, fs};

/// `BuxConvert` key (MoveCommandData.cpp:53, ZzzInfomation.cpp `BuxConvert`).
const BUX_CODE: [u8; 3] = [0xFC, 0xCF, 0xAB];

/// `BuxConvert`: XOR with `FC CF AB` (its own inverse).
pub fn bux_convert(buf: &mut [u8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b ^= BUX_CODE[i % 3];
    }
}

/// A move window destination (`MOVEREQINFO`, MoveCommandData.h:13-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveReq {
    /// `index`: what the client sends in `SendRequestMoveMap`.
    pub index: i32,
    /// `szMainMapName`: the name the window shows.
    pub name: String,
    /// `szSubMapName`: the `/move` name.
    pub sub_name: String,
    /// `iReqLevel`.
    pub level: i32,
    /// `m_iReqMaxLevel` (unused by the window).
    pub max_level: i32,
    /// `iReqZen`.
    pub zen: i32,
    /// `iGateNum`: the arrival gate (only the server uses it).
    pub gate: i32,
}

const MOVEREQ_LEN: usize = 4 + 32 + 32 + 4 * 4;

/// Reads a `movereq_<lang>.bmd`.
pub fn read_movereq_bmd(buf: &[u8]) -> Result<Vec<MoveReq>> {
    let count = buf.get(..4).map(|b| i32::from_le_bytes(b.try_into().unwrap())).ok_or_else(|| Error::Format("movereq vacío".into()))?;
    let count = usize::try_from(count).map_err(|_| Error::Format(format!("movereq: {count} destinos")))?;
    let body = &buf[4..];
    if body.len() < count * MOVEREQ_LEN {
        return Err(Error::Format(format!("movereq: {count} destinos no caben en {} bytes", body.len())));
    }
    let int = |r: &[u8], i: usize| i32::from_le_bytes(r[i..i + 4].try_into().unwrap());
    let string = |r: &[u8]| {
        let end = r.iter().position(|&c| c == 0).unwrap_or(r.len());
        // Windows-1252: non-ASCII bytes stay as the equivalent Latin-1 character.
        r[..end].iter().map(|&c| c as char).collect::<String>()
    };
    Ok(body
        .chunks_exact(MOVEREQ_LEN)
        .take(count)
        .map(|chunk| {
            let mut r = [0u8; MOVEREQ_LEN];
            r.copy_from_slice(chunk);
            bux_convert(&mut r);
            MoveReq {
                index: int(&r, 0),
                name: string(&r[4..36]),
                sub_name: string(&r[36..68]),
                level: int(&r, 68),
                max_level: int(&r, 72),
                zen: int(&r, 76),
                gate: int(&r, 80),
            }
        })
        .collect())
}

/// Writes a `movereq_<lang>.bmd` (for tests and to regenerate the original).
pub fn write_movereq_bmd(list: &[MoveReq]) -> Vec<u8> {
    let mut out = (list.len() as i32).to_le_bytes().to_vec();
    for m in list {
        let mut r = [0u8; MOVEREQ_LEN];
        r[0..4].copy_from_slice(&m.index.to_le_bytes());
        for (at, s) in [(4, &m.name), (36, &m.sub_name)] {
            let n = s.len().min(31);
            r[at..at + n].copy_from_slice(&s.as_bytes()[..n]);
        }
        for (i, v) in [m.level, m.max_level, m.zen, m.gate].into_iter().enumerate() {
            r[68 + 4 * i..72 + 4 * i].copy_from_slice(&v.to_le_bytes());
        }
        bux_convert(&mut r);
        out.extend_from_slice(&r);
    }
    out
}

/// Reads MuMain's `Data/Maps/Warps.json` (`formatVersion` 1). The rules MuMain adds
/// (`noClassDiscount`, `needsOneOf`...) are not read: the window applies them like the original, by
/// name and index (`SettingCanMoveMap`).
pub fn read_warps_json(text: &str) -> Result<Vec<MoveReq>> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| Error::Format(format!("Warps.json: {e}")))?;
    let warps = v.get("warps").and_then(|w| w.as_array()).ok_or_else(|| Error::Format("Warps.json sin `warps`".into()))?;
    warps
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let int = |k: &str| w.get(k).and_then(|x| x.as_i64()).map(|x| x as i32);
            let index = int("index").ok_or_else(|| Error::Format(format!("warps[{i}] sin index")))?;
            let name = w
                .get("name")
                .and_then(|n| n.get("en"))
                .and_then(|n| n.as_str())
                .ok_or_else(|| Error::Format(format!("warps[{i}] sin name.en")))?;
            // The original stores at most 31 characters (`char[32]`).
            let cut = |s: &str| s.chars().take(31).collect::<String>();
            Ok(MoveReq {
                index,
                name: cut(name),
                sub_name: cut(w.get("command").and_then(|c| c.as_str()).unwrap_or(name)),
                level: int("level").unwrap_or(0),
                // Not in the .json; all entries of the original .bmd have 400.
                max_level: 400,
                zen: int("zen").unwrap_or(0),
                gate: int("gate").unwrap_or(0),
            })
        })
        .collect()
}

/// The list from the `Data` folder: `Local/Eng/movereq_eng.bmd` (the original's) or, if missing,
/// `Maps/Warps.json` (MuMain's).
pub fn load(data: &Path) -> Result<Vec<MoveReq>> {
    match fs::read(&data.join("Local").join("Eng"), "movereq_eng.bmd") {
        Ok(b) => read_movereq_bmd(&b),
        Err(Error::NotFound(_)) => {
            let b = fs::read(&data.join("Maps"), "Warps.json")?;
            read_warps_json(&String::from_utf8_lossy(&b))
        }
        Err(e) => Err(e),
    }
}

/// `MAX_GATES` (_define.h).
pub const MAX_GATES: usize = 512;
const GATE_LEN: usize = 14;

/// A `Gate.bmd` gate (`GATE_ATTRIBUTE`, _struct.h).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    /// 0 arrival, 1 entrance, 2 entrance and arrival (the stairs ones).
    pub flag: u8,
    /// `WorldActive` (the folder is `World{map + 1}`).
    pub map: u8,
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
    /// Arrival gate of an entrance.
    pub target: u16,
    /// Where the character faces on arrival (the client rotates `(angle - 1) * 45`).
    pub angle: u8,
    pub level: u16,
    pub max_level: u16,
}

impl Gate {
    /// The rectangle with sorted corners (some `Gate.bmd` gates have them reversed,
    /// e.g. Devias 72: (23, 27)-(27, 24)).
    pub fn area(&self) -> (u8, u8, u8, u8) {
        (self.x1.min(self.x2), self.y1.min(self.y2), self.x1.max(self.x2), self.y1.max(self.y2))
    }
}

/// Reads `Gate.bmd`: always [`MAX_GATES`] gates (those missing from the file, empty).
pub fn read_gates(buf: &[u8]) -> Vec<Gate> {
    let mut gates = vec![Gate::default(); MAX_GATES];
    for (g, chunk) in gates.iter_mut().zip(buf.chunks_exact(GATE_LEN)) {
        let mut r = [0u8; GATE_LEN];
        r.copy_from_slice(chunk);
        bux_convert(&mut r);
        let word = |i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
        *g = Gate {
            flag: r[0],
            map: r[1],
            x1: r[2],
            y1: r[3],
            x2: r[4],
            y2: r[5],
            target: word(6),
            angle: r[8],
            level: word(10),
            max_level: word(12),
        };
    }
    gates
}

/// `Data/Gate.bmd`.
pub fn load_gates(data: &Path) -> Result<Vec<Gate>> {
    Ok(read_gates(&fs::read(data, "Gate.bmd")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn dist_data() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data")
    }

    #[test]
    fn bmd_ida_y_vuelta() {
        let list = vec![
            MoveReq { index: 2, name: "Lorencia".into(), sub_name: "Lorencia".into(), level: 10, max_level: 400, zen: 2000, gate: 17 },
            MoveReq { index: 42, name: "Vulcanus".into(), sub_name: "Vulcanus".into(), level: 30, max_level: 400, zen: 15000, gate: 294 },
        ];
        let bytes = write_movereq_bmd(&list);
        assert_eq!(bytes.len(), 4 + 2 * 84);
        // The first index is encrypted: 02 00 00 00 ^ FC CF AB FC.
        assert_eq!(&bytes[4..8], &[0x02 ^ 0xFC, 0xCF, 0xAB, 0xFC]);
        assert_eq!(read_movereq_bmd(&bytes).unwrap(), list);
        assert!(read_movereq_bmd(&bytes[..100]).is_err());
    }

    /// Main 5.2's `movereq_eng.bmd` (if present in the references).
    #[test]
    fn movereq_del_main52() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../referencias/Main5.2/Source Main 5.2/bin/Data/Local/Eng/MoveReq_eng.bmd");
        let Ok(bytes) = std::fs::read(&path) else { return };
        let list = read_movereq_bmd(&bytes).unwrap();
        assert_eq!(list.len(), 37);
        let lorencia = list.iter().find(|m| m.name == "Lorencia").unwrap();
        assert_eq!((lorencia.index, lorencia.zen, lorencia.gate), (2, 2000, 17));
    }

    /// The `dist` `Warps.json` (the one the client reads).
    #[test]
    fn warps_json_de_dist() {
        let Ok(list) = load(&dist_data()) else {
            eprintln!("sin {}: no se prueba", dist_data().display());
            return;
        };
        assert_eq!(list.len(), 37);
        assert_eq!(list[0], MoveReq { index: 1, name: "Arena".into(), sub_name: "Arena".into(), level: 50, max_level: 400, zen: 2000, gate: 50 });
        assert_eq!(list[1].name, "Lorencia");
        assert_eq!((list[1].index, list[1].level, list[1].zen, list[1].gate), (2, 10, 2000, 17));
        let icarus = list.iter().find(|m| m.name == "Icarus").unwrap();
        assert_eq!((icarus.index, icarus.level, icarus.zen, icarus.gate), (23, 170, 10000, 63));
        let last = list.last().unwrap();
        assert_eq!((last.index, last.name.as_str(), last.zen, last.gate), (42, "Vulcanus", 15000, 294));
        // They match OpenMU's `CreateWarpEntries` (VersionSeasonSix/Gates.cs).
        let swamp = list.iter().find(|m| m.index == 33).unwrap();
        assert_eq!((swamp.name.as_str(), swamp.level, swamp.zen, swamp.gate), ("PeaceSwamp", 400, 15000, 273));
    }

    #[test]
    fn puertas_de_dist() {
        let Ok(gates) = load_gates(&dist_data()) else {
            eprintln!("sin Gate.bmd: no se prueba");
            return;
        };
        assert_eq!(gates.len(), MAX_GATES);
        // Lorencia (17) and Noria (27), as in OpenMU's `CreateTargetGates`.
        let g = gates[17];
        assert_eq!((g.flag, g.map, g.x1, g.y1, g.x2, g.y2), (0, 0, 133, 118, 151, 135));
        let g = gates[27];
        assert_eq!((g.map, g.x1, g.y1, g.x2, g.y2), (3, 171, 108, 177, 117));
        // Icarus (63): map 10, facing 5.
        assert_eq!((gates[63].map, gates[63].angle), (10, 5));
        // Gate 72 has its corners reversed.
        assert_eq!(gates[72].area(), (23, 24, 27, 27));
        // Every gate in the list exists and is not an entrance.
        for m in load(&dist_data()).unwrap_or_default() {
            let g = gates[m.gate as usize];
            assert_ne!(g.flag, 1, "{} lleva a una puerta de entrada", m.name);
            assert!(g.x2 > 0 && g.y2 > 0, "{}: puerta {} vacía", m.name, m.gate);
        }
    }
}
