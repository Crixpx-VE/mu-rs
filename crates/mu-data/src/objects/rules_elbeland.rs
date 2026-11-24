//! Draw rules for the objects of Elbeland (`WD_51HOME_6TH_CHAR`, folder World52), Swamp of
//! Calmness (`WD_56MAP_SWAMP_OF_QUIET`, World57), Santa Village (`WD_62SANTA_TOWN`, World63), Duel
//! Arena (`WD_64DUELARENA`, World65) and Loren Market (`WD_79UNITEDMARKETPLACE`, World80).
//!
//! Taken from `GMNewTown.cpp`, `GMSwampOfQuiet.cpp`, `GMSantaTown.cpp`, `GMDuelArena.cpp` and
//! `GMUnitedMarketPlace.cpp` (Main 5.2). What creates particles, sprites, ground lights and sounds
//! is in `mu-client/src/fx/kinds/objects_elbeland.rs`.
//!
//! `MapProcess`'s `CreateObject` (`w_MapProcess.cpp:206-225`) calls the one of **all** registered
//! maps, wherever the player is, until one returns `true`: Raklion, Santa (12, 13,
//! 18, 19, 21, 25), PK Field (only on its map), Duel Arena (0, 1, 32), Doppelganger 2 (0-6), Empire
//! Guardian 1-4 (129-132 and 115/117; 10 in EG4) and, last, `GMUnitedMarketPlace::CreateObject`
//! (`GMUnitedMarketPlace.cpp:48-62`), which sets `HiddenMesh = -2` (and `CreateOperate`) on type 67
//! on any map. That's why Elbeland type 67 (`monfence02`, 65 fences) is not drawn.
//! Empire Guardian's `Angle[2] = (int)Angle[2] % 360` (129-132) changes nothing here: in
//! `EncTerrain52.obj` all angles of those types are already whole numbers.
//!
//! Types without a model in `Data` (Elbeland 165, Swamp 58/59/64, Santa 37/40/51, Duel 39, Market 51)
//! aren't drawn by the original either (`AccessModel` doesn't find the file): marked hidden.

use super::render::{Pass, Saw, flags};
use super::{Glow, ModelRules};

/// Folders (`WorldActive + 1`).
const ELBELAND: u16 = 52;
const SWAMP: u16 = 57;
const SANTA: u16 = 63;
const DUEL: u16 = 65;
const MARKET: u16 = 80;

/// Rules for the maps in this file (`None` = not one of them).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    Some(match (world, kind) {
        // --- Elbeland (`GMNewTown.cpp`) --------------------------------------------------------
        // `MoveObject` (108-237): fire points (0, 61), water (54, 58, 59), mist (60) and the eagle
        // (62) are not drawn. 67: see above. 165: no model.
        (ELBELAND, 0 | 54 | 58 | 59 | 60 | 61 | 62 | 67 | 165) => hidden(),
        // Walls (`RenderObject` 789-801): `IntensityTransform = 2` on all vertices and
        // `o->Light = (1, 1, 1)`, so the color is `(ground light + 1) · 2`, which OpenGL clamps to 1:
        // the texture as is, unshaded. One mesh each: flat light (1, 1, 1).
        (ELBELAND, 4..=14 | 129) => r().passes(ELBELAND_WALL),
        // 2, 53, 55 (`MoveObject` 132-149): `BlendMeshTexCoordV += 0.015` per tick = 0.000375 per ms
        // (8000 ms sawtooth = 3 whole turns, the jump isn't noticeable). The `_R` mesh is its
        // own `BlendMesh` (`RenderBody`), so it scrolls. Drawn after the characters
        // (`RenderObjectAfterCharacter` 954-957, normal `RenderBody`).
        (ELBELAND, 2 | 53 | 55) => r().tex_v(8000.0, 0.000375),
        // 89 (170-174): `+= 0.005` per tick = 0.000125 per ms (8000 ms = 1 turn).
        (ELBELAND, 89) => r().tex_v(8000.0, 0.000125),
        // 56 (150-154): `BlendMesh = 0`, `BlendMeshLight = sin(WT·0.003)·0.3 + 0.5`, `Velocity = 0.05`.
        (ELBELAND, 56) => add(0).pulse(0.003, 0.3, 0.5).speed(0.05),
        // 73 (827-831): `RenderBody(TEXTURE)` then `RenderBody(BRIGHT | CHROME, Alpha 0.5)`.
        // TODO(original): the second pass multiplies `BodyLight` (the ground light) by the `Alpha`
        // 0.5 (`ZzzBMD.cpp:1122-1128`) and [`Pass::light`] only supports a fixed light: the chrome is missing.
        (ELBELAND, 73) => r(),
        // 78/79 (`RenderObjectAfterCharacter` 964-990): mesh 0 normal and again additive. The
        // bone flares are created by the effects code.
        (ELBELAND, 78 | 79) => r().passes(ELBELAND_78_79),
        // 104 (832-837): everything normal and mesh 4 additive with `BlendMeshLight = (sin(WT·0.002) + 1)·0.5`.
        (ELBELAND, 104) => r().pulse(0.002, 0.5, 0.5).passes(ELBELAND_104),
        // 110 (`RenderObjectAfterCharacter` 958-963): meshes 2 (chrome 2), 1 and 0 additive.
        (ELBELAND, 110) => r().passes(ELBELAND_110),
        // 113/114 (838-847): everything normal and mesh 9 chrome additive (`BITMAP_CHROME`).
        (ELBELAND, 113 | 114) => r().passes(ELBELAND_113_114),
        // 121 (848-857): meshes 2, 3 and 1 normal and mesh 1 additive with `(sin(WT·0.002) + 1)·0.5`.
        // TODO(original): mesh 0 is only drawn chrome additive with `Alpha = (sin(WT·0.0015) +
        // 1)·0.1 + 0.1`, which multiplies `BodyLight` (`ZzzBMD.cpp:1122-1128`): a time-varying color
        // in a chrome pass can't be expressed with [`Pass`]; not drawn.
        (ELBELAND, 121) => r().pulse(0.002, 0.5, 0.5).passes(ELBELAND_121),

        // --- Swamp of Calmness (`GMSwampOfQuiet.cpp`) ------------------------------------------
        // `MoveObject` (69-107): fire, smoke and mist points. `RenderObject` (204-271) only
        // handles monsters. 58, 59, 64: no model.
        (SWAMP, 57 | 58 | 59 | 64 | 71 | 72 | 73 | 74 | 77 | 78) => hidden(),

        // --- Santa Village (`GMSantaTown.cpp`) --------------------------------------------------
        // 16 (`MoveObject` 89-94: `Velocity = 0.06`; `RenderObjectMesh` 134-143): meshes 0-2 normal
        // and mesh 2 chrome additive with `BodyLight = (0, 0.5, 0.5)`.
        (SANTA, 16) => r().speed(0.06).passes(SANTA_16),
        // 26-28 (`MoveObject` 95-102): colored lights. 37, 40, 51: no model.
        (SANTA, 26 | 27 | 28 | 37 | 40 | 51) => hidden(),

        // --- Duel Arena (`GMDuelArena.cpp`) -----------------------------------------------------
        // `MoveObject` (69-94): fire (34, 36) and smoke (35). 39: no model.
        (DUEL, 34 | 35 | 36 | 39) => hidden(),

        // --- Loren Market (`GMUnitedMarketPlace.cpp`) -------------------------------------------
        // 8, the fountain (`MoveObject` 96-100: `Velocity = 0.2`; `RenderAfterObjectMesh` 421-431):
        // meshes 2-4 normal; mesh 1 additive with `BlendMeshLight` 0.3 and V = `-(int)WT % 10000 · 0.001`;
        // mesh 0 additive with 0.3. Meshes it doesn't name are not drawn.
        // TODO(original): mesh 0 has U = `sin(WT·0.0007)·0.05`, V = `sin(WT·0.001)·0.05` (a
        // sway, not a sawtooth): [`Saw`] can't express it; drawn without scrolling.
        (MARKET, 8) => ModelRules { glow: Glow::Steady(0.3), ..r() }.speed(0.2).passes(MARKET_8),
        // 54-58 (`MoveObject` 135-143): water, fire and lights. 51: no model.
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
    // No `MeshTexture`: `RENDER_CHROME` uses `BITMAP_CHROME` (`ZzzBMD.cpp:1164`).
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

    /// The model from `dist/cliente/Data` (`None` if the data is missing; if present, it must load).
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

    /// The meshes named by the original code exist and, those passed as `BlendMesh`,
    /// have that texture index (the original compares `m->Texture == BlendMesh`).
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
        // Elbeland walls have a single mesh (the flat pass is `StreamMesh = 0`).
        for kind in (4..=14).chain([129]) {
            let Some(m) = model(ELBELAND, kind) else { return };
            assert_eq!(m.meshes.len(), 1, "muro {kind}");
        }
    }

    #[test]
    fn pasadas() {
        let o = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        let mesh = |i: usize| MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: false };
        // Wall: flat white light.
        let l = resolve_all(ELBELAND_WALL, &o, &mesh(0));
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].1.color, l[0].1.light, l[0].1.blend), (LookColor::Flat, Some([1.0; 3]), LookBlend::Opaque));
        // 104: mesh 4 normal and then additive with `BlendMeshLight`.
        let l = resolve_all(ELBELAND_104, &o, &mesh(4));
        assert_eq!(l.iter().map(|(_, l)| (l.blend, l.color)).collect::<Vec<_>>(), vec![
            (LookBlend::Opaque, LookColor::Lit),
            (LookBlend::Add, LookColor::BlendLight)
        ]);
        // 121: mesh 0 is not drawn (see TODO); mesh 1 normal and additive.
        assert!(resolve_all(ELBELAND_121, &o, &mesh(0)).is_empty());
        assert_eq!(resolve_all(ELBELAND_121, &o, &mesh(1)).len(), 2);
        // Market fountain: mesh 1 scrolls in V; meshes it doesn't name are not drawn.
        let l = resolve_all(MARKET_8, &o, &mesh(1));
        assert_eq!(l.len(), 1);
        assert!((l[0].1.wave.unwrap()[1].at(2500.0) + 2.5).abs() < 1e-5);
        assert!(resolve_all(MARKET_8, &o, &mesh(5)).is_empty());
        // Scroll of 2/53/55: 0.015 per tick (40 ms).
        let s = rules(ELBELAND, 2).unwrap().tex_coord[1];
        assert!((s.at(40.0) - 0.015).abs() < 1e-6);
        assert!(rules(1, 2).is_none());
    }
}
