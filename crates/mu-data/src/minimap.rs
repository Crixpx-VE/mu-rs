//! Marcas del minimapa (la ventana del Tab, `CNewUIMiniMap`):
//! `Data/Local/<Idioma>/Minimap/Minimap_WorldN_<idioma>.bmd`.
//!
//! `CNewUIMiniMap::LoadImages` (NewUIMiniMap.cpp:210-272) lee de una vez
//! `sizeof(MINI_MAP) × MAX_MINI_MAP_DATA + 45` bytes y detrás un `DWORD` con la suma de
//! `GenerateCheckSum2(…, 0x2BC1)` (ZzzInfomation.h:108-131); si no cuadra dice "File corrupted." y
//! cierra el juego. Cada registro va con `BuxConvert` (XOR `FC CF AB`, empezando de nuevo en cada
//! uno). Los 45 bytes del final entran en la suma pero no se usan.
//!
//! `MINI_MAP` (_struct.h:190-196), con el alineamiento normal de 4 (fuera del `pack(1)`):
//!
//! | desplazamiento | campo                                         |
//! |----------------|-----------------------------------------------|
//! | 0              | `BYTE Kind` (1 = NPC, 2 = puerta/portal)      |
//! | 1..4           | relleno                                       |
//! | 4, 8           | `int Location[2]`: casilla X e Y              |
//! | 12             | `int Rotation`: giro del icono en grados       |
//! | 16..116        | `char Name[100]` (Windows-1252, acaba en 0)    |
//!
//! La lista acaba en el primer registro con `Kind == 0` (los bucles del original paran ahí).
//!
//! Que un mapa tenga minimapa no depende de este archivo sino de que exista
//! `Data/WorldN/mini_map.ozt` (`m_bSuccess`): sin marcas el mapa se ve igual, vacío.

use std::path::Path;

use crate::{Error, Result, fs};

/// `MAX_MINI_MAP_DATA` (_define.h:346).
pub const MAX_MINI_MAP_DATA: usize = 100;
/// `sizeof(MINI_MAP)`.
pub const RECORD_SIZE: usize = 116;
/// Lo que entra en la suma: los 100 registros y 45 bytes más.
const BODY: usize = RECORD_SIZE * MAX_MINI_MAP_DATA + 45;
/// Clave de `GenerateCheckSum2` (NewUIMiniMap.cpp:249).
const CHECKSUM_KEY: u16 = 0x2BC1;
/// `bBuxCode` (WSclient.cpp:202).
const BUX: [u8; 3] = [0xFC, 0xCF, 0xAB];

/// `Kind` de un NPC (icono `mini_map_ui_npc`).
pub const KIND_NPC: u8 = 1;
/// `Kind` de una puerta o portal (icono `mini_map_ui_portal`).
pub const KIND_PORTAL: u8 = 2;

/// Un registro `MINI_MAP`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub kind: u8,
    /// `Location[0]` y `Location[1]`: la casilla X e Y del mapa (0..256).
    pub location: [i32; 2],
    /// `Rotation`: grados que se gira el icono (`RenderPointRotate`, `Rotate_Loc`).
    pub rotation: i32,
    pub name: String,
}

/// `GenerateCheckSum2` (ZzzInfomation.h:108-131): bloques de 4 bytes que se van sumando o
/// haciendo XOR, alternando, con un remolino cada 16 bytes. El último bloque incompleto no entra.
pub fn checksum2(buf: &[u8], key: u16) -> u32 {
    let k = key as u32;
    let mut result = k << 9;
    let mut checked = 0usize;
    while checked + 4 <= buf.len() {
        let v = u32::from_le_bytes([buf[checked], buf[checked + 1], buf[checked + 2], buf[checked + 3]]);
        if (checked / 4 + key as usize) % 2 == 0 {
            result ^= v;
        } else {
            result = result.wrapping_add(v);
        }
        if checked % 16 == 0 {
            result ^= k.wrapping_add(result) >> ((checked / 4) % 8 + 1);
        }
        checked += 4;
    }
    result
}

/// Lee un `Minimap_WorldN_<idioma>.bmd` entero.
pub fn read(buf: &[u8]) -> Result<Vec<Marker>> {
    if buf.len() < BODY + 4 {
        return Err(Error::Format(format!("minimapa de {} bytes, se esperaban {}", buf.len(), BODY + 4)));
    }
    let stored = u32::from_le_bytes([buf[BODY], buf[BODY + 1], buf[BODY + 2], buf[BODY + 3]]);
    if stored != checksum2(&buf[..BODY], CHECKSUM_KEY) {
        return Err(Error::Format("minimapa: File corrupted. (la suma no cuadra)".into()));
    }
    let mut out = Vec::new();
    for rec in buf[..RECORD_SIZE * MAX_MINI_MAP_DATA].chunks_exact(RECORD_SIZE) {
        let d: Vec<u8> = rec.iter().enumerate().map(|(i, &b)| b ^ BUX[i % 3]).collect();
        if d[0] == 0 {
            break;
        }
        let int = |o: usize| i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
        let name = &d[16..RECORD_SIZE];
        let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
        out.push(Marker {
            kind: d[0],
            location: [int(4), int(8)],
            rotation: int(12),
            name: name[..end].iter().map(|&b| cp1252(b)).collect(),
        });
    }
    Ok(out)
}

/// `Data/Local/<lang>/Minimap/Minimap_World<world>_<lang>.bmd` (NewUIMiniMap.cpp:230), con
/// `world` la carpeta del mapa (`WorldActive + 1`) y `lang` el idioma (`g_strSelectedML`, "Eng").
pub fn load(data: &Path, world: u16, lang: &str) -> Result<Vec<Marker>> {
    let dir = fs::find(data, "Local")
        .and_then(|d| fs::find(&d, lang))
        .and_then(|d| fs::find(&d, "Minimap"))
        .ok_or_else(|| Error::NotFound(data.join("Local").join(lang).join("Minimap")))?;
    read(&fs::read(&dir, &format!("Minimap_World{world}_{lang}.bmd"))?)
}

/// Si el mapa tiene minimapa: `Data/WorldN/mini_map.ozt` existe (`m_bSuccess`, NewUIMiniMap.cpp:214-228).
pub fn has_minimap(data: &Path, world: u16) -> bool {
    fs::find(data, &format!("World{world}")).and_then(|d| fs::find(&d, "mini_map.ozt")).is_some()
}

/// Un byte de Windows-1252 (los nombres van con el `char` del cliente).
fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8D}', 'Ž', '\u{8F}', '\u{90}', '‘',
        '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9D}', 'ž', 'Ÿ',
    ];
    match b {
        0x80..=0x9F => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data")
    }

    /// Cifra un registro como lo haría la herramienta de Webzen (para probar sin datos).
    fn record(kind: u8, x: i32, y: i32, rot: i32, name: &str) -> Vec<u8> {
        let mut r = vec![0u8; RECORD_SIZE];
        r[0] = kind;
        r[4..8].copy_from_slice(&x.to_le_bytes());
        r[8..12].copy_from_slice(&y.to_le_bytes());
        r[12..16].copy_from_slice(&rot.to_le_bytes());
        r[16..16 + name.len()].copy_from_slice(name.as_bytes());
        r.iter().enumerate().map(|(i, &b)| b ^ BUX[i % 3]).collect()
    }

    #[test]
    fn archivo_sintetico() {
        let mut buf = record(2, 10, 20, 45, "Gate");
        buf.extend(record(1, 121, 108, 0, "Trainer"));
        buf.extend(record(0, 0, 0, 0, ""));
        buf.resize(BODY, 0);
        let sum = checksum2(&buf, CHECKSUM_KEY);
        buf.extend(sum.to_le_bytes());
        let m = read(&buf).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], Marker { kind: 2, location: [10, 20], rotation: 45, name: "Gate".into() });
        assert_eq!(m[1].name, "Trainer");
        // Un byte cambiado: la suma ya no cuadra.
        buf[50] ^= 1;
        assert!(read(&buf).is_err());
    }

    #[test]
    fn lorencia_real() {
        let data = data();
        if !data.exists() {
            return;
        }
        assert!(has_minimap(&data, 1));
        let m = load(&data, 1, "Eng").expect("Minimap_World1_eng.bmd");
        assert_eq!(m.len(), 15);
        assert_eq!(m[0], Marker { kind: KIND_NPC, location: [121, 108], rotation: 0, name: "Trainer".into() });
        assert_eq!(m[4].name, "Hanzo the Blacksmith");
        assert_eq!(m[4].location, [114, 141]);
        assert!(m.iter().all(|k| k.kind == KIND_NPC));
    }

    /// Todos los mapas con `mini_map.ozt` tienen su lista en inglés y se lee sin errores.
    #[test]
    fn todos_los_mapas() {
        let data = data();
        if !data.exists() {
            return;
        }
        let with: Vec<u16> = (1..=100).filter(|&w| has_minimap(&data, w)).collect();
        assert_eq!(with, [1, 2, 3, 4, 5, 8, 9, 11, 32, 34, 35, 38, 39, 42, 52, 57, 58, 64, 81, 82]);
        for w in with {
            let m = load(&data, w, "Eng").unwrap_or_else(|e| panic!("World{w}: {e}"));
            assert!(!m.is_empty(), "World{w} sin marcas");
            for k in &m {
                assert!(matches!(k.kind, KIND_NPC | KIND_PORTAL), "World{w}: {k:?}");
                assert!((0..256).contains(&k.location[0]) && (0..256).contains(&k.location[1]), "World{w}: {k:?}");
                println!("World{w}: {k:?}");
            }
        }
    }
}
