//! Draw rules for the objects of Raklion (`WD_57ICECITY` and `WD_58ICECITY_BOSS`, folders World58
//! and World59), Vulcanus (`WD_63PK_FIELD`, World64), Balgass Barracks and Refuge
//! (`WD_41CHANGEUP3RD_1ST` / `WD_42CHANGEUP3RD_2ND`, World42/43), Karutan 1 and 2 (`WD_80KARUTAN1` /
//! `WD_81KARUTAN2`, World81/82) and Chaos Castle (`WD_18CHAOS_CASTLE..`, which all load World19).
//!
//! Taken from `GM_Raklion.cpp`, `GM_PK_Field.cpp`, `GM3rdChangeUp.cpp`, `GMKarutan1.cpp` and
//! `CSChaosCastle.cpp` (Main 5.2): `CreateObject`, `MoveObject`, `RenderObjectMesh` and
//! `RenderAfterObjectMesh`, plus the speeds from `MapManager.cpp` (1188-1207). What creates particles,
//! sprites, trails, ground lights and sounds (and the animation of objects with `fx_animation`)
//! is in `mu-client/src/fx/kinds/objects_raklion.rs`.
//!
//! Objects these maps draw "after the characters" (`m_bRenderAfterCharacter`:
//! `RenderObjectMesh` draws nothing and `RenderAfterObjectMesh` makes the calls) are drawn here with
//! those calls; the ordering relative to characters is not translated.
//!
//! Type 67 is not drawn on any map except Chaos Castle: `MapProcess::CreateObject`
//! (`w_MapProcess.cpp:206-225`) calls the one of every registered map until one returns `true`
//! (`w_MapProcess.cpp:33-94`) and none does so for 67 until `GMUnitedMarketPlace::CreateObject`
//! (`GMUnitedMarketPlace.cpp:48-62`), which sets `HiddenMesh = -2`. This affects the Vulcanus lava
//! fish 67 (whose `RenderObjectMesh`, `GM_PK_Field.cpp:485-535`, never runs), the Barracks central
//! tower and `icesca08` on the Raklion boss map. Chaos Castle never reaches that chain
//! (`CreateChaosCastleObject` returns `true` first, `ZzzObject.cpp:4897`).

use super::render::{Bitmap, Pass, Saw, flags};
use super::{Glow, LightAdd, ModelRules};

/// Folders (`WorldActive + 1`).
const RAKLION: u16 = 58;
const RAKLION_BOSS: u16 = 59;
const VULCANUS: u16 = 64;
const BARRACKS: u16 = 42;
const REFUGE: u16 = 43;
const KARUTAN1: u16 = 81;
const KARUTAN2: u16 = 82;
/// `MapManager.cpp:1139-1142` / `:1250-1253`: the seven Chaos Castles and the master-level one load
/// `Object19` and `World19`.
const CHAOS_CASTLE: u16 = 19;

/// Rules for the maps in this file (`None` = not one of them).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    match world {
        RAKLION | RAKLION_BOSS => Some(raklion(kind)),
        VULCANUS => Some(vulcanus(kind)),
        BARRACKS | REFUGE => Some(balgass(kind)),
        KARUTAN1 | KARUTAN2 => Some(karutan(kind)),
        CHAOS_CASTLE => Some(chaos_castle(kind)),
        _ => None,
    }
}

// --- Raklion (`GM_Raklion.cpp`) ---------------------------------------------------------------------

fn raklion(kind: i16) -> ModelRules {
    let r = ModelRules::default;
    match kind {
        // `RenderObjectMesh` (1104-1119): `IntensityTransform = 0.5` on all vertices and
        // `o->Light = (1, 1, 1)`, which from the next frame on is added to the ground light
        // (`BodyLight`): flat color `(ground light + 1) · 0.5`.
        6..=12 => ModelRules { light_add: Some(LightAdd::Fixed([1.0; 3])), intensity: Some(0.5), ..r() },
        // Penguins (1120-1164): switch action at random on reaching frame 19. The speed is the
        // action's `PlaySpeed` (`ZzzObject.cpp:3819-3822`): 0.8 for actions 0 and 1 and 1 for 2 and
        // 3 (`MapManager.cpp:1188-1198`, only in `WD_57ICECITY`). Handled by the effects code.
        16 | 17 => ModelRules { fx_animation: true, ..r().speed(0.8) },
        // Orcas (1165-1291): after the jump they stay in action 1, which doesn't exist in the model
        // (the animation stops) until `rand() % 40` returns them to 0. Speed 0.16.
        19 | 21 => ModelRules { fx_animation: true, ..r() },
        // 20 (1231-1236): meshes 0 and 1, and mesh 1 again with `RENDER_CHROME2 | RENDER_DARK` and with
        // `RENDER_CHROME | RENDER_BRIGHT`, both with `BITMAP_CHROME8`.
        20 => ModelRules { fx_animation: true, ..r().passes(RAKLION_20) },
        // `MoveObject` (266-271): `BlendMeshLight = sin(WorldTime·0.001) + 1` (the `_R` mesh).
        22 => r().pulse(0.001, 1.0, 1.0),
        // 68, 69 and 71 (1364-1370): `RenderBody(RENDER_TEXTURE, Alpha, 0, (sin(WT·0.001) + 1)·0.5 + 0.3)`.
        // 68: `PlaySpeed` 0.05 (`MapManager.cpp:1197`).
        // TODO(original): in `WD_58ICECITY_BOSS` (the only one with type 68) `MapManager` doesn't set the
        // `PlaySpeed` and the original reads the uninitialized one from `Action_t` (`ZzzBMD.cpp:2527`); 0.05 is used.
        68 => ModelRules::add(0).pulse(0.001, 0.5, 0.8).speed(0.05),
        69 | 71 => ModelRules::add(0).pulse(0.001, 0.5, 0.8),
        // Blue (70) and red (80) fire points: `MoveObject` (272-278) sets `HiddenMesh = -2` on them.
        70 | 80 => ModelRules::hidden(),
        // Aurora (`RenderAfterObjectMesh` 2229-2237): `o->Light = (sin(WT·0.002), sin(WT·0.0015),
        // sin(WT·0.0014))·0.2 + 0.5`, added to the ground light on the `_R` mesh.
        76 => ModelRules {
            light_add: Some(LightAdd::Wave { freq_ms: [0.002, 0.0015, 0.0014], amp: 0.2, base: 0.5 }),
            ..r()
        },
        // See the module comment (67 only exists on the boss map; `CGM_Raklion::CreateObject`
        // returns `false` for 67 and the chain continues up to `GMUnitedMarketPlace`).
        67 => ModelRules::hidden(),
        // 46, 53 (`RenderAfterObjectMesh`), 57 and 82 (with `m_bCanGoBossMap`, which starts as `true`):
        // normal `RenderBody(RENDER_TEXTURE)`.
        // TODO(original): 82 stays on frame 0 while the boss can't be entered (event).
        _ => r(),
    }
}

const RAKLION_20: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE),
    Pass::mesh(1, flags::CHROME2 | flags::DARK).texture(Bitmap::Chrome8),
    Pass::mesh(1, flags::CHROME | flags::BRIGHT).texture(Bitmap::Chrome8),
];

// --- Vulcanus (`GM_PK_Field.cpp`) ------------------------------------------------------------------

fn vulcanus(kind: i16) -> ModelRules {
    let r = ModelRules::default;
    match kind {
        // Smoke, fire and spark points (`MoveObject` 318-342). 67: see the module comment.
        0..=6 | 67 => ModelRules::hidden(),
        // 15 (477-484): `StreamMesh = 0` and V = -(WT % 10000)·0.0001.
        15 => r().stream(0).tex_v(10000.0, -0.0001),
        // 16 (`song_lava2`): normal `RenderBody` (the `_R` mesh is additive with `BlendMeshLight` 1) and again
        // after the characters with `BlendMesh = 0` and `(sin(WT·0.002) + 1)·0.5` (572-588).
        16 => r().passes(VULCANUS_16),
        // Lava fish 68 (536-568): the mesh with `StreamMesh = 1` and V = (WT % 10000)·0.0001, mesh 0
        // again normal and, with `BodyLight = (1, 0, 0)`, chrome additive with `Alpha` 0.2 (which in
        // chrome with `RENDER_BRIGHT` multiplies `BodyLight`: `ZzzBMD.cpp:1122-1128`).
        68 => r().passes(VULCANUS_68),
        _ => r(),
    }
}

const VULCANUS_16: &[Pass] = &[
    Pass::body(flags::TEXTURE),
    Pass::body(flags::TEXTURE).blend_mesh(0).pulse(0.002, 0.5, 0.5),
];

const VULCANUS_68: &[Pass] = &[
    Pass::body(flags::TEXTURE).stream(1).tex_coord(Saw::ZERO, Saw { period: 10000.0, k: 0.0001 }),
    Pass::mesh(0, flags::TEXTURE).stream(-1),
    Pass::mesh(0, flags::CHROME | flags::BRIGHT).light([0.2, 0.0, 0.0]).stream(-1).blend_mesh(0).glow(Glow::Steady(0.2)),
];

// --- Balgass Barracks and Refuge (`GM3rdChangeUp.cpp`) ----------------------------------------------

fn balgass(kind: i16) -> ModelRules {
    let r = ModelRules::default;
    match kind {
        // `MoveObject` (77-117): smoke, fire, water and spark points. 6: the clouds set
        // `HiddenMesh = -2` on it (`RenderObjectVisual` 163-176). 67: see the module comment.
        2 | 3 | 5 | 6 | 58 | 59 | 60 | 85..=93 | 67 => ModelRules::hidden(),
        // 57 (93-95): V = -(WT % 10000)·0.0002 (after the characters, normal `RenderBody`).
        57 => r().tex_v(10000.0, -0.0002),
        // 78 (`RenderAfterObjectMesh` 308-319): `BodyLight` 0.52, `StreamMesh = 0` and mesh 0 additive
        // with U = -(WT % 100000)·0.00001. (`Alpha = 0.5` in `MoveObject`: irrelevant when additive.)
        78 => r().passes(BALGASS_78),
        // 79 (`RenderObjectMesh` 267-271): `StreamMesh = 0` and V = -(WT % 10000)·0.0001.
        79 => r().stream(0).tex_v(10000.0, -0.0001),
        // 81 (272-277): `RenderBody(RENDER_TEXTURE, Alpha, 0, (sin(WT·0.002) + 1)·0.5 + 0.3)`.
        81 => ModelRules::add(0).pulse(0.002, 0.5, 0.8),
        // 82, 83 (278-285): mesh 0 normal and mesh 1 with `BlendMesh = 1` and `sin(WT·0.0005)·10` (OpenGL
        // clamps it to [0, 1]: it turns on and off).
        82 | 83 => r().passes(BALGASS_82_83),
        // 84 (`RenderAfterObjectMesh` 320-326): moves up and down (`MoveObject` 96-102, handled by the effects
        // code); `StreamMesh = 0` with `BlendMesh = 0`, `(sin(WT·0.001) + 1)·0.5` and U = (WT % 10000)·0.0001,
        // then normal `RenderBody`.
        84 => ModelRules { fx_animation: true, ..r().passes(BALGASS_84) },
        _ => r(),
    }
}

const BALGASS_78: &[Pass] = &[Pass::mesh(0, flags::TEXTURE | flags::BRIGHT)
    .light([0.52; 3])
    .stream(0)
    .tex_coord(Saw { period: 100000.0, k: -0.00001 }, Saw::ZERO)];

const BALGASS_82_83: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE).blend_mesh(1).pulse(0.0005, 10.0, 0.0),
];

const BALGASS_84: &[Pass] = &[
    Pass::body(flags::TEXTURE)
        .stream(0)
        .blend_mesh(0)
        .pulse(0.001, 0.5, 0.5)
        .tex_coord(Saw { period: 10000.0, k: 0.0001 }, Saw::ZERO),
    Pass::body(flags::TEXTURE).stream(-1),
];

// --- Karutan (`GMKarutan1.cpp`) ---------------------------------------------------------------------

fn karutan(kind: i16) -> ModelRules {
    let r = ModelRules::default;
    match kind {
        // `MoveObject` (46-69): fire points (113, with light), water, mist and smoke. 67: see the
        // module comment (there are none in World81/82).
        113..=116 | 118 | 67 => ModelRules::hidden(),
        // After the characters (`RenderAfterObjectMesh` 291-351):
        // 1: meshes 1-3 normal and mesh 0 additive.
        1 => r().passes(KARUTAN_1),
        // 54: mesh 0 with `BlendMesh = 0` and `(sin(WT·0.002) + 0.5)·0.5 + 1`.
        54 => r().passes(KARUTAN_54),
        // 55, 57: `StreamMesh = 0` and mesh 0 additive with V = (WT % 10000)·0.0001.
        55 | 57 => r().passes(KARUTAN_55_57),
        // 58: 0 and 3 normal, 1 with `BlendMesh = 1` and `(sin(WT·0.001) + 1)·0.5`, 2 additive.
        58 => r().passes(KARUTAN_58),
        // 63: 0 with `BlendMesh = 0` (`BlendMeshLight` 1), 1 and 3 normal, 2 with `BlendMesh = 2` and the pulse.
        63 => r().passes(KARUTAN_63),
        // 66: switches action at random (`RenderObjectMesh` 192-198, handled by the effects code) with
        // `PlaySpeed` 0.15 (`MapManager.cpp:1202-1203`); mesh 0 normal and mesh 1 additive.
        66 => ModelRules { fx_animation: true, ..r().speed(0.15).passes(KARUTAN_66) },
        // 107: `PlaySpeed` 5 (`MapManager.cpp:1205`, `ZzzObject.cpp:3824-3827`).
        107 => r().speed(5.0),
        // 119: 0 normal and 1 with `BlendMesh = 1` and the pulse.
        119 => r().passes(KARUTAN_119),
        // 3, 56, 62: normal `RenderBody`.
        _ => r(),
    }
}

/// `(sin(WT·0.001) + 1)·0.5` of 58, 63 and 119.
const KARUTAN_PULSE: Glow = Glow::Pulse { freq: 0.001 * 1000.0, amp: 0.5, base: 0.5 };

const KARUTAN_1: &[Pass] = &[
    Pass::mesh(1, flags::TEXTURE),
    Pass::mesh(2, flags::TEXTURE),
    Pass::mesh(3, flags::TEXTURE),
    Pass::mesh(0, flags::TEXTURE | flags::BRIGHT),
];

const KARUTAN_54: &[Pass] = &[Pass::mesh(0, flags::TEXTURE | flags::BRIGHT).blend_mesh(0).pulse(0.002, 0.5, 1.25)];

const KARUTAN_55_57: &[Pass] =
    &[Pass::mesh(0, flags::TEXTURE | flags::BRIGHT).stream(0).tex_coord(Saw::ZERO, Saw { period: 10000.0, k: 0.0001 })];

const KARUTAN_58: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE),
    Pass::mesh(1, flags::TEXTURE | flags::BRIGHT).blend_mesh(1).glow(KARUTAN_PULSE),
    Pass::mesh(3, flags::TEXTURE),
    Pass::mesh(2, flags::TEXTURE | flags::BRIGHT),
];

const KARUTAN_63: &[Pass] = &[
    Pass::mesh(0, flags::TEXTURE | flags::BRIGHT).blend_mesh(0),
    Pass::mesh(1, flags::TEXTURE),
    Pass::mesh(2, flags::TEXTURE | flags::BRIGHT).blend_mesh(2).glow(KARUTAN_PULSE),
    Pass::mesh(3, flags::TEXTURE),
];

const KARUTAN_66: &[Pass] = &[Pass::mesh(0, flags::TEXTURE), Pass::mesh(1, flags::TEXTURE | flags::BRIGHT)];

const KARUTAN_119: &[Pass] =
    &[Pass::mesh(0, flags::TEXTURE), Pass::mesh(1, flags::TEXTURE | flags::BRIGHT).blend_mesh(1).glow(KARUTAN_PULSE)];

// --- Chaos Castle (`CSChaosCastle.cpp`) -------------------------------------------------------------

fn chaos_castle(kind: i16) -> ModelRules {
    match kind {
        // Clouds (6-12): `RenderChaosCastleVisual` (311-400) sets `HiddenMesh = -2` on them.
        // 18-21 and 24-35: `CreateChaosCastleObject` (279-297) hides them; they appear when the castle
        // collapses (`g_byCurrCastleLevel`, event: not translated). In the initial state (255) they stay so.
        6..=12 | 18..=21 | 24..=35 => ModelRules::hidden(),
        // TODO(original): 0-5 and 13-17 are hidden when the collapse begins (`g_byCurrCastleLevel >= 2`) and
        // fall (`MoveChaosCastleAllObject` 154-271) with `BITMAP_SMOKE + 4` and shaking: event.
        _ => ModelRules::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::render::{LookBlend, LookColor, MeshInfo, ObjectDraw, resolve_all};

    fn mesh(i: usize) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: false }
    }

    #[test]
    fn orca_cromada() {
        let r = rules(RAKLION, 20).unwrap();
        assert!(r.fx_animation);
        let o = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        // Mesh 1: normal, subtractive with Chrome08 and additive with Chrome08 (all three visible).
        let looks = resolve_all(r.passes, &o, &mesh(1));
        let blends: Vec<_> = looks.iter().map(|(_, l)| l.blend).collect();
        assert_eq!(blends, vec![LookBlend::Opaque, LookBlend::Subtract, LookBlend::Add]);
        assert!(looks[1..].iter().all(|(_, l)| l.texture == Some(Bitmap::Chrome8)));
        // Mesh 0: only the normal one.
        assert_eq!(resolve_all(r.passes, &o, &mesh(0)).len(), 1);
    }

    #[test]
    fn luces_de_balgass_y_karutan() {
        let o = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        // 82/83: mesh 1 is additive with its own `BlendMeshLight` (sin·10).
        let r = rules(REFUGE, 82).unwrap();
        let looks = resolve_all(r.passes, &o, &mesh(1));
        assert_eq!(looks.len(), 1);
        let (pass, l) = looks[0];
        assert_eq!((l.blend, l.color), (LookBlend::Add, LookColor::BlendLight));
        assert_eq!(r.passes[pass].glow, Some(Glow::Pulse { freq: 0.5, amp: 10.0, base: 0.0 }));
        // 78: the `StreamMesh` is additive with flat `BodyLight` 0.52 and scrolls in U.
        let r = rules(BARRACKS, 78).unwrap();
        let (_, l) = resolve_all(r.passes, &o, &mesh(0))[0];
        assert_eq!((l.blend, l.color, l.light), (LookBlend::Add, LookColor::Flat, Some([0.52; 3])));
        assert!(l.wave.is_some());
        // Karutan 63: two meshes are additive with different brightness (1 fixed and the pulse).
        let r = rules(KARUTAN1, 63).unwrap();
        let g0 = resolve_all(r.passes, &o, &mesh(0)).iter().map(|(p, _)| r.passes[*p].glow).collect::<Vec<_>>();
        let g2 = resolve_all(r.passes, &o, &mesh(2)).iter().map(|(p, _)| r.passes[*p].glow).collect::<Vec<_>>();
        assert_eq!(g0, vec![None]);
        assert_eq!(g2, vec![Some(KARUTAN_PULSE)]);
        // Type 67 is not drawn (`CreateObject` chain) except in Chaos Castle.
        assert!(rules(VULCANUS, 67).unwrap().hidden && rules(BARRACKS, 67).unwrap().hidden);
        assert!(rules(RAKLION_BOSS, 67).unwrap().hidden && !rules(CHAOS_CASTLE, 67).unwrap().hidden);
    }
}
