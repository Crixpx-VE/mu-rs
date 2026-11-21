//! Objetos del mapa con efectos de Elbeland (`WD_51HOME_6TH_CHAR`), Swamp of Calmness
//! (`WD_56MAP_SWAMP_OF_QUIET`), Santa Village (`WD_62SANTA_TOWN`), Duel Arena (`WD_64DUELARENA`) y
//! Loren Market (`WD_79UNITEDMARKETPLACE`).
//!
//! Traducido de `GMNewTown.cpp`, `GMSwampOfQuiet.cpp`, `GMSantaTown.cpp`, `GMDuelArena.cpp` y
//! `GMUnitedMarketPlace.cpp` (Main 5.2): `MoveObject` (luces del suelo, sonidos) y
//! `RenderObjectVisual` / `RenderObjectAfterCharacter` (partículas y sprites). El dibujo de las
//! mallas (ocultas, que suman, cromadas, que se desplazan, velocidad de la animación) está en
//! `mu_data::objects::rules_elbeland`.
//!
//! Como en el original, el `RenderObjectVisual` común (`ZzzObject.cpp:2775`) gasta un `rand()`
//! (`Luminosity`) antes de llegar al código de cada mapa.
//!
//! Sonidos (`GMNewTown::PlayObjectSound`, 239-278): los objetos de Elbeland que se mueven piden su
//! sonido en cada tick; el original lo pide con `PlayBuffer(id, objeto)` sobre un búfer de un solo
//! canal, que no vuelve a empezar mientras suena. Los de agua no suenan si el héroe está en zona
//! segura. El de Swamp está comentado en el original; Santa, Duel Arena y el mercado no tienen.

use bevy::math::Vec3;

use super::objects_2::FX_MAP_ANCHOR;
use crate::fx::core::*;

// `gMapManager.WorldActive`.
const WD_51HOME_6TH_CHAR: i32 = 51;
const WD_56MAP_SWAMP_OF_QUIET: i32 = 56;
const WD_62SANTA_TOWN: i32 = 62;
const WD_64DUELARENA: i32 = 64;
const WD_79UNITEDMARKETPLACE: i32 = 79;

/// Tipos de Elbeland con código en `MoveObject`, `PlayObjectSound`, `RenderObjectVisual` o
/// `RenderObjectAfterCharacter` (sprites). Los de 133-155 (personajes y monstruos de adorno del
/// login antiguo) no están en `EncTerrain52.obj`.
const ELBELAND_TYPES: [i32; 16] = [0, 2, 53, 54, 56, 58, 59, 60, 61, 63, 78, 79, 85, 89, 110, 121];
const SWAMP_TYPES: [i32; 6] = [57, 71, 72, 74, 77, 78];
const SANTA_TYPES: [i32; 3] = [26, 27, 28];
const DUEL_TYPES: [i32; 3] = [34, 35, 36];
const MARKET_TYPES: [i32; 7] = [30, 35, 54, 55, 56, 57, 58];

pub fn register(r: &mut Registry) {
    for ty in ELBELAND_TYPES {
        r.object(WD_51HOME_6TH_CHAR, ty, elbeland);
    }
    for ty in SWAMP_TYPES {
        r.object(WD_56MAP_SWAMP_OF_QUIET, ty, swamp);
    }
    for ty in SANTA_TYPES {
        r.object(WD_62SANTA_TOWN, ty, santa);
    }
    for ty in DUEL_TYPES {
        r.object(WD_64DUELARENA, ty, duel);
    }
    for ty in MARKET_TYPES {
        r.object(WD_79UNITEDMARKETPLACE, ty, market);
    }
}

/// `rand() % n` en coma flotante.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// El `rand()` de `Luminosity` al principio de `RenderObjectVisual` (`ZzzObject.cpp:2775`).
fn visual_luminosity(ctx: &mut Ctx) {
    let _ = ctx.rand();
}

/// `(TerrainWall[héroe] & TW_SAFEZONE) == TW_SAFEZONE` (`GMNewTown.cpp:245-248`).
fn hero_in_safe_zone(ctx: &Ctx) -> bool {
    ctx.hero_actor()
        .map(|h| h.position)
        .is_some_and(|p| ctx.terrain_attribute(p.x, p.y) & mu_data::terrain::attr::SAFE_ZONE != 0)
}

/// Fuego de tres tipos al azar con su resplandor rojo (`GMDuelArena.cpp:143-165`,
/// `GMUnitedMarketPlace.cpp:303-325`).
fn fire_point(o: &MapObject, ctx: &mut Ctx) {
    ctx.create_sprite(BITMAP_LIGHT, o.position, 2.0 * o.scale, Vec3::new(1.0, 0.2, 0.0), 0.0, 0);
    let light = Vec3::ONE;
    match ctx.rand() % 3 {
        0 => ctx.create_particle(BITMAP_FIRE_HIK1, o.position, o.angle, light, 0, o.scale, None),
        1 => ctx.create_particle(BITMAP_FIRE_CURSEDLICH, o.position, o.angle, light, 4, o.scale, None),
        _ => ctx.create_particle(BITMAP_FIRE_HIK3, o.position, o.angle, light, 0, o.scale, None),
    }
}

// --- Elbeland (`GMNewTown.cpp`) -------------------------------------------------------------------

fn elbeland(o: &mut MapObject, ctx: &mut Ctx) {
    // `PlayObjectSound` (239-278), al principio de `MoveObject`.
    let sound = match o.ty {
        2 => Some(("SOUND_ELBELAND_WATERSMALL01", true)),
        53 => Some(("SOUND_ELBELAND_RAVINE01", true)),
        56 => Some(("SOUND_ELBELAND_ENTERATLANCE01", false)),
        59 => Some(("SOUND_ELBELAND_WATERFALLSMALL01", true)),
        85 => Some(("SOUND_ELBELAND_ENTERDEVIAS01", false)),
        89 => Some(("SOUND_ELBELAND_WATERWAY01", true)),
        110 => Some(("SOUND_ELBELAND_VILLAGEPROTECTION01", false)),
        _ => None,
    };
    // Búfer de un solo canal: varios objetos del mismo tipo en el mismo tick son una sola petición
    // (`sound.rs` aún no ve como "sonando" lo que pidió en este mismo fotograma).
    if let Some((id, not_in_safe_zone)) = sound
        && !(not_in_safe_zone && hero_in_safe_zone(ctx))
        && !ctx.sounds.contains(&id)
    {
        ctx.play_sound(id);
    }

    // `MoveObject` (124-235). Los desplazamientos de textura de 2/53/55/89 y la malla de 56 están
    // en las reglas de dibujo; el águila de 62 en `boids.rs`.
    match o.ty {
        0 => {
            let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l, l * 0.6, l * 0.2), 3.0);
            o.hidden_mesh = -2;
        }
        61 => {
            let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
            ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l * 0.2, l * 0.6, l), 3.0);
            o.hidden_mesh = -2;
        }
        54 | 58 | 59 | 60 => o.hidden_mesh = -2,
        _ => {}
    }

    // Lo que crea el dibujo: `RenderObjectVisual` (295-364) y, para 78/79,
    // `RenderObjectAfterCharacter` (964-990).
    visual_luminosity(ctx);
    let wt = ctx.world_time;
    match o.ty {
        0 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, Vec3::ONE, 0, o.scale, None);
            }
        }
        54 => {
            if ctx.rand() % 4 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 4, o.scale, None);
            }
        }
        58 => {
            // `Light` sin inicializar y `Scale` por defecto (1): el subtipo 0 pone su luz.
            ctx.create_particle(BITMAP_WATERFALL_5, o.position, o.angle, Vec3::ZERO, 0, 1.0, None);
        }
        59 => {
            ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 8, o.scale, None);
        }
        60 => elbeland_fog(o, ctx),
        61 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_BLUE, o.position, o.angle, Vec3::ONE, 0, o.scale, None);
            }
        }
        63 => {
            let lumi = ((wt * 0.001).sin() + 1.0) * 0.3 + 0.4;
            let light = Vec3::new(lumi, lumi, lumi * 0.6);
            let pos = o.bone_world(5, Vec3::new(-40.0, -10.0, 0.0));
            ctx.create_sprite(BITMAP_LIGHT, pos, o.scale * 6.0, light, 0.0, 0);
        }
        110 => {
            if ctx.rand() % 2 == 0 {
                let x = (wt * 0.03).cos() * (30.0 + rf(ctx, 5));
                let y = (wt * 0.03).sin() * (30.0 + rf(ctx, 5));
                let pos = o.bone_world(0, Vec3::new(x, y, 70.0));
                // `o` como dueño: el subtipo 11 no lo usa.
                ctx.create_particle(BITMAP_LIGHT, pos, o.angle, Vec3::ONE, 11, 0.6, None);
            }
        }
        121 => {
            let lumi = ((wt * 0.002).sin() + 1.0) * 0.5 + 0.5;
            let light = Vec3::new(lumi, lumi * 0.5, lumi * 0.3);
            let scale = lumi / 1.0;
            for bone in 3..=8 {
                let pos = o.bone_world(bone, Vec3::new(5.0, -4.0, -1.0));
                ctx.create_sprite(BITMAP_LIGHT, pos, scale, light, 0.0, 0);
            }
        }
        78 | 79 => {
            let last = if o.ty == 78 { 4 } else { 6 };
            for bone in 1..=last {
                let pos = o.bone_world(bone, Vec3::ZERO);
                ctx.create_sprite(BITMAP_SPARK + 1, pos, 5.5, Vec3::new(0.1, 0.1, 0.3), 0.0, 0);
            }
        }
        _ => {}
    }
}

/// 60 (`RenderObjectVisual` 318-334): con `HiddenMesh != -2` crea 10 nubes (`BITMAP_CLOUD`
/// subtipo 3) que siguen al objeto (`o->Target`) mientras se ve; al morir ponen
/// `Target->HiddenMesh = 0` y se vuelven a crear la próxima vez que se dibuja. Como las nubes de
/// Heaven (`objects_2.rs`): las sigue una marca ([`FX_MAP_ANCHOR`]) que muere cuando el objeto deja
/// de moverse, y se crean otra vez cuando vuelve a moverse tras un hueco.
fn elbeland_fog(o: &mut MapObject, ctx: &mut Ctx) {
    // `user[0]`: `world_time` de la última vez que se movió este objeto.
    let gap = ctx.world_time - o.user[0];
    o.user[0] = ctx.world_time;
    if !o.created || gap > 100.0 {
        let anchor = ctx.create_effect(FX_MAP_ANCHOR, o.position, Vec3::ZERO, Vec3::ZERO, 0, None, -1, 0, 0, 0.0);
        let light = Vec3::new(0.06, 0.07, 0.08);
        for _ in 0..10 {
            ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 3, o.scale, Some(Target::Effect(anchor)));
        }
    }
    o.hidden_mesh = -2;
}

// --- Swamp of Calmness (`GMSwampOfQuiet.cpp`) -----------------------------------------------------

fn swamp(o: &mut MapObject, ctx: &mut Ctx) {
    // `MoveObject` (79-105).
    if o.ty == 57 {
        let l = (ctx.rand() % 4 + 3) as f32 * 0.1;
        ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l, l * 0.6, l * 0.2), 3.0);
    }
    o.hidden_mesh = -2;

    // `RenderObjectVisual` (137-200).
    visual_luminosity(ctx);
    let light = Vec3::ONE;
    match o.ty {
        57 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, light, 5, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, light, 21, o.scale, None);
            }
        }
        71 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, light, 5, o.scale, None);
            }
        }
        72 => {
            if ctx.rand() % 6 == 0 {
                let position = o.position + Vec3::new(0.0, 0.0, 50.0);
                ctx.create_particle(BITMAP_SMOKE, position, o.angle, Vec3::splat(0.03), 49, o.scale, None);
            }
        }
        74 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, light, 21, o.scale * 2.0, None);
            }
        }
        77 | 78 => {
            if ctx.rand() % 6 == 0 {
                let light = if o.ty == 77 { Vec3::new(0.04, 0.06, 0.03) } else { Vec3::new(0.03, 0.03, 0.05) };
                // `o` como dueño: el subtipo 20 no lo usa.
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 20, o.scale, None);
            }
        }
        _ => {}
    }
}

// --- Santa Village (`GMSantaTown.cpp`) ------------------------------------------------------------

fn santa(o: &mut MapObject, ctx: &mut Ctx) {
    // `MoveObject` (95-102).
    o.hidden_mesh = -2;

    // `RenderObjectVisual` (149-186): luces de colores.
    visual_luminosity(ctx);
    let lumi = ((ctx.world_time * 0.005).sin() + 1.0) * 0.1 + 0.9;
    let light = match o.ty {
        26 => Vec3::new(lumi * 0.8, lumi * 0.2, lumi * 0.0),
        27 => Vec3::new(lumi * 0.0, lumi * 0.2, lumi * 0.8),
        _ => Vec3::new(lumi * 0.0, lumi * 0.6, lumi * 0.6),
    };
    ctx.create_sprite(BITMAP_LIGHT, o.position, 8.0 * o.scale, light, 0.0, 0);
}

// --- Duel Arena (`GMDuelArena.cpp`) ---------------------------------------------------------------

fn duel(o: &mut MapObject, ctx: &mut Ctx) {
    // `MoveObject` (74-91).
    if o.ty == 34 {
        let l = (ctx.rand() % 3 + 5) as f32 * 0.1;
        ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(l * 0.9, l * 0.2, l * 0.1), 3.0);
    }
    o.hidden_mesh = -2;

    // `RenderObjectVisual` (127-169).
    visual_luminosity(ctx);
    match o.ty {
        35 => {
            if ctx.rand() % 3 == 0 {
                // `o` como dueño: el subtipo 14 no lo usa.
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 14, o.scale, None);
            }
        }
        36 => fire_point(o, ctx),
        _ => {}
    }
}

// --- Loren Market (`GMUnitedMarketPlace.cpp`) -----------------------------------------------------

fn market(o: &mut MapObject, ctx: &mut Ctx) {
    let wt = ctx.world_time;
    // `MoveObject` (85-147).
    match o.ty {
        30 => {
            let pos = o.bone_world(1, Vec3::ZERO);
            let lumi = ((wt * 0.002).sin() + 1.0) * 0.2 + 0.6;
            ctx.add_terrain_light(pos.x, pos.y, Vec3::new(lumi * 0.7, lumi * 0.5, lumi * 0.2), 3.0);
        }
        35 => {
            let pos = o.bone_world(2, Vec3::ZERO);
            let lumi = ((wt * 0.039).sin() + 1.0) * 0.2 + 0.6;
            ctx.add_terrain_light(pos.x, pos.y, Vec3::new(lumi * 0.7, lumi * 0.5, lumi * 0.2), 1.0);
        }
        54..=58 => o.hidden_mesh = -2,
        _ => {}
    }

    // `RenderObjectVisual` (243-338).
    visual_luminosity(ctx);
    match o.ty {
        30 => {
            for bone in 1..=3 {
                let pos = o.bone_world(bone, Vec3::ZERO);
                let lumi = ((wt * 0.002).sin() + 1.0) * 0.2 + 0.6;
                ctx.create_sprite(BITMAP_FLARE, pos, 1.5 * o.scale, Vec3::splat(lumi * 0.5), 0.0, 0);
            }
        }
        35 => {
            let pos = o.bone_world(2, Vec3::ZERO);
            let lumi = ((wt * 0.039).sin() + 1.0) * 0.2 + 0.6;
            ctx.create_sprite(BITMAP_FLARE, pos, 1.5 * o.scale, Vec3::splat(lumi * 0.7), 0.0, 0);
        }
        54 => {
            // `Light` = (0, 0, 0) y `Scale` por defecto (1): el subtipo 0 pone su luz.
            ctx.create_particle(BITMAP_WATERFALL_5, o.position, o.angle, Vec3::ZERO, 0, 1.0, None);
        }
        55 => {
            ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 8, o.scale, None);
        }
        56 => {
            if ctx.rand() % 8 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 4, o.scale, None);
            }
        }
        57 => fire_point(o, ctx),
        58 => {
            let lumi = ((wt * 0.005).sin() + 1.0) * 0.3 + 0.5;
            let light = Vec3::new(lumi * 0.4, lumi * 0.4, lumi * 0.2);
            ctx.create_sprite(BITMAP_LIGHT, o.position, 20.0 * o.scale, light, 0.0, 0);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mu_data::objects::model_rules;

    /// Los tipos que el `MoveObject` del original oculta (`HiddenMesh = -2`) tampoco los dibujan las
    /// reglas de `mu_data` (la carpeta es `WorldActive + 1`), y los que se dibujan siguen visibles.
    #[test]
    fn ocultos_como_en_move_object() {
        let hidden: &[(i32, &[i32])] = &[
            (WD_51HOME_6TH_CHAR, &[0, 54, 58, 59, 60, 61, 62]),
            (WD_56MAP_SWAMP_OF_QUIET, &[57, 71, 72, 73, 74, 77, 78]),
            (WD_62SANTA_TOWN, &[26, 27, 28]),
            (WD_64DUELARENA, &[34, 35, 36]),
            (WD_79UNITEDMARKETPLACE, &[54, 55, 56, 57, 58]),
        ];
        for &(world, types) in hidden {
            for &ty in types {
                assert!(model_rules(world as u16 + 1, ty as i16).hidden, "WorldActive {world} tipo {ty}");
            }
        }
        for (world, ty) in [(WD_51HOME_6TH_CHAR, 63), (WD_51HOME_6TH_CHAR, 110), (WD_79UNITEDMARKETPLACE, 30)] {
            assert!(!model_rules(world as u16 + 1, ty as i16).hidden, "WorldActive {world} tipo {ty}");
        }
    }

    #[test]
    fn registrados() {
        let mut r = Registry::default();
        register(&mut r);
        assert_eq!(r.objects.len(), ELBELAND_TYPES.len() + SWAMP_TYPES.len() + SANTA_TYPES.len() + DUEL_TYPES.len() + MARKET_TYPES.len());
        assert!(r.objects.contains_key(&(WD_51HOME_6TH_CHAR, 60)));
        assert!(r.objects.contains_key(&(WD_79UNITEDMARKETPLACE, 30)));
    }
}
