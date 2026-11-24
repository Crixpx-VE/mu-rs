//! Ground and water critters around the character (`MoveFishs` / `RenderFishs`,
//! GOBoid.cpp:1615-1888): 10 slots in Atlans, Tarkan, Kalima and Crywolf; 3 elsewhere.
//!
//! - Lorencia: fish (`Object1/Fish01`) on water tiles (`TerrainMappingLayer1 == 5`),
//!   almost transparent (alpha 0.2 or 0.3: with the original's alpha test the 0.2 ones are
//!   invisible, only their shadow shows).
//! - Dungeon: rats (`Object2/Rat01`) with their squeak. Arena: bugs (`Object7/Bug01`).
//! - Tarkan and Aida: fireflies (`Object9/Bug02`) with a `BITMAP_JOINT_ENERGY` 4 trail.
//! - Crywolf (outside the safe zone): scorpions (`Object35/scorpion`) with their
//!   `BITMAP_SCOLPION_TAIL` tail.
//! - Atlans: fish (`Object8/Fish04-07`) in the southern half; in the northern half, the glowing
//!   ones (`Fish08-09`, mesh 0 additive and no shadow).
//! - Kalima: no model, only `BITMAP_FLARE+1` 8 sparkles running over the clouds.
//!
//! They move in flocks like the birds (`MoveBoid`), stuck to the ground, and turn around when they
//! reach a place they must not be; the second time they leave. Non-fish stop when their `LifeTime`
//! ends and walk again 1 tick out of 64.

use bevy::prelude::*;
use mu_data::terrain::{Terrain, attr};

use crate::Args;
use crate::boids::{self, Ai, Hero, Kind, Model, Obj};
use crate::character::Player;
use crate::fx::FxWorld;
use crate::fx::core::{self as fxc, Rng};
use crate::model_light::ModelMaterial;
use crate::terrain::LoadedTerrain;

pub struct FishPlugin;

impl Plugin for FishPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, setup_fish.run_if(crate::scene::load_step(crate::scene::Step::Boids)))
            .add_systems(FixedUpdate, step_fish.before(crate::fx::sim::tick))
            .add_systems(Update, draw_fish);
    }
}

/// `MAX_FISHS` (_define.h:127).
const MAX_FISHS: usize = 10;
/// Water texture on layer 1 of the Lorencia ground.
const WATER_TILE: u8 = 5;

#[derive(Resource)]
struct Fishes {
    /// `WorldActive`.
    wa: i32,
    models: Vec<Option<Model>>,
    list: Vec<Obj>,
    rng: Rng,
}

impl Fishes {
    fn rand(&mut self) -> i32 {
        self.rng.rand()
    }
}

/// Slots used by each map (GOBoid.cpp:1655-1662).
fn fish_slots(wa: i32) -> usize {
    if matches!(wa, 7 | 8 | 34 | 67) || boids::in_hellas(wa) { MAX_FISHS } else { 3 }
}

/// `M33Aida::IsInAida` (Aida or its character selection).
fn in_aida(wa: i32) -> bool {
    wa == 33 || wa == 54
}

fn setup_fish(
    mut commands: Commands,
    args: Res<Args>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    let wa = args.world as i32 - 1;
    // `CMapManager::Load` (MapManager.cpp:49-186).
    let files: Vec<(&str, String)> = match wa {
        0 => vec![("Object1", "Fish01.bmd".into())],
        1 => vec![("Object2", "Rat01.bmd".into())],
        6 => vec![("Object7", "Bug01.bmd".into())],
        8 | 33 => vec![("Object9", "Bug02.bmd".into())],
        34 => vec![("Object35", "scorpion.bmd".into())],
        // `MODEL_FISH01 + 3..8` = `Fish04..09`.
        7 | 67 => (4..=9).map(|n| ("Object8", format!("Fish{n:02}.bmd"))).collect(),
        _ => vec![],
    };
    let models = files.iter().map(|(d, f)| boids::load_model(&args.data, d, f, &mut images)).collect();
    let list = (0..MAX_FISHS).map(|_| Obj::new(crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material))).collect();
    // A different sequence from `Boids` (same generator, offset).
    let mut rng = Rng::default();
    for _ in 0..97 {
        rng.rand();
    }
    commands.insert_resource(Fishes { wa, models, list, rng });
}

fn step_fish(
    fishes: Option<ResMut<Fishes>>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&Player>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut fx: ResMut<FxWorld>,
) {
    let (Some(mut f), Some(t), Ok(p)) = (fishes, terrain, players.single()) else { return };
    let f = &mut *f;
    let t = &t.0;
    let fx = &mut *fx;
    let hero = Hero::of(p, t);
    for i in 0..fish_slots(f.wa) {
        if !f.list[i].live {
            create_fish(f, i, &hero, t, fx);
            if !f.list[i].live {
                f.list[i].sync_anchor(fx);
                continue;
            }
        }
        f.list[i].remember();
        move_fish(f, i, &hero, t, &mut sound);
        f.list[i].sync_anchor(fx);
    }
}

/// Tile `(x, y)` is water on layer 1 (`TerrainMappingLayer1[Index] == 5`).
fn water_at(t: &Terrain, p: Vec3) -> bool {
    t.mapping.layer1.get(boids::tile_index(p.x, p.y)) == Some(&WATER_TILE)
}

/// The creation part of `MoveFishs` (GOBoid.cpp:1665-1777).
fn create_fish(f: &mut Fishes, i: usize, hero: &Hero, t: &Terrain, fx: &mut FxWorld) {
    let wa = f.wa;
    let (rx, ry) = (f.rand() % 1024 - 512, f.rand() % 1024 - 512);
    let pos = hero.pos + Vec3::new(rx as f32, ry as f32, 0.0);
    let wall = boids::wall_at(t, pos.x, pos.y);
    let ok = match wa {
        0 => water_at(t, pos),
        1 | 6 => wall < attr::NO_GROUND,
        7 | 67 | 8 | 34 => boids::open_wall(wall),
        _ => (boids::in_hellas(wa) || in_aida(wa)) && boids::open_wall(wall),
    };
    if !ok {
        return;
    }
    let (life, scale) = (f.rand() % 128, f.rand() % 4 + 4);
    let o = &mut f.list[i];
    o.initialize();
    o.live = true;
    o.alpha = 0.0;
    o.alpha_target = 1.0;
    o.blend_mesh = -1;
    o.light = Vec3::splat(0.5);
    o.angle = Vec3::ZERO;
    o.light_enable = true;
    o.alpha_enable = true;
    o.sub_type = 0;
    o.life = life;
    o.scale = scale as f32 * 0.1;
    o.gravity = 13.0;
    o.ai = Ai::Fly;
    o.shadow_on = true;
    o.model = Some(0);
    o.place(pos);
    match wa {
        0 => {
            let a = f.rand() % 2 + 2;
            let o = &mut f.list[i];
            o.kind = Kind::Fish;
            o.alpha_target = a as f32 * 0.1;
            o.velocity = 0.6 / o.scale;
        }
        1 => {
            let o = &mut f.list[i];
            o.kind = Kind::Rat;
            o.velocity = 0.6 / o.scale;
        }
        6 => {
            let o = &mut f.list[i];
            o.kind = Kind::Bug;
            o.velocity = 0.6 / o.scale;
        }
        33 | 8 => {
            let s = f.rand() % 4 + 8;
            let o = &mut f.list[i];
            o.kind = Kind::Firefly;
            o.scale = s as f32 * 0.1;
            o.velocity = 2.5 / o.scale;
            o.gravity = 9.0;
            o.life = 100;
            boids::joint_on(o, fx, fxc::BITMAP_JOINT_ENERGY, 4, 30.0);
        }
        34 => {
            if hero.safe {
                f.list[i].live = false;
            } else {
                let (s, v) = (f.rand() % 4 + 8, f.rand() % 8 + 1);
                let o = &mut f.list[i];
                o.kind = Kind::Scorpion;
                o.scale = s as f32 * 0.1;
                o.velocity = (v as f32 * 0.1) / o.scale;
                o.gravity = 1.0;
                o.life = 100;
                boids::joint_on(o, fx, fxc::BITMAP_SCOLPION_TAIL, 0, 30.0);
            }
        }
        7 | 67 => {
            let s = f.rand() % 2 + 8;
            let south = hero.pos.y * 0.01 < 128.0;
            let (ty, g) = if south { (f.rand() % 4, f.rand() % 2) } else { (f.rand() % 2, f.rand() % 2) };
            let (timer, rz) = (f.rand() % 32, f.rand() % 150 + 50);
            let o = &mut f.list[i];
            o.scale = s as f32 * 0.1;
            if south {
                o.kind = Kind::SeaFish;
                o.model = Some(ty as usize);
                o.velocity = 1.0 / o.scale;
                o.gravity = if g == 0 { 2.0 } else { 3.0 };
            } else {
                o.kind = Kind::GlowFish;
                o.model = Some(4 + ty as usize);
                o.blend_mesh = 0;
                o.blend_mesh_light = 1.0;
                o.shadow_on = false;
                o.velocity = 0.5 / o.scale;
                o.gravity = if g == 0 { 1.0 } else { 2.0 };
            }
            o.timer = timer as f32 * 0.1;
            let mut p = o.pos;
            p.z = boids::ground(t, p) + rz as f32;
            o.place(p);
        }
        _ => {}
    }
    if boids::in_hellas(wa) {
        let s = f.rand() % 4 + 8;
        let o = &mut f.list[i];
        o.kind = Kind::Flare;
        o.model = None;
        o.scale = s as f32 * 0.1;
        o.velocity = 2.5 / o.scale;
        o.gravity = 9.0;
        o.life = 70;
        boids::joint_on(o, fx, fxc::BITMAP_FLARE + 1, 8, 50.0);
    }
    // Without its model the original would not create it (empty `Models[o->Type]`).
    let o = &mut f.list[i];
    if o.model.is_some_and(|m| f.models.get(m).is_none_or(Option::is_none)) {
        o.live = false;
    }
}

/// What each live slot does in `MoveFishs` (GOBoid.cpp:1779-1886).
fn move_fish(f: &mut Fishes, i: usize, hero: &Hero, t: &Terrain, sound: &mut crate::sound::SoundQueue) {
    let wa = f.wa;
    let kind = f.list[i].kind;
    let speed = f.list[i].velocity * 0.5;
    boids::play_animation(&mut f.list[i], &f.models, speed);
    if kind == Kind::GlowFish {
        let o = &mut f.list[i];
        o.blend_mesh_light = o.timer.sin() * 0.4 + 0.5;
        o.timer += 0.1;
    }
    let is_fish = matches!(kind, Kind::Fish | Kind::SeaFish | Kind::GlowFish);
    if is_fish || f.list[i].life > 0 {
        f.list[i].action = if matches!(kind, Kind::Bug | Kind::Scorpion) { 1 } else { 0 };
        boids::move_boid(&mut f.list, i);
        let r = f.rand() % 4 + 6;
        let o = &mut f.list[i];
        let d = fxc::rotate(o.angle, Vec3::new(o.velocity * r as f32, 0.0, 0.0));
        o.pos += d;
        // The original's condition here is always true (`!=7 || !InHellas || !=67`): all of them,
        // Atlans fish included, sit at ground height.
        o.pos.z = boids::ground(t, o.pos);
        o.dir.x = o.pos.x + 3.0 * d.x;
        o.dir.y = o.pos.y + 3.0 * d.y;
        let wall = boids::wall_at(t, o.pos.x, o.pos.y);
        let turn = if wa == 8 || boids::in_hellas(wa) || in_aida(wa) {
            wall == attr::SAFE_ZONE || wall >= attr::NO_GROUND
        } else {
            (kind == Kind::Fish && !water_at(t, o.pos))
                || (kind == Kind::Rat && wall >= attr::NO_GROUND)
                || ((wa == 7 || wa == 67) && (wall == attr::SAFE_ZONE || wall >= attr::NO_GROUND))
        };
        if turn {
            o.angle.z += 180.0;
            if o.angle.z >= 360.0 {
                o.angle.z -= 360.0;
            }
            o.sub_type += 1;
        } else if o.sub_type > 0 {
            o.sub_type -= 1;
        }
        if boids::in_hellas(wa) && o.life <= 1 {
            o.live = false;
        }
        if o.sub_type >= 2 {
            o.live = false;
        }
        let range = hero.range(o.pos);
        if range >= 1500.0 {
            o.live = false;
        }
        if range < 600.0 && kind == Kind::Rat && f.rand() % 256 == 0 {
            sound.play_object("SOUND_RAT01");
        }
    } else {
        f.list[i].action = 0;
    }
    f.list[i].life -= 1;
    if f.list[i].life <= 0 && kind != Kind::Bug && f.rand() % 64 == 0 {
        f.list[i].life = f.rand() % 128;
    }
    f.list[i].alpha_step();
}

fn draw_fish(
    mut commands: Commands,
    fishes: Option<ResMut<Fishes>>,
    fixed: Res<Time<Fixed>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut q: Query<(&mut Transform, &mut Visibility)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
) {
    let Some(mut f) = fishes else { return };
    let f = &mut *f;
    let a = fixed.overstep_fraction();
    let mut sprites = vec![];
    boids::draw_objs(
        &mut f.list,
        &f.models,
        a,
        terrain.as_deref().map(|t| &t.0),
        &mut commands,
        &mut q,
        &mut meshes,
        &mut materials,
        &mut sprites,
        &mut f.rng,
        "Pez",
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn huecos_por_mapa() {
        assert_eq!(fish_slots(0), 3);
        assert_eq!(fish_slots(1), 3);
        assert_eq!(fish_slots(7), 10);
        assert_eq!(fish_slots(8), 10);
        assert_eq!(fish_slots(25), 10);
        assert_eq!(fish_slots(33), 3);
        assert_eq!(fish_slots(34), 10);
    }
}
