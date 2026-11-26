//! Map objects with effects in Elbeland (`WD_51HOME_6TH_CHAR`), Swamp of Calmness
//! (`WD_56MAP_SWAMP_OF_QUIET`), Santa Village (`WD_62SANTA_TOWN`), Duel Arena (`WD_64DUELARENA`) and
//! Loren Market (`WD_79UNITEDMARKETPLACE`).
//!
//! Translated from `GMNewTown.cpp`, `GMSwampOfQuiet.cpp`, `GMSantaTown.cpp`, `GMDuelArena.cpp` and
//! `GMUnitedMarketPlace.cpp` (Main 5.2): `MoveObject` (ground lights, sounds) and
//! `RenderObjectVisual` / `RenderObjectAfterCharacter` (particles and sprites). Mesh drawing
//! (hidden, additive, chrome, scrolling, animation speed) lives in
//! `mu_data::objects::rules_elbeland`.
//!
//! As in the original, the shared `RenderObjectVisual` (`ZzzObject.cpp:2775`) consumes one `rand()`
//! (`Luminosity`) before reaching each map's code.
//!
//! Sounds (`GMNewTown::PlayObjectSound`, 239-278): moving Elbeland objects request their
//! sound every tick; the original does it with `PlayBuffer(id, object)` on a single-channel
//! buffer, which doesn't restart while playing. Water ones are silent if the hero is in the safe
//! zone. Swamp's is commented out in the original; Santa, Duel Arena and the market have none.

use bevy::math::Vec3;

use super::objects_2::FX_MAP_ANCHOR;
use crate::fx::core::*;

// `gMapManager.WorldActive`.
const WD_51HOME_6TH_CHAR: i32 = 51;
const WD_56MAP_SWAMP_OF_QUIET: i32 = 56;
const WD_62SANTA_TOWN: i32 = 62;
const WD_64DUELARENA: i32 = 64;
const WD_79UNITEDMARKETPLACE: i32 = 79;

/// Elbeland types with code in `MoveObject`, `PlayObjectSound`, `RenderObjectVisual` or
/// `RenderObjectAfterCharacter` (sprites). Types 133-155 (decorative characters and monsters of the
/// old login) are not in `EncTerrain52.obj`.
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

/// `rand() % n` as floating point.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// The `Luminosity` `rand()` at the start of `RenderObjectVisual` (`ZzzObject.cpp:2775`).
fn visual_luminosity(ctx: &mut Ctx) {
    let _ = ctx.rand();
}

/// `(TerrainWall[hero] & TW_SAFEZONE) == TW_SAFEZONE` (`GMNewTown.cpp:245-248`).
fn hero_in_safe_zone(ctx: &Ctx) -> bool {
    ctx.hero_actor()
        .map(|h| h.position)
        .is_some_and(|p| ctx.terrain_attribute(p.x, p.y) & mu_data::terrain::attr::SAFE_ZONE != 0)
}

/// Fire of three random kinds with its red glow (`GMDuelArena.cpp:143-165`,
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
    // `PlayObjectSound` (239-278), at the start of `MoveObject`.
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
    // Single-channel buffer: several objects of the same type in the same tick are one request
    // (`sound.rs` doesn't yet see as "playing" what it requested in this same frame).
    if let Some((id, not_in_safe_zone)) = sound
        && !(not_in_safe_zone && hero_in_safe_zone(ctx))
        && !ctx.sound_requested(id)
    {
        ctx.play_sound(id);
    }

    // `MoveObject` (124-235). The texture scrolls of 2/53/55/89 and the mesh of 56 are
    // in the draw rules; the eagle of 62 in `boids.rs`.
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

    // What drawing creates: `RenderObjectVisual` (295-364) and, for 78/79,
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
            // Uninitialized `Light` and default `Scale` (1): subtype 0 sets its own light.
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
                // `o` as owner: subtype 11 doesn't use it.
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

/// 60 (`RenderObjectVisual` 318-334): with `HiddenMesh != -2` creates 10 clouds (`BITMAP_CLOUD`
/// subtype 3) that follow the object (`o->Target`) while visible; when they die they set
/// `Target->HiddenMesh = 0` and are recreated the next time it is drawn. Like the Heaven
/// clouds (`objects_2.rs`): they follow a marker ([`FX_MAP_ANCHOR`]) that dies when the object stops
/// moving, and are created again when it moves again after a gap.
fn elbeland_fog(o: &mut MapObject, ctx: &mut Ctx) {
    // `user[0]`: `world_time` of the last time this object moved.
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
                // `o` as owner: subtype 20 doesn't use it.
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

    // `RenderObjectVisual` (149-186): colored lights.
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
                // `o` as owner: subtype 14 doesn't use it.
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
            // `Light` = (0, 0, 0) and default `Scale` (1): subtype 0 sets its own light.
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

    /// The types the original's `MoveObject` hides (`HiddenMesh = -2`) are not drawn by the
    /// `mu_data` rules either (the folder is `WorldActive + 1`), and the drawn ones stay visible.
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
