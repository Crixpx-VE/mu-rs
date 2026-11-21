//! Lista de la ventana de mover (`M`, `CNewUIMoveCommandWindow`) y puertas (`Gate.bmd`).
//!
//! # La lista (`MOVEREQINFO`)
//!
//! El cliente S6 la lee de `Data/Local/<idioma>/movereq_<idioma>.bmd` (ZzzOpenData.cpp:5424,
//! `CMoveCommandData::Create`, MoveCommandData.cpp:23-42):
//!
//! ```text
//! <n i32> n × MOVEREQINFO (84 bytes, #pragma pack(1), cada uno con BuxConvert):
//!   <index i32> <nombre 32> <nombre corto 32> <nivel i32> <nivel máx i32> <zen i32> <puerta i32>
//! ```
//!
//! `BuxConvert` (MoveCommandData.cpp:51-56) hace XOR de cada byte con `FC CF AB` (posición % 3,
//! contando desde el principio de cada registro).
//!
//! Los datos de `dist` (los de MuMain) no traen ese archivo: MuMain lo cambió por
//! `Data/Maps/Warps.json` ("follows Webzen's Season 6", `docs/warp-list.md`), con los mismos campos
//! (`name.en` = nombre, `command` = nombre corto, `level`, `zen`, `gate`) y en el orden de la
//! ventana. [`load`] busca primero el `.bmd` del original y, si no está, el `.json`.
//!
//! # Las puertas (`GATE_ATTRIBUTE`)
//!
//! `Data/Gate.bmd` (`OpenGateScript`, ZzzInfomation.cpp:160-180): `MAX_GATES` = 512 registros de
//! 14 bytes (struct sin empaquetar, _struct.h), cada uno con `BuxConvert`:
//!
//! ```text
//! <flag u8> <mapa u8> <x1 u8> <y1 u8> <x2 u8> <y2 u8> <destino u16> <ángulo u8> <relleno>
//! <nivel u16> <nivel máx u16>
//! ```
//!
//! La puerta de un `MOVEREQINFO` es una de llegada (`flag` 0 o 2): el servidor deja al personaje
//! en una casilla del rectángulo, mirando a `ángulo`.

use std::path::Path;

use crate::{Error, Result, fs};

/// Clave de `BuxConvert` (MoveCommandData.cpp:53, ZzzInfomation.cpp `BuxConvert`).
const BUX_CODE: [u8; 3] = [0xFC, 0xCF, 0xAB];

/// `BuxConvert`: XOR con `FC CF AB` (es su propio inverso).
pub fn bux_convert(buf: &mut [u8]) {
    for (i, b) in buf.iter_mut().enumerate() {
        *b ^= BUX_CODE[i % 3];
    }
}

/// Un destino de la ventana de mover (`MOVEREQINFO`, MoveCommandData.h:13-22).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveReq {
    /// `index`: lo que manda el cliente en `SendRequestMoveMap`.
    pub index: i32,
    /// `szMainMapName`: el nombre que enseña la ventana.
    pub name: String,
    /// `szSubMapName`: el nombre de `/move`.
    pub sub_name: String,
    /// `iReqLevel`.
    pub level: i32,
    /// `m_iReqMaxLevel` (la ventana no lo usa).
    pub max_level: i32,
    /// `iReqZen`.
    pub zen: i32,
    /// `iGateNum`: la puerta de llegada (solo la usa el servidor).
    pub gate: i32,
}

const MOVEREQ_LEN: usize = 4 + 32 + 32 + 4 * 4;

/// Lee un `movereq_<idioma>.bmd`.
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
        // Windows-1252: lo que no sea ASCII se queda como el carácter Latin-1 equivalente.
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

/// Escribe un `movereq_<idioma>.bmd` (para las pruebas y para volver a sacar el original).
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

/// Lee el `Data/Maps/Warps.json` de MuMain (`formatVersion` 1). Las reglas que añade MuMain
/// (`noClassDiscount`, `needsOneOf`...) no se leen: la ventana las aplica como el original, por
/// nombre e índice (`SettingCanMoveMap`).
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
            // El original guarda 31 caracteres como mucho (`char[32]`).
            let cut = |s: &str| s.chars().take(31).collect::<String>();
            Ok(MoveReq {
                index,
                name: cut(name),
                sub_name: cut(w.get("command").and_then(|c| c.as_str()).unwrap_or(name)),
                level: int("level").unwrap_or(0),
                // No está en el .json; todos los del .bmd original tienen 400.
                max_level: 400,
                zen: int("zen").unwrap_or(0),
                gate: int("gate").unwrap_or(0),
            })
        })
        .collect()
}

/// La lista de la carpeta `Data`: `Local/Eng/movereq_eng.bmd` (el del original) o, si no está,
/// `Maps/Warps.json` (el de MuMain).
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

/// Una puerta de `Gate.bmd` (`GATE_ATTRIBUTE`, _struct.h).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Gate {
    /// 0 llegada, 1 entrada, 2 entrada y llegada (las de las escaleras).
    pub flag: u8,
    /// `WorldActive` (la carpeta es `World{map + 1}`).
    pub map: u8,
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
    /// Puerta de llegada de una entrada.
    pub target: u16,
    /// Hacia dónde mira el personaje al llegar (el cliente gira `(ángulo - 1) * 45`).
    pub angle: u8,
    pub level: u16,
    pub max_level: u16,
}

impl Gate {
    /// El rectángulo con las esquinas ordenadas (alguna puerta de `Gate.bmd` las trae al revés,
    /// p. ej. la 72 de Devias: (23, 27)-(27, 24)).
    pub fn area(&self) -> (u8, u8, u8, u8) {
        (self.x1.min(self.x2), self.y1.min(self.y2), self.x1.max(self.x2), self.y1.max(self.y2))
    }
}

/// Lee `Gate.bmd`: siempre [`MAX_GATES`] puertas (las que falten en el archivo, vacías).
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
        // El índice del primero va cifrado: 02 00 00 00 ^ FC CF AB FC.
        assert_eq!(&bytes[4..8], &[0x02 ^ 0xFC, 0xCF, 0xAB, 0xFC]);
        assert_eq!(read_movereq_bmd(&bytes).unwrap(), list);
        assert!(read_movereq_bmd(&bytes[..100]).is_err());
    }

    /// El `movereq_eng.bmd` del Main 5.2 (si está en las referencias).
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

    /// El `Warps.json` de `dist` (el que lee el cliente).
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
        // Coinciden con `CreateWarpEntries` de OpenMU (VersionSeasonSix/Gates.cs).
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
        // Lorencia (17) y Noria (27), como en `CreateTargetGates` de OpenMU.
        let g = gates[17];
        assert_eq!((g.flag, g.map, g.x1, g.y1, g.x2, g.y2), (0, 0, 133, 118, 151, 135));
        let g = gates[27];
        assert_eq!((g.map, g.x1, g.y1, g.x2, g.y2), (3, 171, 108, 177, 117));
        // Icarus (63): mapa 10, mirando a 5.
        assert_eq!((gates[63].map, gates[63].angle), (10, 5));
        // La 72 trae las esquinas al revés.
        assert_eq!(gates[72].area(), (23, 24, 27, 27));
        // Todas las puertas de la lista existen y no son de entrada.
        for m in load(&dist_data()).unwrap_or_default() {
            let g = gates[m.gate as usize];
            assert_ne!(g.flag, 1, "{} lleva a una puerta de entrada", m.name);
            assert!(g.x2 > 0 && g.y2 > 0, "{}: puerta {} vacía", m.name, m.gate);
        }
    }
}
