//! Reglas de dibujo de los objetos de Elbeland (`WD_51HOME_6TH_CHAR`, carpeta World52), Swamp of
//! Calmness (`WD_56MAP_SWAMP_OF_QUIET`, World57), Santa Village (`WD_62SANTA_TOWN`, World63), Duel
//! Arena (`WD_64DUELARENA`, World65) y Loren Market (`WD_79UNITEDMARKETPLACE`, World80).
//!
//! Sacado de `GMNewTown.cpp`, `GMSwampOfQuiet.cpp`, `GMSantaTown.cpp`, `GMDuelArena.cpp` y
//! `GMUnitedMarketPlace.cpp` (Main 5.2). Lo que crea partículas, sprites, luces del suelo y sonidos
//! está en `mu-client/src/fx/kinds/objects_elbeland.rs`.
//!
//! `CreateObject` de `MapProcess` (`w_MapProcess.cpp:206-225`) llama al de **todos** los mapas
//! registrados, esté donde esté el jugador, hasta que uno devuelve `true`: Raklion, Santa (12, 13,
//! 18, 19, 21, 25), PK Field (solo en su mapa), Duel Arena (0, 1, 32), Doppelganger 2 (0-6), Empire
//! Guardian 1-4 (129-132 y 115/117; 10 en EG4) y, el último, `GMUnitedMarketPlace::CreateObject`
//! (`GMUnitedMarketPlace.cpp:48-62`), que al tipo 67 le pone `HiddenMesh = -2` (y `CreateOperate`)
//! en cualquier mapa. Por eso el tipo 67 de Elbeland (`monfence02`, 65 vallas) no se dibuja.
//! El `Angle[2] = (int)Angle[2] % 360` de Empire Guardian (129-132) no cambia nada aquí: en
//! `EncTerrain52.obj` todos los ángulos de esos tipos ya son enteros.
//!
//! Los tipos sin modelo en `Data` (Elbeland 165, Swamp 58/59/64, Santa 37/40/51, Duel 39, Market 51)
//! tampoco los dibuja el original (`AccessModel` no encuentra el archivo): se marcan ocultos.

use super::render::{Pass, Saw, flags};
use super::{Glow, ModelRules};

/// Carpetas (`WorldActive + 1`).
const ELBELAND: u16 = 52;
const SWAMP: u16 = 57;
const SANTA: u16 = 63;
const DUEL: u16 = 65;
const MARKET: u16 = 80;

/// Reglas de los mapas de este archivo (`None` = no es uno de ellos).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    Some(match (world, kind) {
        // --- Elbeland (`GMNewTown.cpp`) --------------------------------------------------------
        // `MoveObject` (108-237): puntos de fuego (0, 61), agua (54, 58, 59), niebla (60) y el águila
        // (62) no se dibujan. 67: ver arriba. 165: sin modelo.
        (ELBELAND, 0 | 54 | 58 | 59 | 60 | 61 | 62 | 67 | 165) => hidden(),
        // Muros (`RenderObject` 789-801): `IntensityTransform = 2` en todos los vértices y
        // `o->Light = (1, 1, 1)`, así que el color es `(luz del suelo + 1) · 2`, que OpenGL deja en 1:
        // la textura tal cual, sin sombreado. Una malla cada uno: luz plana (1, 1, 1).
        (ELBELAND, 4..=14 | 129) => r().passes(ELBELAND_WALL),
        // 2, 53, 55 (`MoveObject` 132-149): `BlendMeshTexCoordV += 0.015` por tick = 0.000375 por ms
        // (diente de sierra de 8000 ms = 3 vueltas enteras, no se nota el salto). La malla `_R` es su
        // propia `BlendMesh` (`RenderBody`), así que se desplaza. Se dibujan después de los personajes
        // (`RenderObjectAfterCharacter` 954-957, `RenderBody` normal).
        (ELBELAND, 2 | 53 | 55) => r().tex_v(8000.0, 0.000375),
        // 89 (170-174): `+= 0.005` por tick = 0.000125 por ms (8000 ms = 1 vuelta).
        (ELBELAND, 89) => r().tex_v(8000.0, 0.000125),
        // 56 (150-154): `BlendMesh = 0`, `BlendMeshLight = sin(WT·0.003)·0.3 + 0.5`, `Velocity = 0.05`.
        (ELBELAND, 56) => add(0).pulse(0.003, 0.3, 0.5).speed(0.05),
        // 73 (827-831): `RenderBody(TEXTURE)` y luego `RenderBody(BRIGHT | CHROME, Alpha 0.5)`.
        // TODO(original): la segunda pasada multiplica `BodyLight` (la luz del suelo) por el `Alpha`
        // 0.5 (`ZzzBMD.cpp:1122-1128`) y [`Pass::light`] solo admite una luz fija: falta el cromado.
        (ELBELAND, 73) => r(),
        // 78/79 (`RenderObjectAfterCharacter` 964-990): la malla 0 normal y otra vez sumando. Los
        // destellos de los huesos los crea el código de efectos.
        (ELBELAND, 78 | 79) => r().passes(ELBELAND_78_79),
        // 104 (832-837): todo normal y la malla 4 sumando con `BlendMeshLight = (sin(WT·0.002) + 1)·0.5`.
        (ELBELAND, 104) => r().pulse(0.002, 0.5, 0.5).passes(ELBELAND_104),
        // 110 (`RenderObjectAfterCharacter` 958-963): mallas 2 (cromado 2), 1 y 0 sumando.
        (ELBELAND, 110) => r().passes(ELBELAND_110),
        // 113/114 (838-847): todo normal y la malla 9 cromada sumando (`BITMAP_CHROME`).
        (ELBELAND, 113 | 114) => r().passes(ELBELAND_113_114),
        // 121 (848-857): mallas 2, 3 y 1 normales y la 1 sumando con `(sin(WT·0.002) + 1)·0.5`.
        // TODO(original): la malla 0 solo se dibuja cromada sumando con `Alpha = (sin(WT·0.0015) +
        // 1)·0.1 + 0.1`, que multiplica `BodyLight` (`ZzzBMD.cpp:1122-1128`): un color que cambia con
        // el tiempo en una pasada cromada no se puede expresar con [`Pass`]; no se dibuja.
        (ELBELAND, 121) => r().pulse(0.002, 0.5, 0.5).passes(ELBELAND_121),

        // --- Swamp of Calmness (`GMSwampOfQuiet.cpp`) ------------------------------------------
        // `MoveObject` (69-107): puntos de fuego, humo y niebla. `RenderObject` (204-271) solo
        // trata monstruos. 58, 59, 64: sin modelo.
        (SWAMP, 57 | 58 | 59 | 64 | 71 | 72 | 73 | 74 | 77 | 78) => hidden(),

        // --- Santa Village (`GMSantaTown.cpp`) --------------------------------------------------
        // 16 (`MoveObject` 89-94: `Velocity = 0.06`; `RenderObjectMesh` 134-143): mallas 0-2 normales
        // y la 2 cromada sumando con `BodyLight = (0, 0.5, 0.5)`.
        (SANTA, 16) => r().speed(0.06).passes(SANTA_16),
        // 26-28 (`MoveObject` 95-102): luces de colores. 37, 40, 51: sin modelo.
        (SANTA, 26 | 27 | 28 | 37 | 40 | 51) => hidden(),

        // --- Duel Arena (`GMDuelArena.cpp`) -----------------------------------------------------
        // `MoveObject` (69-94): fuego (34, 36) y humo (35). 39: sin modelo.
        (DUEL, 34 | 35 | 36 | 39) => hidden(),

        // --- Loren Market (`GMUnitedMarketPlace.cpp`) -------------------------------------------
        // 8, la fuente (`MoveObject` 96-100: `Velocity = 0.2`; `RenderAfterObjectMesh` 421-431):
        // mallas 2-4 normales; la 1 sumando con `BlendMeshLight` 0.3 y V = `-(int)WT % 10000 · 0.001`;
        // la 0 sumando con 0.3. Las mallas que no nombra no se dibujan.
        // TODO(original): la malla 0 lleva U = `sin(WT·0.0007)·0.05`, V = `sin(WT·0.001)·0.05` (un
        // vaivén, no un diente de sierra): [`Saw`] no lo expresa; se dibuja sin desplazar.
        (MARKET, 8) => ModelRules { glow: Glow::Steady(0.3), ..r() }.speed(0.2).passes(MARKET_8),
        // 54-58 (`MoveObject` 135-143): agua, fuego y luces. 51: sin modelo.
        (MARKET, 51 | 54 | 55 | 56 | 57 | 58) => hidden(),

        (ELBELAND | SWAMP | SANTA | DUEL | MARKET, _) => r(),
        _ => return None,
    })
}

const ELBELAND_WALL: &[Pass] = &[Pass::body(flags::TEXTURE).light([1.0, 1.0, 1.0]).stream(0)];
const ELBELAND_78_79: &[Pass] = &[Pass::mesh(0, flags::TEXTURE), Pass::mesh(0, flags::TEXTURE | flags::BRIGHT)];
const ELBELAND_104: &[Pass] = &[Pass::body(flags::TEXTURE), Pass::mesh(4, flags::BRIGHT).blend_mesh(4)];
const ELBELAND_110: &[Pass] = &[
    Pass::mesh(2, flags::TEXTURE | flags::BRIGHT | flags::CHROME2),
    Pass::mesh(1, flags::TEXTURE | flags::BRIGHT),
    Pass::mesh(0, flags::TEXTURE | flags::BRIGHT),
];
const ELBELAND_113_114: &[Pass] = &[Pass::body(flags::TEXTURE), Pass::mesh(9, flags::BRIGHT | flags::CHROME)];
const ELBELAND_121: &[Pass] = &[
    Pass::mesh(2, flags::TEXTURE),
    Pass::mesh(3, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE),
    Pass::mesh(1, flags::BRIGHT).blend_mesh(1),
];
const SANTA_16: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE),
    Pass::mesh(2, flags::TEXTURE),
    // Sin `MeshTexture`: `RENDER_CHROME` pone `BITMAP_CHROME` (`ZzzBMD.cpp:1164`).
    Pass::mesh(2, flags::CHROME | flags::BRIGHT).light([0.0, 0.5, 0.5]),
];
const MARKET_8: &[Pass] = &[
    Pass::mesh(2, flags::TEXTURE),
    Pass::mesh(3, flags::TEXTURE),
    Pass::mesh(4, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE | flags::BRIGHT).blend_mesh(1).tex_coord(Saw::ZERO, Saw { period: 10000.0, k: -0.001 }),
    Pass::mesh(0, flags::TEXTURE | flags::BRIGHT).blend_mesh(0),
];

#[cfg(test)]
mod tests {
    use super::super::render::{LookBlend, LookColor, MeshInfo, ObjectDraw, resolve_all};
    use super::*;
    use crate::{bmd, fs};
    use std::path::Path;

    /// El modelo de `dist/cliente/Data` (`None` si no están los datos; si están, tiene que cargar).
    fn model(world: u16, kind: i16) -> Option<bmd::Bmd> {
        let data = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data");
        if !data.exists() {
            return None;
        }
        let rel = super::super::model_path(world, kind).expect("ruta del modelo");
        let dir = data.join(rel.parent().unwrap());
        let m = fs::read(&dir, rel.file_name().unwrap().to_str().unwrap()).and_then(|b| bmd::read(&b));
        Some(m.unwrap_or_else(|e| panic!("World{world} tipo {kind}: {e}")))
    }

    /// Las mallas que nombra el código del original existen y, las que se pasan como `BlendMesh`,
    /// tienen ese índice de textura (el original compara `m->Texture == BlendMesh`).
    #[test]
    fn mallas_del_original() {
        let checks: &[(u16, i16, &[usize], &[usize])] = &[
            (ELBELAND, 104, &[4], &[4]),
            (ELBELAND, 110, &[0, 1, 2], &[]),
            (ELBELAND, 113, &[9], &[]),
            (ELBELAND, 114, &[9], &[]),
            (ELBELAND, 121, &[0, 1, 2, 3], &[1]),
            (ELBELAND, 78, &[0], &[]),
            (ELBELAND, 79, &[0], &[]),
            (SANTA, 16, &[0, 1, 2], &[]),
            (MARKET, 8, &[0, 1, 2, 3, 4], &[0, 1]),
        ];
        for &(world, kind, meshes, blend) in checks {
            let Some(m) = model(world, kind) else { return };
            for &i in meshes {
                assert!(i < m.meshes.len(), "World{world} tipo {kind}: no tiene la malla {i}");
            }
            for &i in blend {
                assert_eq!(m.meshes[i].texture_index as usize, i, "World{world} tipo {kind} malla {i}");
            }
        }
        // Los muros de Elbeland tienen una sola malla (la pasada plana es `StreamMesh = 0`).
        for kind in (4..=14).chain([129]) {
            let Some(m) = model(ELBELAND, kind) else { return };
            assert_eq!(m.meshes.len(), 1, "muro {kind}");
        }
    }

    #[test]
    fn pasadas() {
        let o = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        let mesh = |i: usize| MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: false };
        // Muro: luz plana blanca.
        let l = resolve_all(ELBELAND_WALL, &o, &mesh(0));
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].1.color, l[0].1.light, l[0].1.blend), (LookColor::Flat, Some([1.0; 3]), LookBlend::Opaque));
        // 104: la malla 4 normal y luego sumando con `BlendMeshLight`.
        let l = resolve_all(ELBELAND_104, &o, &mesh(4));
        assert_eq!(l.iter().map(|(_, l)| (l.blend, l.color)).collect::<Vec<_>>(), vec![
            (LookBlend::Opaque, LookColor::Lit),
            (LookBlend::Add, LookColor::BlendLight)
        ]);
        // 121: la malla 0 no se dibuja (ver TODO); la 1 normal y sumando.
        assert!(resolve_all(ELBELAND_121, &o, &mesh(0)).is_empty());
        assert_eq!(resolve_all(ELBELAND_121, &o, &mesh(1)).len(), 2);
        // Fuente del mercado: la malla 1 se desplaza en V; las que no nombra no se dibujan.
        let l = resolve_all(MARKET_8, &o, &mesh(1));
        assert_eq!(l.len(), 1);
        assert!((l[0].1.wave.unwrap()[1].at(2500.0) + 2.5).abs() < 1e-5);
        assert!(resolve_all(MARKET_8, &o, &mesh(5)).is_empty());
        // Desplazamiento de 2/53/55: 0.015 por tick (40 ms).
        let s = rules(ELBELAND, 2).unwrap().tex_coord[1];
        assert!((s.at(40.0) - 0.015).abs() < 1e-6);
        assert!(rules(1, 2).is_none());
    }
}
