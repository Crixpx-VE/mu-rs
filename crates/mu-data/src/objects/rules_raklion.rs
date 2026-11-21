//! Reglas de dibujo de los objetos de Raklion (`WD_57ICECITY` y `WD_58ICECITY_BOSS`, carpetas World58
//! y World59), Vulcanus (`WD_63PK_FIELD`, World64), Barracks y Refuge de Balgass
//! (`WD_41CHANGEUP3RD_1ST` / `WD_42CHANGEUP3RD_2ND`, World42/43), Karutan 1 y 2 (`WD_80KARUTAN1` /
//! `WD_81KARUTAN2`, World81/82) y Chaos Castle (`WD_18CHAOS_CASTLE..`, que cargan todos World19).
//!
//! Sacado de `GM_Raklion.cpp`, `GM_PK_Field.cpp`, `GM3rdChangeUp.cpp`, `GMKarutan1.cpp` y
//! `CSChaosCastle.cpp` (Main 5.2): `CreateObject`, `MoveObject`, `RenderObjectMesh` y
//! `RenderAfterObjectMesh`, y las velocidades de `MapManager.cpp` (1188-1207). Lo que crea partículas,
//! sprites, estelas, luces del suelo y sonidos (y la animación de los objetos con `fx_animation`)
//! está en `mu-client/src/fx/kinds/objects_raklion.rs`.
//!
//! Los objetos que estos mapas dibujan "después de los personajes" (`m_bRenderAfterCharacter`:
//! `RenderObjectMesh` no dibuja nada y `RenderAfterObjectMesh` hace las llamadas) se dibujan aquí con
//! esas llamadas; el orden respecto a los personajes no se traduce.
//!
//! El tipo 67 no se dibuja en ningún mapa salvo Chaos Castle: `MapProcess::CreateObject`
//! (`w_MapProcess.cpp:206-225`) llama al de todos los mapas registrados hasta que uno devuelve `true`
//! (`w_MapProcess.cpp:33-94`) y ninguno lo hace con el 67 hasta `GMUnitedMarketPlace::CreateObject`
//! (`GMUnitedMarketPlace.cpp:48-62`), que le pone `HiddenMesh = -2`. Pasa con el pez de lava 67 de
//! Vulcanus (cuyo `RenderObjectMesh`, `GM_PK_Field.cpp:485-535`, no llega a ejecutarse), la torre
//! central de Barracks y `icesca08` del mapa del jefe de Raklion. Chaos Castle no llega a esa cadena
//! (`CreateChaosCastleObject` devuelve `true` antes, `ZzzObject.cpp:4897`).

use super::render::{Bitmap, Pass, Saw, flags};
use super::{Glow, LightAdd, ModelRules};

/// Carpetas (`WorldActive + 1`).
const RAKLION: u16 = 58;
const RAKLION_BOSS: u16 = 59;
const VULCANUS: u16 = 64;
const BARRACKS: u16 = 42;
const REFUGE: u16 = 43;
const KARUTAN1: u16 = 81;
const KARUTAN2: u16 = 82;
/// `MapManager.cpp:1139-1142` / `:1250-1253`: los siete Chaos Castle y el de nivel maestro cargan
/// `Object19` y `World19`.
const CHAOS_CASTLE: u16 = 19;

/// Reglas de los mapas de este archivo (`None` = no es uno de ellos).
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
        // `RenderObjectMesh` (1104-1119): `IntensityTransform = 0.5` en todos los vértices y
        // `o->Light = (1, 1, 1)`, que desde el fotograma siguiente se suma a la luz del suelo
        // (`BodyLight`): color plano `(luz del suelo + 1) · 0.5`.
        6..=12 => ModelRules { light_add: Some(LightAdd::Fixed([1.0; 3])), intensity: Some(0.5), ..r() },
        // Pingüinos (1120-1164): cambian de acción al azar al llegar al fotograma 19. La velocidad es la
        // `PlaySpeed` de la acción (`ZzzObject.cpp:3819-3822`): 0.8 en las acciones 0 y 1 y 1 en la 2 y
        // la 3 (`MapManager.cpp:1188-1198`, solo en `WD_57ICECITY`). La lleva el código de efectos.
        16 | 17 => ModelRules { fx_animation: true, ..r().speed(0.8) },
        // Orcas (1165-1291): al acabar el salto se quedan en la acción 1, que no existe en el modelo
        // (la animación se para) hasta que `rand() % 40` las devuelve a la 0. Velocidad 0.16.
        19 | 21 => ModelRules { fx_animation: true, ..r() },
        // 20 (1231-1236): mallas 0 y 1, y la 1 otra vez con `RENDER_CHROME2 | RENDER_DARK` y con
        // `RENDER_CHROME | RENDER_BRIGHT`, las dos con `BITMAP_CHROME8`.
        20 => ModelRules { fx_animation: true, ..r().passes(RAKLION_20) },
        // `MoveObject` (266-271): `BlendMeshLight = sin(WorldTime·0.001) + 1` (la malla `_R`).
        22 => r().pulse(0.001, 1.0, 1.0),
        // 68, 69 y 71 (1364-1370): `RenderBody(RENDER_TEXTURE, Alpha, 0, (sin(WT·0.001) + 1)·0.5 + 0.3)`.
        // 68: `PlaySpeed` 0.05 (`MapManager.cpp:1197`).
        // TODO(original): en `WD_58ICECITY_BOSS` (el único con el tipo 68) `MapManager` no pone la
        // `PlaySpeed` y el original lee la de `Action_t` sin iniciar (`ZzzBMD.cpp:2527`); se usa 0.05.
        68 => ModelRules::add(0).pulse(0.001, 0.5, 0.8).speed(0.05),
        69 | 71 => ModelRules::add(0).pulse(0.001, 0.5, 0.8),
        // Puntos de fuego azul (70) y rojo (80): `MoveObject` (272-278) les pone `HiddenMesh = -2`.
        70 | 80 => ModelRules::hidden(),
        // Aurora (`RenderAfterObjectMesh` 2229-2237): `o->Light = (sin(WT·0.002), sin(WT·0.0015),
        // sin(WT·0.0014))·0.2 + 0.5`, que se suma a la luz del suelo en la malla `_R`.
        76 => ModelRules {
            light_add: Some(LightAdd::Wave { freq_ms: [0.002, 0.0015, 0.0014], amp: 0.2, base: 0.5 }),
            ..r()
        },
        // Ver el comentario del módulo (solo hay 67 en el mapa del jefe; `CGM_Raklion::CreateObject`
        // devuelve `false` con el 67 y la cadena sigue hasta `GMUnitedMarketPlace`).
        67 => ModelRules::hidden(),
        // 46, 53 (`RenderAfterObjectMesh`), 57 y 82 (con `m_bCanGoBossMap`, que empieza en `true`):
        // `RenderBody(RENDER_TEXTURE)` normal.
        // TODO(original): 82 se queda en el fotograma 0 mientras no se pueda ir al jefe (evento).
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
        // Puntos de humo, fuego y chispas (`MoveObject` 318-342). 67: ver el comentario del módulo.
        0..=6 | 67 => ModelRules::hidden(),
        // 15 (477-484): `StreamMesh = 0` y V = -(WT % 10000)·0.0001.
        15 => r().stream(0).tex_v(10000.0, -0.0001),
        // 16 (`song_lava2`): `RenderBody` normal (la malla `_R` suma con `BlendMeshLight` 1) y otra vez
        // después de los personajes con `BlendMesh = 0` y `(sin(WT·0.002) + 1)·0.5` (572-588).
        16 => r().passes(VULCANUS_16),
        // Pez de lava 68 (536-568): la malla con `StreamMesh = 1` y V = (WT % 10000)·0.0001, la malla 0
        // otra vez normal y, con `BodyLight = (1, 0, 0)`, cromada sumando con `Alpha` 0.2 (que en el
        // cromado con `RENDER_BRIGHT` multiplica `BodyLight`: `ZzzBMD.cpp:1122-1128`).
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

// --- Barracks y Refuge de Balgass (`GM3rdChangeUp.cpp`) ---------------------------------------------

fn balgass(kind: i16) -> ModelRules {
    let r = ModelRules::default;
    match kind {
        // `MoveObject` (77-117): puntos de humo, fuego, agua y chispas. 6: las nubes le ponen
        // `HiddenMesh = -2` (`RenderObjectVisual` 163-176). 67: ver el comentario del módulo.
        2 | 3 | 5 | 6 | 58 | 59 | 60 | 85..=93 | 67 => ModelRules::hidden(),
        // 57 (93-95): V = -(WT % 10000)·0.0002 (después de los personajes, `RenderBody` normal).
        57 => r().tex_v(10000.0, -0.0002),
        // 78 (`RenderAfterObjectMesh` 308-319): `BodyLight` 0.52, `StreamMesh = 0` y la malla 0 sumando
        // con U = -(WT % 100000)·0.00001. (`Alpha = 0.5` en `MoveObject`: con la suma no cuenta.)
        78 => r().passes(BALGASS_78),
        // 79 (`RenderObjectMesh` 267-271): `StreamMesh = 0` y V = -(WT % 10000)·0.0001.
        79 => r().stream(0).tex_v(10000.0, -0.0001),
        // 81 (272-277): `RenderBody(RENDER_TEXTURE, Alpha, 0, (sin(WT·0.002) + 1)·0.5 + 0.3)`.
        81 => ModelRules::add(0).pulse(0.002, 0.5, 0.8),
        // 82, 83 (278-285): malla 0 normal y la 1 con `BlendMesh = 1` y `sin(WT·0.0005)·10` (OpenGL lo
        // limita a [0, 1]: se enciende y se apaga).
        82 | 83 => r().passes(BALGASS_82_83),
        // 84 (`RenderAfterObjectMesh` 320-326): sube y baja (`MoveObject` 96-102, lo lleva el código de
        // efectos); `StreamMesh = 0` con `BlendMesh = 0`, `(sin(WT·0.001) + 1)·0.5` y U = (WT % 10000)·0.0001,
        // y luego `RenderBody` normal.
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
        // `MoveObject` (46-69): puntos de fuego (113, con luz), agua, niebla y humo. 67: ver el
        // comentario del módulo (no hay ninguno en World81/82).
        113..=116 | 118 | 67 => ModelRules::hidden(),
        // Después de los personajes (`RenderAfterObjectMesh` 291-351):
        // 1: mallas 1-3 normales y la 0 sumando.
        1 => r().passes(KARUTAN_1),
        // 54: la malla 0 con `BlendMesh = 0` y `(sin(WT·0.002) + 0.5)·0.5 + 1`.
        54 => r().passes(KARUTAN_54),
        // 55, 57: `StreamMesh = 0` y la malla 0 sumando con V = (WT % 10000)·0.0001.
        55 | 57 => r().passes(KARUTAN_55_57),
        // 58: 0 y 3 normales, 1 con `BlendMesh = 1` y `(sin(WT·0.001) + 1)·0.5`, 2 sumando.
        58 => r().passes(KARUTAN_58),
        // 63: 0 con `BlendMesh = 0` (`BlendMeshLight` 1), 1 y 3 normales, 2 con `BlendMesh = 2` y el pulso.
        63 => r().passes(KARUTAN_63),
        // 66: cambia de acción al azar (`RenderObjectMesh` 192-198, lo lleva el código de efectos) con
        // `PlaySpeed` 0.15 (`MapManager.cpp:1202-1203`); malla 0 normal y la 1 sumando.
        66 => ModelRules { fx_animation: true, ..r().speed(0.15).passes(KARUTAN_66) },
        // 107: `PlaySpeed` 5 (`MapManager.cpp:1205`, `ZzzObject.cpp:3824-3827`).
        107 => r().speed(5.0),
        // 119: 0 normal y 1 con `BlendMesh = 1` y el pulso.
        119 => r().passes(KARUTAN_119),
        // 3, 56, 62: `RenderBody` normal.
        _ => r(),
    }
}

/// `(sin(WT·0.001) + 1)·0.5` de 58, 63 y 119.
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
        // Nubes (6-12): `RenderChaosCastleVisual` (311-400) les pone `HiddenMesh = -2`.
        // 18-21 y 24-35: `CreateChaosCastleObject` (279-297) los oculta; aparecen cuando el castillo se
        // derrumba (`g_byCurrCastleLevel`, evento: no se traduce). En el estado inicial (255) siguen así.
        6..=12 | 18..=21 | 24..=35 => ModelRules::hidden(),
        // TODO(original): 0-5 y 13-17 se ocultan al empezar el derrumbe (`g_byCurrCastleLevel >= 2`) y
        // caen (`MoveChaosCastleAllObject` 154-271) con `BITMAP_SMOKE + 4` y temblor: evento.
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
        // Malla 1: normal, restando con Chrome08 y sumando con Chrome08 (las tres se ven).
        let looks = resolve_all(r.passes, &o, &mesh(1));
        let blends: Vec<_> = looks.iter().map(|(_, l)| l.blend).collect();
        assert_eq!(blends, vec![LookBlend::Opaque, LookBlend::Subtract, LookBlend::Add]);
        assert!(looks[1..].iter().all(|(_, l)| l.texture == Some(Bitmap::Chrome8)));
        // Malla 0: solo la normal.
        assert_eq!(resolve_all(r.passes, &o, &mesh(0)).len(), 1);
    }

    #[test]
    fn luces_de_balgass_y_karutan() {
        let o = ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 };
        // 82/83: la malla 1 suma con su propio `BlendMeshLight` (sin·10).
        let r = rules(REFUGE, 82).unwrap();
        let looks = resolve_all(r.passes, &o, &mesh(1));
        assert_eq!(looks.len(), 1);
        let (pass, l) = looks[0];
        assert_eq!((l.blend, l.color), (LookBlend::Add, LookColor::BlendLight));
        assert_eq!(r.passes[pass].glow, Some(Glow::Pulse { freq: 0.5, amp: 10.0, base: 0.0 }));
        // 78: la `StreamMesh` suma con `BodyLight` 0.52 plano y se desplaza en U.
        let r = rules(BARRACKS, 78).unwrap();
        let (_, l) = resolve_all(r.passes, &o, &mesh(0))[0];
        assert_eq!((l.blend, l.color, l.light), (LookBlend::Add, LookColor::Flat, Some([0.52; 3])));
        assert!(l.wave.is_some());
        // Karutan 63: dos mallas suman con brillos distintos (1 fijo y el pulso).
        let r = rules(KARUTAN1, 63).unwrap();
        let g0 = resolve_all(r.passes, &o, &mesh(0)).iter().map(|(p, _)| r.passes[*p].glow).collect::<Vec<_>>();
        let g2 = resolve_all(r.passes, &o, &mesh(2)).iter().map(|(p, _)| r.passes[*p].glow).collect::<Vec<_>>();
        assert_eq!(g0, vec![None]);
        assert_eq!(g2, vec![Some(KARUTAN_PULSE)]);
        // El tipo 67 no se dibuja (cadena de `CreateObject`) salvo en Chaos Castle.
        assert!(rules(VULCANUS, 67).unwrap().hidden && rules(BARRACKS, 67).unwrap().hidden);
        assert!(rules(RAKLION_BOSS, 67).unwrap().hidden && !rules(CHAOS_CASTLE, 67).unwrap().hidden);
    }
}
