//! Reglas de dibujo de los objetos de Kalima 1-7 (`InHellas`: `WD_24HELLAS..WD_24HELLAS_END` y
//! `WD_24HELLAS_7`, carpetas World25-30 y World37; todas usan los datos de `World25`/`Object25`) y de
//! Land of Trials (`WD_31HUNTING_GROUND`, World32).
//!
//! - Kalima: `MoveHellasVisual` (`GMHellas.cpp:386-400`) y `RenderHellasObjectMesh` (:513-631). Casi
//!   todos los objetos llevan encima una segunda pasada con las cáusticas del agua
//!   (`BITMAP_WATER + WaterTextureNumber`) a `Alpha` 0.3 y `BodyLight = sin(WT·0.002)·0.1 + 0.3`; el
//!   34, además, su malla 1 cromada sumando (con esa misma `BodyLight`, que nadie restaura).
//! - Land of Trials: `MoveHuntingGroundObject` (`GMHuntingGround.cpp:44-113`) y
//!   `RenderHuntingGroundObjectMesh` (:154-190): luces que laten (27, 52, 54) y una fija (10).
//!
//! Lo que crea partículas, sprites, luces del suelo y sonidos está en
//! `mu-client/src/fx/kinds/objects_kalima.rs`; el mar de Kalima, en `mu-client/src/water_terrain.rs`.

use super::render::{Bitmap, Pass, flags};
use super::{Motion, ModelRules};

/// Carpetas (`WorldActive + 1`) de Kalima 1-6 y de la Kalima oculta.
const KALIMA_1: u16 = 25;
const KALIMA_6: u16 = 30;
const KALIMA_7: u16 = 37;
const HUNTING_GROUND: u16 = 32;

fn kalima(world: u16) -> bool {
    (KALIMA_1..=KALIMA_6).contains(&world) || world == KALIMA_7
}

/// Reglas de los mapas de este archivo (`None` = no es uno de ellos).
pub fn rules(world: u16, kind: i16) -> Option<ModelRules> {
    let r = ModelRules::default;
    let hidden = ModelRules::hidden;
    if kalima(world) {
        return Some(match kind {
            // 35 y 36 (`RenderHellasVisual` :467-478) y las cascadas 37-40 (`MoveHellasVisual`):
            // `HiddenMesh = -2`, solo sueltan partículas.
            35..=40 => hidden(),
            // `CheckGrass` + `Position[2] = GetWaterTerrain(x, y) + 180` (:453-465).
            // TODO(original): después dibujan otra vez todo con `RENDER_TEXTURE | RENDER_SHADOWMAP` y
            // `BodyLight` 0.1 (:615-621): una copia oscura aplastada a 5 de su origen (`ZzzBMD.cpp:
            // 1319-1330`) que se mueve con el agua; las sombras de `movers.rs` son fijas y negras.
            15 | 29 | 32 => r().motion(Motion::KalimaFloat),
            34 => r().passes(KALIMA_34),
            k if overlay(k) => r().passes(KALIMA_WATER),
            _ => r(),
        });
    }
    if world == HUNTING_GROUND {
        return Some(match kind {
            // Mariposas (1, 44, 45), cascada (3), humo (53) y luz del suelo (42): `HiddenMesh = -2`.
            1 | 3 | 42 | 44 | 45 | 53 => hidden(),
            // 27 y 54: `BodyLight *= sin(Timer + WT·0.0012)·0.5 + 0.9` con `Timer = rand()%1000 · 0.01`
            // (`CreateHuntingGroundObject` :36-39). El 27 además sube y baja (`MoveHuntingGroundObject`).
            27 => r().passes(HUNTING_27_54).motion(Motion::HuntingGroundVibrate),
            54 => r().passes(HUNTING_27_54),
            // 52: `BodyLight *= sin(Timer + WT·0.0009)·0.5 + 0.9` con `Timer = 0`. No devuelve `true`:
            // la cadena acaba en el `RenderBody` normal, con la misma `BodyLight` (la misma malla otra
            // vez encima, no cambia nada).
            52 => r().passes(HUNTING_52),
            // 10: `BodyLight = (0.56, 0.80, 0.81)`.
            10 => r().passes(HUNTING_10),
            _ => r(),
        });
    }
    None
}

/// `o->Timer` que deja `CreateObject` en el objeto número `index` del mapa: 0 salvo Land of Trials
/// 27 y 54, `float(rand()%1000) · 0.01` (`CreateHuntingGroundObject`, GMHuntingGround.cpp:36-39).
/// La secuencia de `rand()` del original no se puede repetir: cada objeto saca la suya del `rand()`
/// de MSVC empezando en una semilla según su número.
pub fn create_timer(world: u16, kind: i16, index: usize) -> f32 {
    if world != HUNTING_GROUND || !matches!(kind, 27 | 54) {
        return 0.0;
    }
    let mut seed = 0x1234_5678u32 ^ (index as u32).wrapping_mul(2_654_435_761);
    seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
    let rand = ((seed >> 16) & 0x7FFF) as i32;
    (rand % 1000) as f32 * 0.01
}

/// Tipos 0-66 de Kalima con la segunda pasada de agua (los de la lista de `:597-598` no la llevan).
fn overlay(kind: i16) -> bool {
    (0..=66).contains(&kind)
        && !matches!(kind, 2 | 4 | 12 | 14 | 15 | 18 | 20 | 21 | 27 | 29 | 30 | 31 | 32 | 41 | 43 | 52 | 54 | 55)
}

/// `RenderBody(RENDER_TEXTURE, 0.3f, ..., BITMAP_WATER + WaterTextureNumber)` con
/// `BodyLight = sin(WorldTime·0.002)·0.1 + 0.3` (:600-602).
const WATER_PASS: Pass = Pass::body(flags::TEXTURE).texture(Bitmap::Water).alpha(0.3).body_pulse(0.002, 0.1, 0.3);
const KALIMA_WATER: &[Pass] = &[Pass::body(flags::TEXTURE), WATER_PASS];
/// 34 (:605-613): `RenderMesh(1, RENDER_CHROME | RENDER_BRIGHT, ..., MeshTexture = o->HiddenMesh = -1)`:
/// `BITMAP_CHROME`, con la `BodyLight` que dejó la pasada del agua.
const KALIMA_34: &[Pass] = &[
    Pass::body(flags::TEXTURE),
    WATER_PASS,
    Pass::mesh(1, flags::CHROME | flags::BRIGHT).body_pulse(0.002, 0.1, 0.3),
];
const HUNTING_27_54: &[Pass] = &[Pass::body(flags::TEXTURE).body_scale_pulse(0.0012, 0.5, 0.9, true)];
const HUNTING_52: &[Pass] = &[Pass::body(flags::TEXTURE).body_scale_pulse(0.0009, 0.5, 0.9, false)];
const HUNTING_10: &[Pass] = &[Pass::body(flags::TEXTURE).light([0.56, 0.80, 0.81])];

#[cfg(test)]
mod tests {
    use super::super::render::{LookBlend, LookColor, MeshInfo, ObjectDraw, Saw, resolve_all};
    use super::*;

    fn obj() -> ObjectDraw {
        ObjectDraw { blend_mesh: -1, stream_mesh: -1, tex_coord: [Saw::ZERO; 2], hidden_mesh: -1 }
    }
    fn mesh(i: usize, alpha: bool) -> MeshInfo {
        MeshInfo { index: i, texture_index: i as i32, script: None, has_alpha: alpha }
    }

    #[test]
    fn kalima_en_todas_sus_carpetas() {
        for world in [25, 26, 27, 28, 29, 30, 37] {
            assert!(rules(world, 37).unwrap().hidden, "World{world}");
            assert_eq!(rules(world, 0).unwrap().passes, KALIMA_WATER);
        }
        assert!(rules(31, 0).is_none() && rules(36, 0).is_none() && rules(24, 0).is_none());
        // Los de la lista no llevan agua.
        assert_eq!(rules(25, 2).unwrap().passes, ModelRules::default().passes);
        assert_eq!(rules(25, 15).unwrap().motion, Some(Motion::KalimaFloat));
    }

    #[test]
    fn pasada_del_agua() {
        // La malla normal opaca y encima el agua a 0.3, con prueba de alfa aunque la textura sea JPG.
        let l = resolve_all(KALIMA_WATER, &obj(), &mesh(0, false));
        assert_eq!(l.len(), 2);
        let w = l[1].1;
        assert_eq!((w.texture, w.blend, w.color, w.alpha), (Some(Bitmap::Water), LookBlend::AlphaTest, LookColor::Lit, 0.3));
        let wave = w.body_wave.unwrap();
        // sin(0)·0.1 + 0.3 = 0.3; a los π/0.004 ms, el máximo 0.4.
        assert_eq!(wave.light([1.0; 3], 0.0, 5.0), [0.3; 3]);
        assert!((wave.at(std::f32::consts::PI / 0.004, 0.0) - 0.4).abs() < 1e-4);
        // 34: la malla 1 cromada sumando con la misma luz.
        let l = resolve_all(KALIMA_34, &obj(), &mesh(1, false));
        assert_eq!(l.len(), 3);
        assert_eq!((l[2].1.blend, l[2].1.texture, l[2].1.color), (LookBlend::Add, Some(Bitmap::Chrome), LookColor::Flat));
        assert!(resolve_all(KALIMA_34, &obj(), &mesh(0, false)).len() == 2);
    }

    #[test]
    fn land_of_trials() {
        assert!(rules(32, 1).unwrap().hidden && rules(32, 53).unwrap().hidden);
        let l = resolve_all(rules(32, 27).unwrap().passes, &obj(), &mesh(0, false));
        let wave = l[0].1.body_wave.unwrap();
        // `BodyLight *= sin(Timer + WT·0.0012)·0.5 + 0.9`: con Timer = π/2 y WT = 0, ×1.4.
        let light = wave.light([0.5, 0.5, 0.5], 0.0, std::f32::consts::FRAC_PI_2);
        assert!((light[0] - 0.7).abs() < 1e-5);
        // 52: sin Timer.
        let l = resolve_all(rules(32, 52).unwrap().passes, &obj(), &mesh(0, false));
        assert_eq!(l[0].1.body_wave.unwrap().light([1.0; 3], 0.0, 3.0), [0.9; 3]);
        let l = resolve_all(rules(32, 10).unwrap().passes, &obj(), &mesh(0, false));
        assert_eq!(l[0].1.light, Some([0.56, 0.80, 0.81]));
        // `Timer`: 0..9.99 en pasos de 0.01 solo en 27 y 54.
        for i in 0..50 {
            let t = create_timer(32, 27, i);
            assert!((0.0..10.0).contains(&t) && ((t * 100.0).round() - t * 100.0).abs() < 1e-3);
        }
        assert_eq!(create_timer(32, 52, 3), 0.0);
        assert_eq!(create_timer(25, 27, 3), 0.0);
    }
}
