//! Objetos del mapa con efectos de la pantalla de login (`WD_73NEW_LOGIN_SCENE`, carpeta World74)
//! y de la selección de personaje (`WD_74NEW_CHARACTER_SCENE`, carpeta World75).
//!
//! En el original estos dos mapas no tienen código propio: `GMNewTown.cpp` (`IsNewMap73_74`) pasa
//! `CreateObject`, `MoveObject`, `UpdateObjectEffects`, `RenderObject` y
//! `RenderObjectAfterCharacter` a `GMEmpireGuardian4.cpp`, porque `CMapManager::IsEmpireGuardian4()`
//! también es verdadero en 73 y 74. Aquí está lo de `GMEmpireGuardian4::CreateObject` (≈38-67),
//! `MoveObject` (≈84-176) y `UpdateObjectEffects` (≈670-984) que crea efectos; el dibujo (mallas
//! ocultas, mallas que suman, luz del cuerpo, velocidad de la animación) está en
//! `mu_data::objects::model_rules` (mapas 74 y 75), y el fundido por distancia de la cámara del
//! login (`CullObjects`, `ZzzObject.cpp` ≈4205-4246) en `objects.rs`.
//!
//! Sonidos: ninguno. `GMNewTown::PlayObjectSound` solo se llama en `WD_51HOME_6TH_CHAR`, y
//! `GMEmpireGuardian4::PlayObjectSound` (`SOUND_EMPIREGUARDIAN_INDOOR_SOUND`) solo lo llama
//! `MapProcess::MoveObject` para los mapas registrados, y 73/74 no lo están (solo `WD_72`).
//! `Data/Maps/ObjectSounds.json` tampoco tiene nada para 73/74.
//!
//! En el login el original solo suelta los efectos de los objetos que dibuja ese fotograma
//! (`MarkDrawn`: a menos de `RENDER_OBJECT_DIST` de la cámara): `objects.rs` lo apunta en
//! `user[`[`SLOT_CULLED`]`]` (1 = no se dibuja) y aquí no se crea nada mientras tanto.

use bevy::math::Vec3;

use crate::fx::core::*;

// `gMapManager.WorldActive`.
const WD_73NEW_LOGIN_SCENE: i32 = 73;
const WD_74NEW_CHARACTER_SCENE: i32 = 74;

/// Campo libre de [`MapObject::user`] que escribe `objects.rs` en el login: 1 si `CullObjects` no
/// dibuja el objeto este fotograma (lejos de la cámara), 0 si lo dibuja.
pub const SLOT_CULLED: usize = 3;

/// Tipos con código en `GMEmpireGuardian4::MoveObject` / `UpdateObjectEffects` / `CreateObject`.
/// TODO(original): 10 (`SubType = rand() % 50` en `CreateObject`, dos veces porque también lo llama
/// la cadena de `MapProcess::CreateObject`; luego `MoveObject` tiene la animación parada en el
/// fotograma 0 esos ticks): `objects.rs` anima los objetos por grupos y no puede retrasar uno.
/// TODO(original): 115/117 (`g_EmpireGuardian1.UpdateProjectileLauncher`: crea `MODEL_PROJECTILE`
/// con `SubType` 100/101), que no está en el motor; no hay ninguno en World74/75.
/// 157 (lámpara con velas) no hace nada en 73/74: `UpdateObjectEffects` vuelve antes.
const TYPES: [i32; 17] = [12, 20, 37, 50, 64, 79, 80, 82, 83, 84, 85, 86, 129, 130, 131, 132, 158];

pub fn register(r: &mut Registry) {
    for world in [WD_73NEW_LOGIN_SCENE, WD_74NEW_CHARACTER_SCENE] {
        for ty in TYPES {
            r.object(world, ty, empire_guardian4);
        }
    }
}

/// `Random::Rand() % n` en coma flotante.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `o->AnimationFrame` dentro de la acción 0 (como la deja `PlayAnimation`, que da la vuelta).
fn anim_frame(o: &MapObject) -> f32 {
    let Some(act) = o.model.as_ref().and_then(|m| m.actions.first()) else { return 0.0 };
    let frames = if act.lock_positions.is_some() { act.frames.saturating_sub(1) } else { act.frames };
    if frames <= 1 {
        return 0.0;
    }
    o.animation_frame.rem_euclid(frames as f32)
}

fn empire_guardian4(o: &mut MapObject, ctx: &mut Ctx) {
    // CreateObject (una vez): 129-132 se quedan con el giro en grados enteros (`(int)Angle[2] % 360`).
    if !o.created && (129..=132).contains(&o.ty) {
        o.angle.z = ((o.angle.z as i32) % 360) as f32;
    }

    // MoveObject: los puntos de fuego, agua y humo no se dibujan.
    if matches!(o.ty, 79 | 80 | 82..=86 | 129..=132) {
        o.hidden_mesh = -2;
    }

    // UpdateObjectVisual: solo los objetos que se dibujan este fotograma.
    if ctx.world_active == WD_73NEW_LOGIN_SCENE && o.user[SLOT_CULLED] != 0.0 {
        return;
    }
    // `Luminosity` del principio de `UpdateObjectVisual` (gasta un `rand()` siempre).
    let _luminosity = rf(ctx, 30);

    let wt = ctx.world_time;
    match o.ty {
        12 => {
            // `BITMAP_SHINY + 5` (sin `LDS_FIX_ACCESS_INDEXNUMBER_ALREADY_LOADTEXTURE`).
            let flumi = (wt * 0.0008).sin().abs() * 0.9 + 0.1;
            let scale = o.scale * 0.3 * flumi;
            let light1 = Vec3::splat(flumi);
            let light2 = Vec3::new(0.9, 0.1, 0.1);
            let pos = o.bone_world(2, Vec3::new(8.0, -3.0, -3.0));
            ctx.create_sprite(BITMAP_SHINY + 5, pos, 0.5, light2, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, pos, scale, light1, 0.0, 0);
            let pos = o.bone_world(3, Vec3::new(3.0, -3.0, -3.5));
            ctx.create_sprite(BITMAP_SHINY + 5, pos, 0.5, light2, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 5, pos, scale, light1, 0.0, 0);
        }
        20 => {
            // Chispas al golpear (dos momentos de la animación, uno girado 180°).
            let frame = anim_frame(o);
            let turn = if frame > 5.4 && frame < 6.5 {
                Some(180.0)
            } else if frame > 15.4 && frame < 16.5 {
                Some(0.0)
            } else {
                None
            };
            if let Some(turn) = turn {
                let mut angle = Vec3::ZERO;
                for _ in 0..4 {
                    angle = Vec3::new((ctx.rand() % 60 + 60 + 90) as f32, 0.0, o.angle.z + turn);
                    // `o` como `Target`: el subtipo 5 no lo usa.
                    ctx.create_joint(BITMAP_JOINT_SPARK, o.position, o.position, angle, 5, None, 10.0);
                }
                ctx.create_particle(BITMAP_SPARK, o.position, angle, o.light, 11, 1.0, None);
            }
        }
        37 => {
            let pos = o.bone_world(1, Vec3::ZERO);
            let lumi = ((wt * 0.039).sin() + 1.0) * 0.2 + 0.6;
            ctx.create_sprite(BITMAP_FLARE, pos, 4.0 * o.scale, Vec3::splat(lumi * 0.7), 0.0, 0);
        }
        50 => {
            let angle = Vec3::new(0.0, -1.0, 0.0);
            let light1 = Vec3::new(0.05, 0.1, 0.3);
            let light2 = Vec3::ONE;
            for bone in 2..=7 {
                let pos = o.bone_world(bone, Vec3::new(0.0, 0.0, 5.0));
                ctx.create_particle(BITMAP_FIRE_HIK3_MONO, pos, angle, light1, 4, o.scale * 0.6, None);
                ctx.create_particle(BITMAP_FIRE_HIK3_MONO, pos, angle, light2, 4, o.scale * 0.3, None);
            }
        }
        64 => {
            let frame = anim_frame(o);
            if (frame > 9.5 && frame < 11.5) || (frame > 23.5 && frame < 25.5) {
                let angle = Vec3::new(0.0, 0.0, o.angle.z + 90.0);
                let position = rotate(angle, Vec3::new(0.0, 30.0, 0.0)) + o.position;
                let light = Vec3::new(0.04, 0.03, 0.02);
                for _ in 0..3 {
                    // `o` como dueño: el subtipo 22 no lo usa.
                    ctx.create_particle(BITMAP_CLOUD, position, o.angle, light, 22, o.scale, None);
                }
            }
        }
        79 => {
            // Fuego: un resplandor rojo y una llama de tres tipos al azar.
            ctx.create_sprite(BITMAP_LIGHT, o.position, 2.0 * o.scale, Vec3::new(1.0, 0.2, 0.0), 0.0, 0);
            let light = Vec3::ONE;
            match ctx.rand() % 3 {
                0 => ctx.create_particle(BITMAP_FIRE_HIK1, o.position, o.angle, light, 0, o.scale, None),
                1 => ctx.create_particle(BITMAP_FIRE_CURSEDLICH, o.position, o.angle, light, 4, o.scale, None),
                _ => ctx.create_particle(BITMAP_FIRE_HIK3, o.position, o.angle, light, 0, o.scale, None),
            }
        }
        80 => {
            let lumi = ((wt * 0.04).sin() + 1.0) * 0.3 + 0.4;
            let light = Vec3::new(lumi * 0.1, lumi * 0.1, lumi * 0.5);
            ctx.create_sprite(BITMAP_LIGHT, o.position, 8.0 * o.scale, light, 0.0, 0);
        }
        82 => {
            // `Light` sin inicializar en el original: el subtipo 9 pone la suya.
            ctx.create_particle(BITMAP_WATERFALL_5, o.position, o.angle, Vec3::ZERO, 9, o.scale, None);
        }
        83 => {
            ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 14, o.scale, None);
        }
        84 => {
            if ctx.rand_fps_check(8) {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 4, o.scale, None);
            }
        }
        85 => {
            let light = Vec3::splat(0.5);
            let remainder = (wt as i32) % 200;
            if (0..=1).contains(&remainder) {
                let n = ctx.rand() % 4 + 4;
                for _ in 0..n {
                    // `CreateEffectFpsChecked(BITMAP_FLAME, ..., 6, NULL, -1, 0, o->Scale)`: la escala
                    // cae en el argumento `Skill` (WORD); la del efecto queda la de por defecto.
                    let kind = (o.scale as i32) & 0xFFFF;
                    ctx.create_effect(BITMAP_FLAME, o.position, o.angle, light, 6, None, -1, kind, 0, 0.0);
                }
            }
        }
        86 | 129 | 130 => {
            // Niebla de colores que sube del suelo.
            if ctx.rand_fps_check(6) {
                let light = match o.ty {
                    86 => Vec3::new(0.05, 0.02, 0.01),
                    129 => Vec3::new(0.01, 0.02, 0.05),
                    _ => Vec3::new(0.01, 0.05, 0.02),
                };
                // `o` como dueño: el subtipo 21 no lo usa.
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 21, o.scale, None);
            }
        }
        131 => {
            if ctx.rand_fps_check(3) {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 22, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale * 2.0, None);
            }
        }
        132 => {
            if ctx.rand_fps_check(3) {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 60, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 60, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale * 2.0, None);
            }
        }
        158 => {
            // `Light` sin inicializar en el original: el subtipo 64 calcula la suya al moverse.
            ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ZERO, 64, o.scale, None);
        }
        _ => {}
    }
}
