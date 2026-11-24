//! Draw rules for the objects of Aida (`WD_33AIDA`, folder World34), Crywolf
//! (`WD_34CRYWOLF_1ST`, World35), Kanturu 1 (`WD_37KANTURU_1ST`, World38), Kanturu 2
//! (`WD_38KANTURU_2ND`, World39), Kanturu 3 (`WD_39KANTURU_3RD`, World40) and the GM map
//! (`WD_40AREA_FOR_GM`, World41).
//!
//! Taken from `GMAida.cpp`, `GMCrywolf1st.cpp`, `GM_kanturu_1st.cpp`, `GM_Kanturu_2nd.cpp` and
//! `GM_Kanturu_3rd.cpp` (Main 5.2): `MoveObject` (speed, `HiddenMesh`, `BlendMeshLight`,
//! UV scrolling), `RenderObjectMesh` (if it returns `true` there is no normal `RenderBody`),
//! `RenderObjectVisual` (passes added after the normal `RenderBody` of `RenderObject`,
//! `ZzzObject.cpp:2612-2615` and `3504-3505`) and `RenderAfterObjectMesh` (objects with
//! `m_bRenderAfterCharacter` are drawn only there, after the characters:
//! `ZzzObject.cpp:3536-3552`, `3672`). What creates particles, sprites, ground lights and sounds
//! is in `mu-client/src/fx/kinds/objects_kanturu.rs`.
//!
//! The GM map uses the Kanturu 1 code (`M40GMArea::IsGmArea` in `GM_kanturu_1st.cpp:23-26`
//! and in the conditions on lines 45, 132, 312 and 345).
//!
//! Types without a model in `Data` (Crywolf 79/80, Kanturu 2 21/22/25/28/39/40/42/43/69/84, GM
//! 15/33/52/74) are not drawn by the original either (`AccessModel` does not find the file): they are marked
//! hidden.
//!
//! TODO(original) — what the pass engine cannot do yet (nothing is invented: it is noted here):
//! - `o->Alpha` < 1 on lit meshes (Kanturu 1 96 with 0.5, `MoveKanturu1stObject` 105-107):
//!   drawn opaque / alpha-tested.
//! - `glAlphaFunc(GL_GREATER, 0)` of Kanturu 1 96 (`GM_kanturu_1st.cpp:231`): 0.25 here.
//! - `b->BodyLight` changing over time within a pass (Kanturu 3 21 and 43) and `BlendMeshLight`
//!   clamped to [0.1, 0.9] (Kanturu 2 10): see each type.
//! - UV scrolling with a constant term (`-0.1 + ...`, Kanturu 3 43) and map-specific
//!   `GL_LINEAR` + `GL_CLAMP_TO_EDGE` textures (`BITMAP_KANTURU3RD_OBJECT`).

use super::render::{Bitmap, Pass, Saw, flags};
use super::ModelRules;

/// Folders (`WorldActive + 1`).
const AIDA: u16 = 34;
const CRYWOLF: u16 = 35;
const KANTURU1: u16 = 38;
const KANTURU2: u16 = 39;
const KANTURU3: u16 = 40;
const GM_AREA: u16 = 41;

const T: u32 = flags::TEXTURE;
const TB: u32 = flags::TEXTURE | flags::BRIGHT;

/// `k · ((int)WorldTime % period)` (`k` < 0 = the original's `-(int)WorldTime % period · |k|`).
const fn saw(period: f32, k: f32) -> Saw {
    Saw { period, k }
}

/// Rules for the maps of this file (`None` = not one of them).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    let r = ModelRules::default;
    let add = ModelRules::add;
    let hidden = ModelRules::hidden;
    Some(match (world, kind) {
        // 67 on all these maps: the `default` branch of `CreateObject` (`ZzzObject.cpp:4895-4912`)
        // ends in `TheMapProcess().CreateObject` (`w_MapProcess.cpp:206`), and
        // `GMUnitedMarketPlace::CreateObject` (without checking the map) sets `HiddenMesh = -2` (and
        // `CreateOperate`). No earlier function in the chain takes 67 on these maps.
        (AIDA | CRYWOLF | KANTURU1 | KANTURU2 | KANTURU3 | GM_AREA, 67) => hidden(),

        // --- Aida (`GMAida.cpp`) ------------------------------------------------------------------
        // 25, 28 (`MoveAidaObject` 50-59): `BlendMeshTexCoordV -= 0.015` per tick = 0.000375 per ms
        // (8000 ms sawtooth = 3 whole wraps: the jump is not visible). The
        // `waterfall_R` mesh is its own `BlendMesh` (`RenderBody`), so it scrolls.
        (AIDA, 25 | 28) => r().tex_v(8000.0, -0.000375),
        // 41 (`RenderAidaObjectVisual` 154-168): after the normal `RenderBody`, mesh 0 again
        // additive as `StreamMesh` (flat color 0.52). The `Alpha = 0.5` of `MoveAidaObject` (74-78)
        // affects neither (the `_R` mesh is a `BlendMesh` and the `StreamMesh` uses
        // `glColor3fv`).
        (AIDA, 41) => r().passes(AIDA_41),
        // Water spots (56-58), fog (59, 62, 63), flame (67, already hidden above) and ghost (70)
        // (`MoveAidaObject` 79-88); 60 (butterflies) is hidden by `RenderAidaObjectVisual` (206).
        (AIDA, 56..=60 | 62 | 63 | 70) => hidden(),
        // 64 (`MoveAidaObject` 89-91): `Velocity = 0.05`.
        (AIDA, 64) => r().speed(0.05),
        // 65, 66 (226-242): mesh 0 again as `StreamMesh` (flat color, opaque) with
        // `U = -(int)WT % 100000 · 0.00005`: it covers the `RenderBody` one.
        (AIDA, 65 | 66) => r().passes(AIDA_65_66),
        // 77, 78 (312-321): same with `U = (int)WT % 100000 · 0.0002`.
        (AIDA, 77 | 78) => r().passes(AIDA_77_78),

        // --- Crywolf (`GMCrywolf1st.cpp`) -----------------------------------------------------------
        // 36 (`RenderCryWolf1stObjectMesh` 442-455): `BodyLight` = 0.4 in peace and in war.
        (CRYWOLF, 36) => r().body_light(0.4),
        // `RenderCryWolf1stObjectMesh` returns `true` without drawing for 57, 71-74, 77, 78 and 81 (474-562;
        // 81 is only drawn in war) and `MoveCryWolf1stObject` hides 82-84 (364-368).
        // TODO(original): 81 in war (`CRYWOLF_OCCUPATION_STATE_WAR`, 540-561): `Scale = 1.03`,
        // `BodyLight` from the statue's life and mesh 0 with `RENDER_BRIGHT | RENDER_CHROME`
        // (`StreamMesh` 0) and `RENDER_BRIGHT | RENDER_CHROME2` (`StreamMesh` 1). Here, in peace.
        (CRYWOLF, 57 | 71..=74 | 77 | 78 | 81..=84) => hidden(),
        // No model in `Data/Object35`.
        (CRYWOLF, 79 | 80) => hidden(),

        // --- Kanturu 1 and the GM map (`GM_kanturu_1st.cpp`) ---------------------------------------
        // No model in `Data/Object41`.
        (GM_AREA, 15 | 33 | 52 | 74) => hidden(),
        // `MoveKanturu1stObject` (53-61, 76, 83, 109) and `RenderKanturu1stObjectVisual` (148).
        (KANTURU1 | GM_AREA, 37 | 59..=62 | 81..=83 | 97 | 107 | 108) => hidden(),
        // Speeds (`MoveKanturu1stObject` 62-66, 85-86, 99-100).
        (KANTURU1 | GM_AREA, 44) => r().speed(0.02),
        // 46 (65-70): `BlendMeshLight = sin(WT·0.0015)·0.8 + 1` (the `_R` mesh of `RenderBody`).
        (KANTURU1 | GM_AREA, 46) => r().speed(0.01).pulse(0.0015, 0.8, 1.0),
        (KANTURU1 | GM_AREA, 70 | 90) => r().speed(0.04),
        // 76: `RenderKanturu1stObjectMesh` (329-336) defers it until after the characters and
        // `RenderKanturu1stAfterObjectMesh` (350-361) only draws mesh 0 additive as `StreamMesh`
        // (flat color 0.52) with `U = -(int)WT % 100000 · 0.00001`. `Alpha = 0.5` (92-94) does not apply.
        (KANTURU1 | GM_AREA, 76) => r().passes(KANTURU1_76),
        // 77 (95-98): `BlendMeshTexCoordV = -(int)WT % 10000 · 0.0002`; normal `RenderBody` after
        // the characters (394-400).
        (KANTURU1 | GM_AREA, 77) => r().tex_v(10000.0, -0.0002),
        // 85 (`RenderKanturu1stObjectVisual` 205-215): mesh 3 again additive and with subtractive
        // chrome, after the normal `RenderBody`.
        (KANTURU1 | GM_AREA, 85) => r().passes(KANTURU1_85),
        // 95 (363-383): only mesh 0, additive and with `RENDER_CHROME7 | RENDER_DARK` (which adds: the
        // branches of `ZzzBMD.cpp:1110-1121` come before the `RENDER_DARK` one).
        (KANTURU1 | GM_AREA, 95) => r().passes(KANTURU1_95),
        // 96 (229-238): mesh 0 again as `StreamMesh` with `V = -(int)WT % 20000 · 0.00005`,
        // with alpha 1 (see the TODO at the top: `Alpha = 0.5` and `glAlphaFunc(GL_GREATER, 0)`).
        (KANTURU1 | GM_AREA, 96) => r().passes(KANTURU1_96),
        // 102 (120-122): `BlendMeshLight = sin(WT·0.001) + 1`.
        (KANTURU1 | GM_AREA, 102) => r().pulse(0.001, 1.0, 1.0),
        // 103 (316-327): `RenderBody` without mesh 1 and mesh 1 as `BlendMesh` with
        // `fLumi = (sin(WT·0.002) + 1)·0.5`: the same as `BlendMesh = 1` with that brightness.
        (KANTURU1 | GM_AREA, 103) => add(1).pulse(0.002, 0.5, 0.5),
        // 105 (385-392): `RenderBody` and mesh 1 with additive chrome, after the characters.
        (KANTURU1 | GM_AREA, 105) => r().passes(KANTURU1_105),
        // 91, 92 (394-400): normal `RenderBody` after the characters.

        // --- Kanturu 2 (`GM_Kanturu_2nd.cpp`) -------------------------------------------------------
        // No model in `Data/Object39` (42 would have `U = V = -(int)WT % 10000 · 0.0002`, 245-250).
        (KANTURU2, 21 | 22 | 25 | 28 | 39 | 40 | 42 | 43 | 69 | 84) => hidden(),
        // 1, 2 (`Render_Kanturu2nd_ObjectMesh` 537-556): like Kanturu 1 103.
        (KANTURU2, 1 | 2) => add(1).pulse(0.002, 0.5, 0.5),
        // 8 (`Render_Kanturu2nd_AfterObjectMesh` 640-644): `RenderBody` and mesh 1 with additive chrome.
        (KANTURU2, 8) => r().passes(KANTURU2_8),
        // 10 (`Move_Kanturu2nd_Object` 225-239, `AfterObjectMesh` 645-650): `Velocity = 0.04`,
        // `RenderBody` without mesh 2 and mesh 2 with additive chrome.
        // TODO(original): `BlendMeshLight = sin(WT·0.0015) + 1` clamped to [0.1, 0.9]; the engine does not
        // clamp the pulse, so a sine of the same period between those two values is used.
        (KANTURU2, 10) => r().speed(0.04).pulse(0.0015, 0.4, 0.5).passes(KANTURU2_10),
        // 12 (557-568): `RenderBody` without mesh 0 (`StreamMesh` 1 with `V = -(int)WT % 2000 · 0.0005`),
        // mesh 0 as `BlendMesh` with `U = V = (int)WT % 2000 · 0.0005` and then chromed subtractively.
        (KANTURU2, 12) => r().passes(KANTURU2_12),
        // 13, 14 (569-586): `StreamMesh` 1 with `V = -(int)WT % 2000 · 0.0005`.
        (KANTURU2, 13 | 14) => r().passes(KANTURU2_13_14),
        // 15, 16 (587-596): same with `StreamMesh` 3.
        (KANTURU2, 15 | 16) => r().passes(KANTURU2_15_16),
        // 27 (597-607): `RenderBody` without mesh 2 (`StreamMesh` 4 with `V`) and mesh 2 additive.
        (KANTURU2, 27) => r().passes(KANTURU2_27),
        // 31 (`AfterObjectMesh` 634-639): `RenderBody` without mesh 0 and mesh 0 additive.
        (KANTURU2, 31) => r().passes(KANTURU2_31),
        // 35, 36 (651-656): `RenderBody` without mesh 1 and mesh 1 additive.
        (KANTURU2, 35 | 36) => r().passes(KANTURU2_35_36),
        // 38 (`Move_Kanturu2nd_Object` 240-244): `BlendMeshLight = sin(WT·0.001) + 1`.
        (KANTURU2, 38) => r().pulse(0.001, 1.0, 1.0),
        // 44 (251-255): `Velocity = 0.02`.
        (KANTURU2, 44) => r().speed(0.02),
        // Fog, water, lightning (256-272).
        (KANTURU2, 45..=56 | 65) => hidden(),
        // 76 (657-677): `BodyLight` 0.4, only mesh 0, additive and with `RENDER_CHROME7 | RENDER_DARK`.
        (KANTURU2, 76) => r().passes(KANTURU2_76),
        // 33, 59, 80 (678-681): normal `RenderBody` after the characters.

        // --- Kanturu 3 (`GM_Kanturu_3rd.cpp`) -------------------------------------------------------
        // TODO(original): 0 is Maya. `CreateKanturu3rdObject` (101-103) lowers her 2000 (from -749.5 to
        // -2749.5) and `RenderKanturu3rdObjectVisual` only raises her when she appears in the battle
        // (`GetMayaAppear`, 219-231) or keeps her at -749.5 in the battle states; with no battle
        // (`KANTURU_STATE_NONE`) she stays 2000 below her spot. The engine does not move objects, so she is not
        // drawn (nor her smoke from bone 34, 234-238, nor `MayaAction`).
        (KANTURU3, 0) => hidden(),
        // `CreateKanturu3rdObject` (104-114), `MoveKanturu3rdObject` (45, 48: 45 is hidden on the
        // first tick with `rand() % 3 == 0`) and `RenderKanturu3rdObjectVisual` (1, 11, 46, 49, 50).
        (KANTURU3, 1 | 11 | 32 | 45..=54 | 57 | 58 | 70) => hidden(),
        // 2 (`RenderKanturu3rdObjectMesh` 473-479): opaque chrome with `BITMAP_CHROME2` and then additive
        // with `BodyLight` 0.3.
        (KANTURU3, 2) => r().passes(KANTURU3_2),
        // 8, 10 (`RenderKanturu3rdAfterObjectMesh` 575-596): meshes 1-5 (1-7) and mesh 0 additive.
        (KANTURU3, 8) => r().passes(KANTURU3_8),
        (KANTURU3, 10) => r().passes(KANTURU3_10),
        // 15-17 (480-487): mesh 1 as `BlendMesh` with `V = -(int)WT % 10000 · 0.0005`.
        (KANTURU3, 15..=17) => r().passes(KANTURU3_15_17),
        // 19, 20 (597-614): `RenderBody` without mesh 7 (6) and that mesh (`hanmcrystal2_R`) as
        // `BlendMesh` with `V = -(int)WT % 10000 · 0.0001`: the same as the normal `RenderBody` (the
        // `_R` mesh is already its `BlendMesh`) with that scrolling.
        (KANTURU3, 19 | 20) => r().tex_v(10000.0, -0.0001),
        // 21 (615-622): only mesh 0, additive, with `BodyLight = sin(WT·0.002)·0.2 + 0.5`.
        // TODO(original): the animated light of a pass; the sine's center (0.5) is used.
        (KANTURU3, 21) => r().passes(KANTURU3_21),
        // 24, 25 (623-631): meshes 0, 2, 3 and mesh 1 additive.
        (KANTURU3, 24 | 25) => r().passes(KANTURU3_24_25),
        // 33 (488-493): mesh 1 as `BlendMesh` with `V = -(int)WT % 2000 · 0.0005`.
        (KANTURU3, 33) => r().passes(KANTURU3_33),
        // 40-42 (494-503): mesh 1 opaque chrome (`BodyLight` (0.6, 0.7, 1)) and mesh 0 as `BlendMesh`
        // with `BodyLight` (0.7, 0.6, 0.7) and `V = -(int)WT % 200000 · 0.00001`.
        (KANTURU3, 40..=42) => r().passes(KANTURU3_40_42),
        // 43 (504-518): see `KANTURU3_43`.
        (KANTURU3, 43) => r().passes(KANTURU3_43),
        // 73 (632-640): without a won battle (`KanturuSuccessMap`), mesh 0 twice additive with
        // `V` and then `U = (int)WT % 10000 · 0.0002`. TODO(original): with the tower conquered it is not
        // drawn (server state).
        (KANTURU3, 73) => r().passes(KANTURU3_73),
        // 74 (313-323): normal `RenderBody` (its `test12_H` mesh is not drawn).

        _ => return None,
    })
}

const AIDA_41: &[Pass] = &[Pass::body(T), Pass::mesh(0, TB).light([0.52; 3]).stream(0)];
const AIDA_65_66: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(saw(100000.0, -0.00005), Saw::ZERO)];
const AIDA_77_78: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(saw(100000.0, 0.0002), Saw::ZERO)];

const KANTURU1_76: &[Pass] = &[Pass::mesh(0, TB).light([0.52; 3]).stream(0).tex_coord(saw(100000.0, -0.00001), Saw::ZERO)];
const KANTURU1_85: &[Pass] = &[
    Pass::body(T),
    Pass::mesh(3, TB),
    Pass::mesh(3, flags::CHROME | flags::DARK).texture(Bitmap::Chrome),
];
const KANTURU1_95: &[Pass] = &[Pass::mesh(0, TB), Pass::mesh(0, flags::CHROME7 | flags::DARK).texture(Bitmap::Chrome)];
const KANTURU1_96: &[Pass] = &[Pass::body(T), Pass::mesh(0, T).stream(0).tex_coord(Saw::ZERO, saw(20000.0, -0.00005))];
const KANTURU1_105: &[Pass] = &[Pass::body(T), Pass::mesh(1, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome)];

const KANTURU2_8: &[Pass] = &[Pass::body(T), Pass::mesh(1, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome)];
/// `RenderBody(HiddenMesh 2)`: mesh 1 (`songk21crystal02_R`) is its own `BlendMesh`.
const KANTURU2_10: &[Pass] = &[
    Pass::mesh(0, T),
    Pass::mesh(1, T).blend_mesh(1),
    Pass::mesh(3, T),
    Pass::mesh(2, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome),
];
/// The `StreamMesh`'s `V = -(int)WT % 2000 · 0.0005`.
const K2_STREAM_V: Saw = saw(2000.0, -0.0005);
const KANTURU2_12: &[Pass] = &[
    Pass::mesh(1, T).stream(1).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(2, T).stream(1),
    Pass::mesh(3, T).stream(1),
    Pass::mesh(0, T).stream(-1).blend_mesh(0).tex_coord(saw(2000.0, 0.0005), saw(2000.0, 0.0005)),
    Pass::mesh(0, flags::CHROME | flags::DARK).stream(-1).texture(Bitmap::Chrome),
];
const KANTURU2_13_14: &[Pass] = &[Pass::body(T).stream(1).tex_coord(Saw::ZERO, K2_STREAM_V)];
const KANTURU2_15_16: &[Pass] = &[Pass::body(T).stream(3).tex_coord(Saw::ZERO, K2_STREAM_V)];
const KANTURU2_27: &[Pass] = &[
    Pass::mesh(0, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(1, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(3, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(4, T).stream(4).tex_coord(Saw::ZERO, K2_STREAM_V),
    Pass::mesh(2, TB).stream(-1),
];
const KANTURU2_31: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(0, TB),
];
const KANTURU2_35_36: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(2, T), Pass::mesh(1, TB)];
const KANTURU2_76: &[Pass] = &[
    Pass::mesh(0, TB).light([0.4; 3]),
    Pass::mesh(0, flags::CHROME7 | flags::DARK).texture(Bitmap::Chrome).light([0.4; 3]),
];

const KANTURU3_2: &[Pass] = &[Pass::mesh(0, flags::CHROME).texture(Bitmap::Chrome2), Pass::mesh(0, TB).light([0.3; 3])];
const KANTURU3_8: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(0, TB),
];
const KANTURU3_10: &[Pass] = &[
    Pass::mesh(1, T),
    Pass::mesh(2, T),
    Pass::mesh(3, T),
    Pass::mesh(4, T),
    Pass::mesh(5, T),
    Pass::mesh(6, T),
    Pass::mesh(7, T),
    Pass::mesh(0, TB),
];
const KANTURU3_15_17: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(1, T).blend_mesh(1).tex_coord(Saw::ZERO, saw(10000.0, -0.0005))];
const KANTURU3_21: &[Pass] = &[Pass::mesh(0, TB).light([0.5; 3])];
const KANTURU3_24_25: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(2, T), Pass::mesh(3, T), Pass::mesh(1, TB)];
const KANTURU3_33: &[Pass] = &[Pass::mesh(0, T), Pass::mesh(1, TB).blend_mesh(1).tex_coord(Saw::ZERO, saw(2000.0, -0.0005))];
const KANTURU3_40_42: &[Pass] = &[
    Pass::mesh(1, flags::CHROME).light([0.6, 0.7, 1.0]),
    Pass::mesh(0, TB).light([0.7, 0.6, 0.7]).blend_mesh(0).tex_coord(Saw::ZERO, saw(200000.0, -0.00001)),
];
/// Kanturu 3 43 (`RenderKanturu3rdObjectMesh` 504-518): mesh 2 normal; mesh 0 as `BlendMesh` with
/// `BodyLight` 1 and the `BITMAP_KANTURU3RD_OBJECT` texture (`Object40\Mtowereffe.JPG`,
/// `MapManager.cpp:472`) with `V = -0.1 + (int)WT % 5000 · 0.0001`; mesh 0 again normal and mesh 1 with
/// `BodyLight = sin(WT·0.0015)·0.2 + 0.4`.
/// TODO(original): the `-0.1` of the offset, that texture's `GL_CLAMP_TO_EDGE` (repeated here)
/// and the sine of mesh 1's light (the center, 0.4, is used).
const KANTURU3_43: &[Pass] = &[
    Pass::mesh(2, T),
    Pass::mesh(0, TB)
        .light([1.0; 3])
        .blend_mesh(0)
        .texture(Bitmap::File { path: "Object40/Mtowereffe.jpg", nearest_clamp: false })
        .tex_coord(Saw::ZERO, saw(5000.0, 0.0001)),
    Pass::mesh(0, T),
    Pass::mesh(1, T).light([0.4; 3]),
];
const KANTURU3_73: &[Pass] = &[
    Pass::mesh(0, TB).blend_mesh(0).tex_coord(Saw::ZERO, saw(10000.0, 0.0002)),
    Pass::mesh(0, TB).blend_mesh(0).tex_coord(saw(10000.0, 0.0002), Saw::ZERO),
];

#[cfg(test)]
mod tests {
    use super::super::render::{LookBlend, LookColor, MeshInfo, ObjectDraw, resolve_all};
    use super::*;

    fn draw(rules: &ModelRules) -> ObjectDraw {
        ObjectDraw {
            blend_mesh: rules.additive_mesh.map_or(-1, |m| m as i32),
            stream_mesh: rules.stream_mesh.map_or(-1, |m| m as i32),
            tex_coord: rules.tex_coord,
            hidden_mesh: -1,
        }
    }

    fn mesh(i: usize) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: false }
    }

    #[test]
    fn solo_estos_mapas() {
        assert!(rules(1, 25).is_none());
        assert!(rules(AIDA, 2).is_none());
        assert!(rules(AIDA, 59).unwrap().hidden);
        // The GM map uses the Kanturu 1 code.
        assert_eq!(rules(GM_AREA, 77), rules(KANTURU1, 77));
        assert!(rules(GM_AREA, 74).unwrap().hidden);
        assert!(rules(KANTURU1, 74).is_none());
    }

    #[test]
    fn cascada_de_aida_al_ritmo_del_original() {
        // -0.015 per tick (25 per second) = -0.375 per second.
        let v = rules(AIDA, 25).unwrap().tex_coord[1];
        assert!((v.at(1000.0) + 0.375).abs() < 1e-5);
        // When it wraps, it jumps a whole number of textures.
        assert!((v.k * v.period).fract().abs() < 1e-6);
    }

    #[test]
    fn la_pasada_de_stream_tapa_a_la_normal() {
        // Aida 65: mesh 0 is left only with the second pass (opaque, flat, scrolling).
        let rules = rules(AIDA, 65).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        assert_eq!(looks.len(), 1);
        assert_eq!(looks[0].0, 1);
        assert_eq!((looks[0].1.blend, looks[0].1.color), (LookBlend::Opaque, LookColor::Flat));
        assert!(looks[0].1.wave.is_some());
    }

    #[test]
    fn cromado7_resta_que_suma() {
        // `RENDER_CHROME7 | RENDER_DARK`: the CHROME7 branch (additive) comes before the DARK one.
        let rules = rules(KANTURU1, 95).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        assert_eq!(looks.iter().map(|(_, l)| l.blend).collect::<Vec<_>>(), vec![LookBlend::Add, LookBlend::Add]);
        // Kanturu 1 85: the chrome of mesh 3 with `RENDER_DARK` does subtract.
        let rules = super::rules(KANTURU1, 85).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(3));
        assert_eq!(looks.last().unwrap().1.blend, LookBlend::Subtract);
    }

    #[test]
    fn kanturu2_12_sin_la_malla_0_normal() {
        let rules = rules(KANTURU2, 12).unwrap();
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(0));
        // Only the `BlendMesh` (additive) and the chrome (subtractive).
        assert_eq!(looks.iter().map(|(_, l)| l.blend).collect::<Vec<_>>(), vec![LookBlend::Add, LookBlend::Subtract]);
        assert_eq!(looks[0].1.color, LookColor::BlendLight);
        // Mesh 1 is the `StreamMesh`: flat and scrolling.
        let looks = resolve_all(rules.passes, &draw(&rules), &mesh(1));
        assert_eq!(looks.len(), 1);
        assert_eq!(looks[0].1.color, LookColor::Flat);
        assert!(looks[0].1.wave.is_some());
    }
}
