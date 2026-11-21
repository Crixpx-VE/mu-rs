//! Objetos del mapa con efectos: Aida (`WD_33AIDA`), Crywolf (`WD_34CRYWOLF_1ST`), Kanturu 1-3
//! (`WD_37KANTURU_1ST` .. `WD_39KANTURU_3RD`) y el mapa de los GM (`WD_40AREA_FOR_GM`, que usa el
//! código de Kanturu 1: `M40GMArea::IsGmArea`, `GM_kanturu_1st.cpp:23-26`).
//!
//! Traducido de `GMAida.cpp`, `GMCrywolf1st.cpp`, `GM_kanturu_1st.cpp`, `GM_Kanturu_2nd.cpp` y
//! `GM_Kanturu_3rd.cpp` (Main 5.2): lo de `MoveObject`, `RenderObjectVisual`, `RenderObjectMesh` y
//! `RenderAfterObjectMesh` que crea partículas, sprites, estelas, efectos, luces del suelo y sonidos.
//! Las mallas ocultas, pasadas, desplazamientos de UV y velocidades están en
//! `mu_data::objects::rules_kanturu`.
//!
//! `MoveObject` va para todos los objetos que se ven y en Aida, Kanturu 1, 2 y 3 todos piden el sonido
//! de ambiente del mapa (`PlayBuffer`, que no vuelve a empezar mientras suena): por eso se registran
//! todos los tipos de esos mapas. En Crywolf también, para hacer una vez por tick las gotas del agua del
//! suelo (`BITMAP_SPOT_WATER`, `FaceTexture`, `ZzzLodTerrain.cpp:1478-1487`).
//!
//! La niebla que se crea "si `HiddenMesh != -2`" (Aida 59/62/63, Kanturu 1 62/107/108, Kanturu 2
//! 45-47/49, Kanturu 3 1/11/46/49/50): `MoveObject` pone `HiddenMesh = -2` en cada tick, pero solo
//! mueve los objetos que se vieron en el fotograma anterior (`MoveObjects`, `ZzzObject.cpp:4516`), así
//! que las nubes salen el primer fotograma en que el objeto vuelve a verse (y siguen al objeto
//! mientras se vea). Como en Heaven (`objects_2.rs`): una marca [`FX_MAP_ANCHOR`] hace de `o->Target`
//! y se vuelven a crear cuando el objeto vuelve a moverse tras un hueco.
//!
//! También van aquí los efectos que solo usan estos mapas y no existían en el motor: `MODEL_GHOST`,
//! `MODEL_FENRIR_THUNDER` (subtipos 1 y 2) y `BITMAP_MAGIC` (subtipos 2, 3 y 7); se registran solo si
//! nadie lo ha hecho. Las mariposas (`MODEL_BUTTERFLY01`) son las de `objects_kalima.rs` y el agua
//! `BITMAP_TWINTAIL_WATER` la de `particles_2.rs`.
//!
//! Sonidos: los de Kanturu 1 y 2 (`SOUND_KANTURU_1ST_*`, `SOUND_KANTURU_2ND_*`) no están en
//! `Data/Audio/SoundFiles.json` y no suenan; en el mapa de los GM el original no los carga
//! (`MapManager.cpp` solo los carga en `WD_37KANTURU_1ST`): no se piden.
//!
//! TODO(original): lo que depende del estado del servidor se deja en el de sin evento: Crywolf en paz
//! (`m_OccupationState = CRYWOLF_OCCUPATION_STATE_PEACE`: sin el humo de 74/84 ocupados ni la estatua
//! 81 en guerra) y Kanturu 3 sin batalla (Maya bajo el suelo, 74 y 73 sin conquistar). El cambio de
//! tiempo de Crywolf (`Change_weather`/`weather`, `GMCrywolf1st.cpp:381-389`, 506-511) es del tiempo.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use bevy::math::{Vec2, Vec3};
use mu_data::terrain::{SIZE, attr};

use crate::fx::core::*;
use crate::fx::kinds::objects_2::FX_MAP_ANCHOR;

// `gMapManager.WorldActive`.
const WD_33AIDA: i32 = 33;
const WD_34CRYWOLF_1ST: i32 = 34;
const WD_37KANTURU_1ST: i32 = 37;
const WD_38KANTURU_2ND: i32 = 38;
const WD_39KANTURU_3RD: i32 = 39;
const WD_40AREA_FOR_GM: i32 = 40;

/// Carpeta de Crywolf (`WorldActive + 1`), para el trapecio del suelo que se ve.
const CRYWOLF_FOLDER: u16 = 35;

pub fn register(r: &mut Registry) {
    // Todos los tipos de cada mapa (ver arriba).
    for ty in 0..=78 {
        r.object(WD_33AIDA, ty, aida);
    }
    for ty in 0..=84 {
        r.object(WD_34CRYWOLF_1ST, ty, crywolf);
    }
    for ty in 0..=110 {
        r.object(WD_37KANTURU_1ST, ty, kanturu1);
        r.object(WD_40AREA_FOR_GM, ty, kanturu1);
    }
    for ty in 0..=84 {
        r.object(WD_38KANTURU_2ND, ty, kanturu2);
    }
    for ty in 0..=74 {
        r.object(WD_39KANTURU_3RD, ty, kanturu3);
    }

    let mut effect = |ty: i32, kind: EffectKind| {
        r.effects.entry(ty).or_insert(kind);
    };
    effect(MODEL_GHOST, EffectKind { create: Some(ghost_create), step: Some(ghost_step), draw: None });
    effect(MODEL_FENRIR_THUNDER, EffectKind { create: Some(fenrir_thunder_create), step: Some(fenrir_thunder_step), draw: None });
    effect(BITMAP_MAGIC, EffectKind { create: Some(magic_create), step: Some(magic_step), draw: None });
}

// --- Común ---------------------------------------------------------------------------------------

/// `rand() % n` en coma flotante.
fn rf(ctx: &mut Ctx, n: i32) -> f32 {
    (ctx.rand() % n) as f32
}

/// `PlayBuffer(id)` desde `MoveObject`: todos los objetos lo piden en cada tick, pero el sonido no
/// vuelve a empezar mientras suena; basta con pedirlo una vez por tick.
fn play_once_per_tick(ctx: &mut Ctx, id: &'static str) {
    if !ctx.sounds.contains(&id) {
        ctx.play_sound(id);
    }
}

/// `timeGetTime()`: milisegundos del reloj de verdad (no del tick), como el original.
fn time_get_time() -> u32 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u32
}

/// Si es la primera vez que se llama en este tick (se distingue el tick por `world_time`).
fn first_in_tick(slot: &Mutex<f32>, world_time: f32) -> bool {
    let mut last = slot.lock().unwrap_or_else(|e| e.into_inner());
    if *last == world_time {
        return false;
    }
    *last = world_time;
    true
}

/// `AddTerrainLight(..., int Range, ...)`: el radio se trunca a entero.
fn light_range(range: f32) -> f32 {
    range as i32 as f32
}

/// `b->TransformPosition(BoneTransform[bone], p, Position)` + `CreateSprite(ty, Position, scale, light, o)`.
fn bone_sprite(o: &MapObject, ctx: &mut Ctx, bone: usize, p: Vec3, ty: i32, scale: f32, light: Vec3) {
    let pos = o.bone_world(bone, p);
    ctx.create_sprite(ty, pos, scale, light, 0.0, 0);
}

/// `for i < count: CreateParticle(BITMAP_CLOUD, Position, Angle, Light, sub, Scale, o)` cuando el
/// objeto vuelve a verse (ver la nota del principio). `o.user[0]` guarda el último tick en que se movió.
fn clouds_on_visible(o: &mut MapObject, ctx: &mut Ctx, count: usize, sub: i32, light: Vec3) {
    let gap = ctx.world_time - o.user[0];
    o.user[0] = ctx.world_time;
    if o.created && gap <= 100.0 {
        return;
    }
    let anchor = ctx.create_effect(FX_MAP_ANCHOR, o.position, Vec3::ZERO, Vec3::ZERO, 0, None, -1, 0, 0, 0.0);
    for _ in 0..count {
        ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, sub, o.scale, Some(Target::Effect(anchor)));
    }
}

/// `int time = timeGetTime() % 1024; if (time >= 0 && time < 10) CreateEffect(MODEL_BUTTERFLY01, ..., 3, o)`
/// y `HiddenMesh = -2` (Aida 60, Kanturu 1 37). El efecto (`objects_kalima.rs`) recibe la escala del
/// dueño en `Scale` (`Direction = (0, -5·Owner->Scale, 0)`).
fn butterfly_point(o: &mut MapObject, ctx: &mut Ctx) {
    let time = time_get_time() % 1024;
    if time < 10 {
        ctx.create_effect(MODEL_BUTTERFLY01, o.position, o.angle, Vec3::ONE, 3, None, -1, 0, 0, o.scale);
    }
    o.hidden_mesh = -2;
}

/// Kanturu 1 105 y Kanturu 2 8: bola de energía en el hueso 4 con chispazos (`sub` de la estela).
fn energy_ball(o: &MapObject, ctx: &mut Ctx, sub: i32) {
    let lumi = ((ctx.world_time * 0.001).sin() + 1.0) * 0.5;
    let light = Vec3::splat(lumi);
    let position = o.bone_world(4, Vec3::ZERO);
    ctx.create_particle(BITMAP_ENERGY, position, o.angle, light, 0, 1.5, None);
    ctx.create_sprite(BITMAP_SPARK + 1, position, 10.0, light, 0.0, 0);
    let (mut start, mut end) = (position, position);
    start.x -= 50.0;
    start.x -= rf(ctx, 100);
    end.x += rf(ctx, 80);
    start.y -= rf(ctx, 50);
    end.y += rf(ctx, 50);
    start.z += 10.0;
    end.z += 10.0;
    if ctx.rand() % 20 == 0 {
        ctx.create_joint(BITMAP_JOINT_THUNDER, start, end, o.angle, sub, None, 40.0);
    }
}

/// Cristales (Kanturu 1 95, Kanturu 2 76, `RenderAfterObjectMesh`): un destello en los huesos 0-9.
fn crystal_sparks(o: &MapObject, ctx: &mut Ctx) {
    let light = Vec3::new(0.1, 0.1, 0.3);
    for bone in 0..10 {
        bone_sprite(o, ctx, bone, Vec3::ZERO, BITMAP_SPARK + 1, 7.5, light);
    }
}

/// Luz azulada de Kanturu 2 4 y Kanturu 3 5: hueso 1 con `(-1, 0, 0)` y `fLumi = (sin(WT·0.002) + 2)·0.5`.
fn blue_light(ctx: &Ctx) -> (f32, Vec3) {
    let lumi = ((ctx.world_time * 0.002).sin() + 2.0) * 0.5;
    (lumi, Vec3::new(lumi * 0.3, lumi * 0.5, lumi * 1.0))
}

// --- Aida (`GMAida.cpp`) --------------------------------------------------------------------------

/// Huesos, desplazamiento y escala del primer sprite de las flores 30 y 71 (`RenderAidaObjectVisual`
/// 108-153 y 257-301).
const FLOWER_SPOTS: [(usize, [f32; 3]); 6] = [
    (6, [0.0, -3.0, 1.0]),
    (7, [0.0, -3.0, 0.0]),
    (8, [0.0, -3.0, 0.0]),
    (12, [-3.0, -1.0, 0.0]),
    (13, [-3.0, -3.0, 0.0]),
    (17, [0.0, 2.0, -8.0]),
];

fn aida(o: &mut MapObject, ctx: &mut Ctx) {
    // `MoveAidaObject` (40-97).
    match o.ty {
        30 => {
            let lum = rf(ctx, 5) * 0.01;
            let light = Vec3::new(lum + 0.4, lum + 0.6, lum + 0.4);
            ctx.add_terrain_light(o.position.x, o.position.y, light, 2.0);
        }
        71 => {
            let lum = rf(ctx, 5) * 0.01;
            let light = Vec3::new(lum + 0.9, lum + 0.2, lum + 0.2);
            ctx.add_terrain_light(o.position.x, o.position.y, light, 2.0);
        }
        56..=59 | 62 | 63 | 67 | 70 => o.hidden_mesh = -2,
        _ => {}
    }
    play_once_per_tick(ctx, "SOUND_AIDA_AMBIENT");

    // `RenderAidaObjectVisual` (99-325).
    match o.ty {
        30 | 71 => {
            let (scales, light1, light2) = if o.ty == 30 {
                ([5.5, 4.5, 4.0, 4.5, 4.0, 4.0], Vec3::new(0.1, 0.1, 0.3), Vec3::splat(0.15))
            } else {
                ([6.0, 5.5, 5.0, 5.5, 5.0, 5.0], Vec3::new(0.25, 0.0, 0.0), Vec3::new(0.15, 0.0, 0.0))
            };
            for (k, &(bone, p)) in FLOWER_SPOTS.iter().enumerate() {
                // 71: el primero es (0.5, 0, 0); el resto (0.25, 0, 0).
                let light = if o.ty == 71 && k == 0 { Vec3::new(0.5, 0.0, 0.0) } else { light1 };
                let pos = o.bone_world(bone, Vec3::from_array(p));
                ctx.create_sprite(BITMAP_SPARK + 1, pos, scales[k], light, 0.0, 0);
                ctx.create_sprite(BITMAP_SPARK + 1, pos, 3.0, light2, 0.0, 0);
            }
        }
        56 => {
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_1, o.position, o.angle, Vec3::ONE, 2, o.scale, None);
            }
        }
        57 => ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 4, o.scale, None),
        58 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 2, o.scale, None);
            }
        }
        59 => clouds_on_visible(o, ctx, 20, 1, Vec3::new(0.01, 0.03, 0.05)),
        60 => butterfly_point(o, ctx),
        62 => clouds_on_visible(o, ctx, 20, 1, Vec3::splat(0.05)),
        63 => clouds_on_visible(o, ctx, 20, 1, Vec3::new(0.05, 0.02, 0.02)),
        67 => {
            o.light = Vec3::splat(0.3);
            ctx.create_particle(BITMAP_FLAME, o.position, o.angle, o.light, 7, o.scale, None);
        }
        70 => {
            let time = time_get_time() % 1024;
            if ctx.rand() % 5 == 1 && time < 30 {
                // `CreateEffect(MODEL_GHOST, ..., 0, o, -1, 0, o->Scale)`: la escala cae en `Skill`
                // (WORD), que es el `Kind`; la del efecto queda la de por defecto.
                let kind = (o.scale as i32) & 0xFFFF;
                ctx.create_effect(MODEL_GHOST, o.position, o.angle, Vec3::splat(0.1), 0, None, -1, kind, 0, 0.0);
            }
        }
        75 => {
            // `fLumi` se calcula y se pisa con (1, 1, 1).
            let pos = o.bone_world(4, Vec3::ZERO);
            ctx.create_sprite(BITMAP_FLARE, pos, 3.0, Vec3::ONE, ctx.world_time / 10.0, 0);
        }
        _ => {}
    }
}

// --- Crywolf (`GMCrywolf1st.cpp`) -----------------------------------------------------------------

static CRYWOLF_TICK: Mutex<f32> = Mutex::new(f32::NAN);
/// `static float Ro` de `RenderCryWolf1stObjectVisual` (396-399): lo comparten todos los 41.
static CRYWOLF_RO: Mutex<f32> = Mutex::new(0.0);

/// `Time_Effect` de `RenderObjects` (`ZzzObject.cpp:3292-3294`): sube 1 cada fotograma de 1 a 41.
fn time_effect(world_time: f32) -> i32 {
    let tick = (world_time / 40.0).round() as i64;
    (tick.rem_euclid(41) + 1) as i32
}

fn crywolf(o: &mut MapObject, ctx: &mut Ctx) {
    if first_in_tick(&CRYWOLF_TICK, ctx.world_time) {
        spot_water(ctx);
    }

    // `MoveCryWolf1stObject` (329-371). TODO(original): 81 `Alpha = 0.2` (solo se dibuja en guerra).
    match o.ty {
        41 => ctx.add_terrain_light(o.position.x, o.position.y, Vec3::new(0.2, 0.7, 0.5), 2.0),
        57 | 71 => {
            let lum = (ctx.rand() % 4 + 3) as f32 * 0.1;
            let light = Vec3::new(lum, lum * 0.6, lum * 0.2);
            ctx.add_terrain_light(o.position.x, o.position.y, light, 3.0);
        }
        82..=84 => o.hidden_mesh = -2,
        _ => {}
    }

    // `RenderCryWolf1stObjectMesh` (430-572), en paz (`CRYWOLF_OCCUPATION_STATE_PEACE`).
    match o.ty {
        56 => {
            let color = (ctx.rand() % 10000) as f32 * 0.0001;
            if ctx.rand() % 3 == 0 {
                // `CreateEffect(BITMAP_MAGIC, ..., Light, 3, o, 4.0f)`: el 4 cae en `PKKey`.
                ctx.create_effect(BITMAP_MAGIC, o.position, o.angle, Vec3::splat(color), 3, None, 4, 0, 0, 0.0);
            }
        }
        57 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, Vec3::ONE, 5, o.scale, None);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale, None);
            }
        }
        71 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, Vec3::ONE, 5, o.scale, None);
            }
        }
        72 => {
            if ctx.rand() % 10 == 0 {
                let position = o.position + Vec3::new(0.0, 0.0, 50.0);
                let lum = (ctx.rand() % 4 + 3) as f32 * 0.1;
                let light = Vec3::new(lum, lum * 0.6, lum * 0.2);
                ctx.create_particle(BITMAP_SMOKE, position, o.angle, light, 33, o.scale, None);
            }
        }
        77 | 78 => {
            if ctx.rand() % 6 == 0 {
                let light = if o.ty == 77 { Vec3::new(0.02, 0.03, 0.02) } else { Vec3::new(0.03, 0.06, 0.05) };
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, light, 9, o.scale, None);
            }
        }
        _ => {}
    }

    // `RenderCryWolf1stObjectVisual` (373-429): dos destellos que suben y bajan sobre la columna 41.
    if o.ty == 41 {
        let ro = {
            let mut ro = CRYWOLF_RO.lock().unwrap_or_else(|e| e.into_inner());
            *ro += 1.0;
            if *ro >= 360.0 {
                *ro = 0.0;
            }
            *ro
        };
        let te = time_effect(ctx.world_time);
        let aaa = (if te > 20 { 40 - te } else { te }) as f32;
        let p = o.position + Vec3::new(0.0, 0.0, 350.0 + aaa);
        ctx.create_sprite(BITMAP_FLARE, p, 1.3, Vec3::ONE, 0.0, 0);
        ctx.create_sprite(BITMAP_FLARE, p, 1.3, Vec3::ONE, ro, 0);
    }
}

/// Cuántas veces mira `FaceTexture` (con `Water`) una casilla de Crywolf para soltar gotas
/// (`RenderTerrainFace`, `ZzzLodTerrain.cpp:1514-1565`): `alpha` son las cuatro esquinas
/// (`TerrainMappingAlpha` en 0-255).
fn water_rolls(layer1: u8, layer2: u8, alpha: [u8; 4]) -> u32 {
    // Con las cuatro esquinas del todo en la capa 2 se pinta la capa 2 sin agua.
    if alpha.iter().all(|&a| a == 255) || layer1 != 5 {
        return 0;
    }
    // La capa 2 encima con `Water` si también es la 5 (y hay algo de alfa).
    1 + u32::from(alpha.iter().any(|&a| a > 0) && layer2 == 5)
}

/// Gotas en el agua de Crywolf (`FaceTexture`, `ZzzLodTerrain.cpp:1478-1487`): en cada casilla de agua
/// que se dibuja, 1 de cada 50 veces una `BITMAP_SPOT_WATER` a la altura del personaje.
/// Las casillas que se dibujan son las del trapecio del original (`TestFrustrum2D` por bloques y por
/// casilla, `RenderTerrainBlock` 2485-2506), sin las `TW_NOGROUND` (`RenderTerrainTile` 1665).
/// La caja de bloques de la pirámide de la cámara (`FrustrumBoundMin/Max`) no se mira: queda fuera
/// del trapecio.
fn spot_water(ctx: &mut Ctx) {
    let Some((hero, hero_angle)) = ctx.hero_actor().map(|a| (a.position, a.angle)) else { return };
    let Some(t) = ctx.terrain else { return };
    let (fx, fy) = crate::terrain::trapezoid(CRYWOLF_FOLDER, Vec2::new(hero.x, hero.y));
    let shape = crate::terrain::CullShape { fx, fy, bound: [0, 0, SIZE as i32 - 1, SIZE as i32 - 1] };
    let lo = |v: [f32; 4]| v.iter().cloned().fold(f32::MAX, f32::min).floor().max(0.0) as usize;
    let hi = |v: [f32; 4]| (v.iter().cloned().fold(f32::MIN, f32::max).ceil().max(0.0) as usize).min(SIZE - 1);
    let alpha = |i: usize| t.mapping.alpha.get(i).copied().unwrap_or(0);
    let light = Vec3::new(0.30, 0.40, 0.20);
    for y in lo(fy)..=hi(fy) {
        for x in lo(fx)..=hi(fx) {
            if !shape.tile_visible(x, y) || t.attribute(x, y) & attr::NO_GROUND != 0 {
                continue;
            }
            // `TERRAIN_INDEX(xi, yi)` y las otras tres esquinas.
            let i = y * SIZE + x;
            let corners = [alpha(i), alpha(i + 1), alpha(i + SIZE + 1), alpha(i + SIZE)];
            for _ in 0..water_rolls(t.mapping.layer1[i], t.mapping.layer2[i], corners) {
                if ctx.rand() % 50 == 0 {
                    let sx = x as f32 * 100.0 + (ctx.rand() % 100 + 1) as f32;
                    let sy = y as f32 * 100.0 + (ctx.rand() % 100 + 1) as f32;
                    let pos = Vec3::new(sx, sy, hero.z + 10.0);
                    ctx.create_particle(BITMAP_SPOT_WATER, pos, hero_angle, light, 0, 1.0, None);
                }
            }
        }
    }
}

// --- Kanturu 1 y mapa de los GM (`GM_kanturu_1st.cpp`) ------------------------------------------

/// `::PlayBuffer(id)` de `MoveKanturu1stObject`: esos sonidos solo los carga `WD_37KANTURU_1ST`
/// (`MapManager.cpp:382-430`); en el mapa de los GM no suenan.
fn sound(ctx: &mut Ctx, id: &'static str) {
    if ctx.world_active == WD_37KANTURU_1ST {
        play_once_per_tick(ctx, id);
    }
}

fn kanturu1(o: &mut MapObject, ctx: &mut Ctx) {
    let wt = ctx.world_time;

    // `MoveKanturu1stObject` (43-128).
    match o.ty {
        59 | 62 | 81..=83 | 107 | 108 => o.hidden_mesh = -2,
        46 => sound(ctx, "SOUND_KANTURU_1ST_BG_WHEEL"),
        60 | 61 => {
            let lum = (ctx.rand() % 4 + 3) as f32 * 0.1;
            let light = if o.ty == 60 {
                Vec3::new(lum * 0.9, lum * 0.2, lum * 0.1)
            } else {
                Vec3::new(lum, lum * 0.6, lum * 0.2)
            };
            ctx.add_terrain_light(o.position.x, o.position.y, light, 3.0);
            o.hidden_mesh = -2;
        }
        70 => {
            let lum = (wt * 0.002).sin() * 0.45 + 0.55;
            let light = Vec3::new(lum * 1.4, lum * 0.7, lum * 0.4);
            ctx.add_terrain_light(o.position.x, o.position.y, light, 4.0);
        }
        77 => sound(ctx, "SOUND_KANTURU_1ST_BG_WATERFALL"),
        92 => sound(ctx, "SOUND_KANTURU_1ST_BG_ELEC"),
        97 => {
            o.hidden_mesh = -2;
            o.timer += 0.1;
            if o.timer > 10.0 {
                o.timer = 0.0;
            }
            if o.timer > 5.0 {
                ctx.create_particle(BITMAP_BUBBLE, o.position, o.angle, o.light, 5, 1.0, None);
            }
        }
        98 => sound(ctx, "SOUND_KANTURU_1ST_BG_PLANT"),
        _ => {}
    }
    sound(ctx, "SOUND_KANTURU_1ST_BG_GLOBAL");

    // `RenderKanturu1stObjectVisual` (130-308).
    match o.ty {
        37 => butterfly_point(o, ctx),
        59 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, Vec3::ONE, 21, o.scale, None);
            }
        }
        61 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TRUE_FIRE, o.position, o.angle, Vec3::ONE, 0, o.scale, None);
            }
        }
        62 => clouds_on_visible(o, ctx, 20, 20, Vec3::splat(0.04)),
        70 => {
            let pos = o.bone_world(6, Vec3::ZERO);
            let lum = (wt * 0.002).sin() + 1.8;
            ctx.create_sprite(BITMAP_SPARK + 1, pos, lum * 7.0, Vec3::new(0.8, 0.4, 0.2), 0.0, 0);
            ctx.create_sprite(BITMAP_SPARK + 1, pos, lum * 4.0, Vec3::splat(0.65), 0.0, 0);
        }
        81 => {
            if ctx.rand() % 2 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_1, o.position, o.angle, Vec3::ONE, 2, o.scale, None);
            }
        }
        82 => ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 4, o.scale, None),
        83 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_2, o.position, o.angle, Vec3::ONE, 2, o.scale, None);
            }
        }
        92 => {
            let start = o.bone_world(1, Vec3::ZERO);
            let end = o.bone_world(2, Vec3::ZERO);
            if ctx.rand() % 30 == 1 {
                ctx.create_joint(BITMAP_JOINT_THUNDER, start, end, o.angle, 18, None, 50.0);
            }
        }
        98 => {
            if ctx.rand() % 3 == 0 {
                let pos = o.bone_world(1, Vec3::ZERO);
                ctx.create_particle(BITMAP_TWINTAIL_WATER, pos, o.angle, Vec3::new(0.5, 0.6, 0.1), 2, 1.0, None);
            }
        }
        105 => energy_ball(o, ctx, 8),
        107 => clouds_on_visible(o, ctx, 20, 2, Vec3::splat(0.06)),
        108 => clouds_on_visible(o, ctx, 20, 7, Vec3::splat(0.2)),
        110 => {
            let lumi = ((wt * 0.002).sin() + 1.5) * 0.5;
            let light = Vec3::new(lumi * 0.6, lumi * 1.0, lumi * 0.8);
            bone_sprite(o, ctx, 1, Vec3::ZERO, BITMAP_LIGHT, 1.1, light);
        }
        _ => {}
    }

    // `RenderKanturu1stAfterObjectMesh` (343-402): los destellos del cristal 95.
    if o.ty == 95 {
        crystal_sparks(o, ctx);
    }
}

// --- Kanturu 2 (`GM_Kanturu_2nd.cpp`) -------------------------------------------------------------

fn kanturu2(o: &mut MapObject, ctx: &mut Ctx) {
    // `Sound_Kanturu2nd_Object` (198-214), desde `Move_Kanturu2nd_Object`.
    play_once_per_tick(ctx, "SOUND_KANTURU_2ND_MAPSOUND_GLOBAL");
    match o.ty {
        9 => play_once_per_tick(ctx, "SOUND_KANTURU_2ND_MAPSOUND_GEAR"),
        31 | 35..=37 => play_once_per_tick(ctx, "SOUND_KANTURU_2ND_MAPSOUND_INCUBATOR"),
        _ => {}
    }
    // `Move_Kanturu2nd_Object` (216-276).
    if matches!(o.ty, 45..=56 | 65) {
        o.hidden_mesh = -2;
    }

    // `Render_Kanturu2nd_ObjectVisual` (332-529).
    match o.ty {
        4 => {
            let (lumi, light) = blue_light(ctx);
            let pos = o.bone_world(1, Vec3::new(-1.0, 0.0, 0.0));
            let scale = lumi / 3.2;
            ctx.create_sprite(BITMAP_LIGHT, pos, scale, light, 0.0, 0);
            ctx.create_sprite(BITMAP_KANTURU_2ND_EFFECT1, pos, scale, light, 0.0, 0);
            ctx.create_sprite(BITMAP_KANTURU_2ND_EFFECT1, pos, scale, light, 0.0, 0);
        }
        8 => energy_ball(o, ctx, 18),
        10 => {
            if ctx.rand() % 6 == 0 {
                let pos = Vec3::new(o.position.x, o.position.y, 410.0);
                ctx.create_particle(BITMAP_WATERFALL_3, pos, o.angle, Vec3::ONE, 7, o.scale, None);
            }
        }
        45 => clouds_on_visible(o, ctx, 20, 1, Vec3::splat(0.06)),
        46 => clouds_on_visible(o, ctx, 20, 2, Vec3::splat(0.06)),
        47 => clouds_on_visible(o, ctx, 20, 7, Vec3::splat(0.2)),
        48 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, Vec3::splat(0.2), 10, o.scale, None);
            }
        }
        49 => clouds_on_visible(o, ctx, 5, 1, Vec3::new(0.0, 0.01, 0.03)),
        50 => {
            if ctx.rand() % 3 == 0 {
                let blue = rf(ctx, 3) * 0.01 + 0.02;
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, Vec3::new(0.0, 0.01, blue), 11, o.scale, None);
            }
        }
        51 => {
            if ctx.rand() % 3 == 0 {
                let red = rf(ctx, 3) * 0.01 + 0.01;
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, Vec3::new(red, 0.0, 0.0), 11, o.scale, None);
            }
        }
        52 => {
            if ctx.rand() % 6 == 0 {
                ctx.create_particle(BITMAP_WATERFALL_3, o.position, o.angle, Vec3::ONE, 7, o.scale, None);
            }
        }
        53 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TWINTAIL_WATER, o.position, o.angle, Vec3::new(0.4, 0.6, 0.7), 1, 1.0, None);
            }
        }
        54 | 56 | 65 => {
            let (chance, sub) = match o.ty {
                54 => (20, 8),
                56 => (10, 9),
                _ => (10, 10),
            };
            if ctx.rand() % chance == 0 {
                let scale = 30.0 + rf(ctx, 10);
                ctx.create_joint(BITMAP_JOINT_THUNDER + 1, o.position, o.position, o.angle, sub, None, scale);
            }
        }
        55 => {
            if ctx.rand() % 5 == 0 {
                // `CreateEffect(MODEL_FENRIR_THUNDER, ..., 1, o)`: el subtipo 1 no usa al dueño.
                let light = Vec3::new(0.8, 0.8, 1.0);
                ctx.create_effect(MODEL_FENRIR_THUNDER, o.position, o.angle, light, 1, None, -1, 0, 0, 0.0);
            }
        }
        _ => {}
    }

    // `Render_Kanturu2nd_AfterObjectMesh` (657-677): los destellos del cristal 76.
    if o.ty == 76 {
        crystal_sparks(o, ctx);
    }
}

// --- Kanturu 3 (`GM_Kanturu_3rd.cpp`) -------------------------------------------------------------

fn kanturu3(o: &mut MapObject, ctx: &mut Ctx) {
    let wt = ctx.world_time;
    // `MoveKanturu3rdObject` (120-198).
    match o.ty {
        25 => play_once_per_tick(ctx, "SOUND_KANTURU_3RD_MAP_SOUND05"),
        40..=42 => play_once_per_tick(ctx, "SOUND_KANTURU_3RD_MAP_SOUND01"),
        45 => {
            if ctx.rand() % 3 == 0 {
                o.hidden_mesh = -2;
                let lum = (ctx.rand() % 4 + 3) as f32 * 0.3;
                ctx.add_terrain_light(o.position.x, o.position.y, Vec3::splat(lum), light_range(1.0 + o.scale / 2.0));
            }
        }
        48 => o.hidden_mesh = -2,
        50 => {
            if ctx.rand() % 3 == 0 {
                let lum = rf(ctx, 10) * 0.2;
                ctx.add_terrain_light(o.position.x, o.position.y, Vec3::splat(lum), light_range(1.0 + o.scale));
            }
        }
        54 => {
            if o.hidden_mesh == -1 {
                o.hidden_mesh = -2;
                let lum = (ctx.rand() % 4 + 3) as f32 * 0.3;
                ctx.add_terrain_light(o.position.x, o.position.y, Vec3::splat(lum), light_range(1.0 + o.scale / 2.0));
            }
        }
        71 => play_once_per_tick(ctx, "SOUND_KANTURU_3RD_MAP_SOUND04"),
        73 => play_once_per_tick(ctx, "SOUND_KANTURU_3RD_MAP_SOUND03"),
        _ => {}
    }
    play_once_per_tick(ctx, "SOUND_KANTURU_3RD_AMBIENT");

    // `RenderKanturu3rdObjectVisual` (200-395). TODO(original): 0 (Maya): ver `rules_kanturu.rs`.
    match o.ty {
        1 => {
            clouds_on_visible(o, ctx, 10, 1, Vec3::new(0.0, 0.03, 0.04));
            o.hidden_mesh = -2;
        }
        5 => {
            let (lumi, light) = blue_light(ctx);
            let pos = o.bone_world(1, Vec3::new(-1.0, 0.0, 0.0));
            let scale = lumi / 2.4;
            ctx.create_sprite(BITMAP_LIGHT, pos, scale, light, 0.0, 0);
            ctx.create_sprite(BITMAP_SHINY + 1, pos, scale, light, ((wt / 2.0) as i32 % 360) as f32, 0);
            ctx.create_sprite(BITMAP_SHINY + 1, pos, scale, light, ((wt / 4.0) as i32 % 360) as f32, 0);
        }
        11 => {
            clouds_on_visible(o, ctx, 10, 3, Vec3::splat(0.1));
            o.hidden_mesh = -2;
        }
        32 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_TWINTAIL_WATER, o.position, o.angle, Vec3::new(0.4, 0.6, 0.7), 1, 1.0, None);
            }
        }
        46 => {
            clouds_on_visible(o, ctx, 20, 7, Vec3::splat(0.25));
            o.hidden_mesh = -2;
        }
        // 74: sin la torre conquistada (`KanturuSuccessMap`, estado del servidor).
        47 | 74 => {
            if ctx.rand() % 5 == 0 {
                ctx.create_joint(BITMAP_JOINT_THUNDER + 1, o.position, o.position, o.angle, 11, None, o.scale * 15.0);
            }
        }
        48 => {
            if ctx.rand() % 3 == 0 {
                ctx.create_particle(BITMAP_CLOUD, o.position, o.angle, o.light, 10, o.scale, None);
            }
        }
        49 => {
            clouds_on_visible(o, ctx, 10, 1, Vec3::new(0.0, 0.06, 0.10));
            o.hidden_mesh = -2;
        }
        50 => {
            clouds_on_visible(o, ctx, 10, 4, Vec3::splat(0.1));
            o.hidden_mesh = -2;
        }
        52 => {
            if ctx.rand() % 3 == 0 {
                o.light = Vec3::new(0.5, 1.0, 0.7);
                ctx.create_particle(BITMAP_TRUE_BLUE, o.position, o.angle, o.light, 1, o.scale, None);
            }
        }
        53 => {
            if ctx.rand() % 3 == 0 {
                o.light = Vec3::splat(0.3);
                ctx.create_particle(BITMAP_SMOKE, o.position, o.angle, o.light, 46, o.scale, None);
            }
        }
        54 => {
            if ctx.rand() % 3 == 0 {
                o.hidden_mesh = -1;
                ctx.create_joint(BITMAP_JOINT_THUNDER, o.position, o.position, o.angle, 19, None, o.scale * 10.0);
            }
        }
        _ => {}
    }
}

// --- Efectos: común -------------------------------------------------------------------------------

/// `Luminosity` del principio de `MoveEffect` (`ZzzEffect.cpp:6923-6928`; gasta un `rand()` siempre).
fn effect_luminosity(e: &Effect, ctx: &mut Ctx) -> f32 {
    let mut l = (ctx.rand() % 4 + 7) as f32 * 0.1;
    if e.life_time < 5.0 {
        l -= (5.0 - e.life_time) * 0.2;
        if l < 0.0 {
            l = 0.0;
        }
    }
    l
}

/// Cola común de `MoveEffect` (`ZzzEffect.cpp:18072-18123`): `PlayAnimation` (solo modelos) y
/// `MoveParticle(o, true)`.
fn move_tail(e: &mut Effect) {
    if e.ty >= MODEL_BIRD01 && e.ty < MODEL_SKILL_END {
        e.animation_frame += e.velocity;
    }
    e.position += rotate(e.angle, e.direction);
}

// --- MODEL_GHOST ------------------------------------------------------------------------------------

/// `CreateEffect` (`ZzzEffect.cpp:687-704`). El Main 5.2 no carga ningún modelo para `MODEL_GHOST`
/// (`RenderObject` no dibuja nada): solo se ve su `BITMAP_LIGHT`.
fn ghost_create(e: &mut Effect, ctx: &mut Ctx) {
    e.kind = 0;
    e.blend_mesh_light = 1.0;
    e.velocity = 1.0;
    e.life_time = 1000.0;
    e.gravity = 1.0;
    e.distance = 1.0;
    e.position.z += rf(ctx, 200) - 100.0;
    e.position.y += rf(ctx, 200) - 100.0;
    e.position.x += rf(ctx, 200) - 100.0;
    e.angle.z += rf(ctx, 360);
    e.direction = Vec3::new(0.0, rf(ctx, 8) * 0.1 - 5.0, 0.0);
    e.scale += rf(ctx, 5) / 40.0;
    e.start_position = e.position;
    e.light = Vec3::new(0.01, 0.01, 0.03);
    e.visible = false;
}

/// `MoveEffect` (`ZzzEffect.cpp:7357-7386`).
fn ghost_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = effect_luminosity(e, ctx);
    if ctx.rand() % 40 == 1 {
        e.velocity = -e.velocity;
        e.gravity = e.velocity * (rf(ctx, 3) + 5.0);
    }
    if ctx.rand() % 40 == 1 {
        e.velocity = -e.velocity;
        e.distance = e.velocity * (rf(ctx, 3) + 1.0);
    }
    if (e.position - e.start_position).length() >= 500.0 && e.kind == 0 {
        e.life_time = 50.0;
        e.kind = 1;
    }
    let wave = ((ctx.world_time as i32 % 10000) as f32 * 0.0001).sin();
    e.angle.z += wave * e.gravity;
    e.angle.x += wave * e.distance / 3.0;
    let l = e.blend_mesh_light;
    e.light = Vec3::new(l / 10.0, l / 10.0, l / 4.0);
    ctx.create_particle(BITMAP_LIGHT, e.position, e.angle, e.light, 9, e.scale, None);
    move_tail(e);
}

// --- MODEL_FENRIR_THUNDER ---------------------------------------------------------------------------

/// `m_iAnimation` (0 = aparece, 1 = se apaga).
const SLOT_ANIMATION: usize = 0;

/// `CreateEffect` (`ZzzEffect.cpp:4555-4600`): subtipos 1 y 2, que no usan al dueño.
/// TODO(original): 0 y 3 (los rayos del Fenrir, en los huesos de su dueño).
fn fenrir_thunder_create(e: &mut Effect, ctx: &mut Ctx) {
    match e.sub_type {
        1 | 2 => {
            e.life_time = if e.sub_type == 1 { 100.0 } else { 4.0 };
            let base = if e.sub_type == 1 { 0.3 } else { 0.1 };
            e.scale = base + rf(ctx, 100) * 0.002;
            e.alpha = 1.0;
            e.user[SLOT_ANIMATION] = 0.0;
            e.angle.x = rf(ctx, 360);
            e.angle.y = rf(ctx, 360);
            e.angle.z = rf(ctx, 360);
            e.position.x += rf(ctx, 40) - 20.0;
            e.position.y += rf(ctx, 40) - 20.0;
            e.position.z += rf(ctx, 40) - 20.0;
            let light = e.light - Vec3::splat(0.3);
            ctx.create_sprite(BITMAP_LIGHT, e.position, 2.0, light, 0.0, 0);
        }
        _ => e.live = false,
    }
}

/// `MoveEffect` (`ZzzEffect.cpp:14277-14342`): aparece (+0.3 de alfa por tick) y se apaga (-0.3).
/// Se dibuja con `RenderBody(RENDER_TEXTURE)` y `BodyLight = o->Light` (`ZzzObject.cpp:1701-1713`).
fn fenrir_thunder_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = effect_luminosity(e, ctx);
    if e.sub_type == 1 && e.live {
        if e.user[SLOT_ANIMATION] == 0.0 {
            e.alpha += 0.3;
            if e.alpha >= 1.0 {
                e.user[SLOT_ANIMATION] = 1.0;
                e.alpha = 1.0;
            }
            if e.alpha < 0.0 {
                e.alpha = 1.0;
            }
        } else {
            e.alpha -= 0.3;
            if e.alpha <= 0.0 {
                e.alpha = 0.0;
                e.live = false;
            }
        }
        e.angle += Vec3::splat(0.15);
    }
    move_tail(e);
}

// --- BITMAP_MAGIC (efecto) --------------------------------------------------------------------------

/// `CreateEffect` (`ZzzEffect.cpp:1296-1339`).
fn magic_create(e: &mut Effect, _ctx: &mut Ctx) {
    let scale = e.scale;
    e.life_time = 20.0;
    e.scale = 0.5;
    match e.sub_type {
        0 => e.life_time = 15.0,
        8 => {
            e.life_time = 30.0;
            e.scale = 1.0;
        }
        9 => {
            e.life_time = 40.0;
            e.scale = 2.4;
            e.head_angle = Vec3::ZERO;
        }
        10 => {
            e.life_time = 44.0;
            e.scale = 12.0;
            e.alpha = 0.0;
        }
        11 => {
            e.life_time = 24.0;
            e.scale = 0.8;
            e.head_angle = Vec3::ZERO;
        }
        // `Scale · 0.1` con el argumento `Scale` (0.9 si no se pasó).
        12 => e.scale = scale * 0.1,
        _ => {}
    }
    // Los subtipos 2, 3, 4 y 7 no se dibujan (`RenderEffects`, `ZzzEffect.cpp:19274-19332`).
    e.visible = false;
}

/// `MoveEffect` (`ZzzEffect.cpp:8244-8266`): subtipos 2, 3 y 7 (destellos que suben).
/// TODO(original): el resto de subtipos (y las marcas en el suelo de 1 y 8-12) no los usa nadie aún.
fn magic_step(e: &mut Effect, ctx: &mut Ctx) {
    let _ = effect_luminosity(e, ctx);
    if matches!(e.sub_type, 2 | 3 | 7) && e.life_time > 5.0 && e.life_time < 10.0 {
        let sub = if e.sub_type == 3 { 10 } else { 0 };
        ctx.create_particle(BITMAP_FLARE, e.position, e.angle, e.light, sub, 0.19, Some(Target::Effect(e.id)));
    }
    move_tail(e);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agua_de_crywolf() {
        // Capa 1 de agua con alguna esquina sin la capa 2 del todo: una tirada.
        assert_eq!(water_rolls(5, 3, [0, 0, 0, 0]), 1);
        // También la capa 2 es agua y hay mezcla: dos.
        assert_eq!(water_rolls(5, 5, [0, 128, 0, 0]), 2);
        assert_eq!(water_rolls(5, 5, [0, 0, 0, 0]), 1);
        // Las cuatro esquinas del todo en la capa 2: se pinta la 2 sin agua.
        assert_eq!(water_rolls(5, 5, [255; 4]), 0);
        // La capa 1 no es agua.
        assert_eq!(water_rolls(3, 5, [0, 128, 0, 0]), 0);
    }

    #[test]
    fn time_effect_de_1_a_41() {
        let values: Vec<i32> = (0..82).map(|t| time_effect(t as f32 * 40.0)).collect();
        assert_eq!(values[0], 1);
        assert_eq!(values[40], 41);
        assert_eq!(values[41], 1);
        assert!(values.iter().all(|v| (1..=41).contains(v)));
    }

    #[test]
    fn radio_entero_de_la_luz() {
        assert_eq!(light_range(1.0 + 1.98 / 2.0), 1.0);
        assert_eq!(light_range(1.0 + 2.3), 3.0);
    }
}
