//! Map objects that move on their own, and `RenderBodyShadow` shadows.
//!
//! - Devias doors 20, 65, 88 (swing) and 86 (slide) when the character approaches:
//!   `CreateObject` (`ZzzObject.cpp:4793-4800`) and `MoveObject` (4004-4047).
//! - Lost Tower skulls 38/39 that get kicked away when stepped on: `CheckSkull`
//!   (`ZzzEffectFireLeave.cpp:88-116`).
//! - Black ground shadow in Blood Castle 28/29: `RenderObject` (`ZzzObject.cpp:1059-1076`) +
//!   `BMD::RenderBodyShadow` (`ZzzBMD.cpp:2190-2241`).
//! - Grass floating on the Kalima sea (15, 29, 32): `CheckGrass` and the water height
//!   (`RenderHellasVisual`, `GMHellas.cpp:452-466`; see `water_terrain.rs`).
//! - Land of Trials 27, which bobs up and down (`MoveHuntingGroundObject`, `GMHuntingGround.cpp:91-96`).
//!
//! `MoveObject` logic runs at 25 ticks per second; drawing interpolates between the last two.

use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::objects::{self, MapObject, Motion};
use mu_data::bmd;

use crate::fx::material::{Blend, FxMaterial};
use crate::fx::tables::ids;
use crate::terrain::MU_SCALE;

/// Moving objects (their entities share position and rotation).
#[derive(Resource, Default)]
pub struct Movers(pub Vec<Mover>, f32);

pub struct Mover {
    motion: Motion,
    /// `o->HeadTargetAngle`: the initial position (doors return to it).
    home: Vec3,
    /// `o->HeadAngle`: for doors, the initial rotation; for skulls, the rotation per tick.
    head_angle: Vec3,
    /// `o->Direction` (skulls).
    direction: Vec3,
    position: Vec3,
    angle: Vec3,
    /// Previous tick state, for interpolation.
    prev_position: Vec3,
    prev_angle: Vec3,
    scale: f32,
    entities: Vec<Entity>,
    /// `o->Timer` (Land of Trials 27).
    timer: f32,
}

impl Mover {
    /// With the `o->Timer` given by `CreateObject`.
    pub fn with_timer(mut self, timer: f32) -> Self {
        self.timer = timer;
        self
    }

    pub fn new(motion: Motion, o: &MapObject, entities: Vec<Entity>) -> Self {
        let mut angle = Vec3::from_array(o.angle);
        let mut head_angle = Vec3::ZERO;
        if matches!(motion, Motion::SwingDoor | Motion::SlidingDoor) {
            // `o->Angle[2] = (float)((int)o->Angle[2] % 360); VectorCopy(o->Angle, o->HeadAngle)`.
            angle.z = ((angle.z as i32) % 360) as f32;
            head_angle = angle;
        }
        let position = Vec3::from_array(o.position);
        Self {
            motion,
            home: position,
            head_angle,
            direction: Vec3::ZERO,
            position,
            angle,
            prev_position: position,
            prev_angle: angle,
            scale: o.scale,
            entities,
            timer: 0.0,
        }
    }
}

/// One tick of the Kalima grass: `CheckGrass` (x, y; the height is set every frame).
fn kalima_float_tick(m: &mut Mover, hero: Option<Vec2>, action: usize) {
    let Some(hero) = hero else { return };
    let mut direction = m.direction.truncate();
    let mut position = m.position.truncate();
    crate::water_terrain::check_grass(&mut direction, &mut position, hero, crate::water_terrain::check_grass_walking(action));
    m.direction = direction.extend(0.0);
    m.position = position.extend(m.position.z);
}

/// `TurnAngle2(angle, a, d)` (`ZzzAI.cpp:75-97`): turns `angle` toward `a` in steps of `d` degrees.
pub fn turn_angle2(mut angle: f32, mut a: f32, d: f32) -> f32 {
    if angle < 0.0 {
        angle += 360.0;
    }
    if a < 0.0 {
        a += 360.0;
    }
    if angle < 180.0 {
        let aa = angle - d;
        if a >= angle + d && a < angle + 180.0 {
            angle += d;
        } else if aa >= 0.0 && (a >= angle + 180.0 || a < aa) {
            angle -= d;
        } else if aa < 0.0 && a >= angle + 180.0 && a < aa + 360.0 {
            angle = angle - d + 360.0;
        } else {
            angle = a;
        }
    } else {
        let aa = angle + d;
        if a < angle - d && a >= angle - 180.0 {
            angle -= d;
        } else if aa < 360.0 && (a < angle - 180.0 || a >= aa) {
            angle += d;
        } else if aa >= 360.0 && a < angle - 180.0 && a >= aa - 360.0 {
            angle = angle + d - 360.0;
        } else {
            angle = a;
        }
    }
    angle
}

/// One door tick (`MoveObject`, `ZzzObject.cpp:4008-4046`). Returns the sound it requests.
fn door_tick(m: &mut Mover, hero: Option<Vec2>) -> Option<&'static str> {
    let distance = hero.map(|h| (h - m.home.truncate()).length());
    match distance {
        Some(d) if d < 200.0 => {
            let push = 200.0 - d;
            if m.motion == Motion::SlidingDoor {
                let a = m.angle.z;
                if a == 90.0 {
                    m.position.y = m.home.y + push * 2.0;
                }
                if a == 270.0 {
                    m.position.y = m.home.y - push * 2.0;
                }
                if a == 0.0 {
                    m.position.x = m.home.x + push * 2.0;
                }
                if a == 180.0 {
                    m.position.x = m.home.x - push * 2.0;
                }
                Some("SOUND_DOOR02")
            } else {
                let h = m.head_angle.z;
                if h == 90.0 {
                    m.angle.z = 30.0 - push * 0.5;
                }
                if h == 270.0 {
                    m.angle.z = 330.0 + push * 0.5;
                }
                if h == 0.0 {
                    m.angle.z = 300.0 - push * 0.5;
                }
                if h == 180.0 {
                    m.angle.z = 240.0 + push * 0.5;
                }
                Some("SOUND_DOOR01")
            }
        }
        _ => {
            m.angle.z = turn_angle2(m.angle.z, m.head_angle.z, 10.0);
            m.position.x += (m.home.x - m.position.x) * 0.2;
            m.position.y += (m.home.y - m.position.y) * 0.2;
            None
        }
    }
}

/// `CheckSkull` (`ZzzEffectFireLeave.cpp:88-116`): `walking` = the character walks or runs.
fn skull_tick(m: &mut Mover, hero: Option<Vec2>, walking: bool) -> Option<&'static str> {
    let mut sound = None;
    if let Some(h) = hero
        && walking
        && m.direction.x < 0.1
    {
        let (dx, dy) = (h.x - m.position.x, h.y - m.position.y);
        if (dx * dx + dy * dy).sqrt() < 50.0 {
            m.direction = Vec3::new(-dx * 0.4, -dy * 0.4, 0.0);
            m.head_angle.y = -dx * 4.0;
            m.head_angle.x = -dy * 4.0;
            sound = Some("SOUND_BONE2");
        }
    }
    m.direction *= 0.6;
    m.head_angle *= 0.6;
    m.position += m.direction;
    m.angle += m.head_angle;
    sound
}

/// `Hero->Object.CurrentAction` between `PLAYER_WALK_MALE` and `PLAYER_RUN_RIDE_WEAPON` (or the Rage
/// Fighter runs).
fn is_walking(action: usize) -> bool {
    let a = action as i32;
    (ids::PLAYER_WALK_MALE..=ids::PLAYER_RUN_RIDE_WEAPON).contains(&a)
        || a == ids::PLAYER_RAGE_UNI_RUN
        || a == ids::PLAYER_RAGE_UNI_RUN_ONE_RIGHT
}

pub fn move_objects(
    time: Res<Time>,
    players: Query<&crate::character::Player>,
    mut movers: ResMut<Movers>,
    mut sounds: ResMut<crate::sound::SoundQueue>,
    mut transforms: Query<&mut Transform>,
) {
    if movers.0.is_empty() {
        return;
    }
    let player = players.single().ok();
    let hero = player.map(|p| p.pos * MU_SCALE);
    let walking = player.is_some_and(|p| is_walking(p.action));
    let action = player.map_or(0, |p| p.action);
    // `WorldTime` (ms).
    let world_time = crate::terrain::world_ms(&time) as f32;
    let tick_len = 1.0 / objects::TICKS_PER_SECOND;
    movers.1 += time.delta_secs();
    let mut ticks = 0;
    while movers.1 >= tick_len && ticks < 8 {
        movers.1 -= tick_len;
        ticks += 1;
    }
    movers.1 = movers.1.min(tick_len);
    let between = movers.1 / tick_len;
    for m in movers.0.iter_mut() {
        for _ in 0..ticks {
            m.prev_position = m.position;
            m.prev_angle = m.angle;
            let sound = match m.motion {
                Motion::SwingDoor | Motion::SlidingDoor => door_tick(m, hero),
                Motion::Skull => skull_tick(m, hero, walking),
                Motion::SledDog => None,
                Motion::KalimaFloat => {
                    kalima_float_tick(m, hero, action);
                    None
                }
                Motion::HuntingGroundVibrate => {
                    m.position.z += (m.timer + world_time * 0.0024).sin() * 0.3;
                    None
                }
            };
            match sound {
                Some(s @ "SOUND_BONE2") => sounds.play_object(s),
                Some(s) => sounds.play(s),
                None => {}
            }
        }
        let mut position = m.prev_position.lerp(m.position, between);
        if m.motion == Motion::KalimaFloat
            && let Some(h) = crate::water_terrain::height_at(position.x, position.y, world_time)
        {
            // `o->Position[2] = GetWaterTerrain(x, y) + 180` every frame.
            position.z = h + 180.0;
        }
        // Doors turn along the short path (`TurnAngle2` wraps through 0/360).
        let dz = (m.angle.z - m.prev_angle.z + 540.0).rem_euclid(360.0) - 180.0;
        let mut angle = m.prev_angle.lerp(m.angle, between);
        angle.z = m.prev_angle.z + dz * between;
        let t = super::mu_transform(position, angle, m.scale);
        for e in &m.entities {
            if let Ok(mut tr) = transforms.get_mut(*e) {
                tr.set_if_neq(t);
            }
        }
    }
}

// --- Shadows -------------------------------------------------------------------------------------------

/// An object's `RenderBodyShadow`: what is needed to rebuild it with its animation.
pub struct ShadowSource {
    pub model: Arc<bmd::Bmd>,
    /// Animation group (`AnimatedModels`) if animated.
    pub group: Option<usize>,
    /// Meshes it casts.
    pub meshes: Vec<usize>,
    /// Object position (MU), rotation and scale.
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Terrain height under the object (`RequestTerrainHeight`).
    pub ground: f32,
}

#[derive(Resource, Default)]
pub struct BodyShadows {
    shadows: Vec<(ShadowSource, Handle<Mesh>, bool)>,
    material: Option<Handle<FxMaterial>>,
}

impl BodyShadows {
    pub fn add(
        &mut self,
        commands: &mut Commands,
        meshes: &mut Assets<Mesh>,
        materials: &mut Assets<FxMaterial>,
        source: ShadowSource,
    ) {
        // `EnableAlphaTest` + `glColor4f(0, 0, 0, 1)`, no texture and no depth write.
        let material = self
            .material
            .get_or_insert_with(|| {
                let mut m = FxMaterial::new(None, Blend::Blend3);
                m.params.tint = Vec4::new(0.0, 0.0, 0.0, 1.0);
                materials.add(m)
            })
            .clone();
        let mesh = meshes.add(shadow_mesh(vec![[0.0; 3]; 3]));
        commands.spawn((
            Name::new("Sombra de objeto"),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            crate::scene::WorldEntity,
            // The mesh changes with the animation: its initial bounding box is useless.
            NoFrustumCulling,
        ));
        self.shadows.push((source, mesh, false));
    }
}

fn shadow_mesh(positions: Vec<[f32; 3]>) -> Mesh {
    let n = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; n])
}

/// An already transformed vertex (MU) projected like `RenderBodyShadow` with `BodyOrigin` on the ground:
/// `x += z·(x + 2000)/(z − 4000)` relative to the origin, and `z = 5` above it.
pub fn project_shadow(v: Vec3, origin: Vec3) -> Vec3 {
    let mut p = v - origin;
    p.x += p.z * (p.x + 2000.0) / (p.z - 4000.0);
    p.z = 5.0;
    p + origin
}

pub fn update_shadows(
    anims: Res<super::AnimatedModels>,
    mut shadows: ResMut<BodyShadows>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    for (s, mesh, done) in shadows.shadows.iter_mut() {
        let bones = match s.group.and_then(|g| anims.0.get(g)) {
            Some(a) => a.bones.clone(),
            None if *done => continue,
            None => s.model.bone_matrices(0, 0),
        };
        // `Position[2] = RequestTerrainHeight(...)`; `BodyOrigin = Position`.
        let origin = Vec3::new(s.position.x, s.position.y, s.ground);
        let mut out = Vec::new();
        for &i in &s.meshes {
            for p in s.model.pose_with(i, &bones).positions {
                let world = s.position + s.rotation * (Vec3::from_array(p) * s.scale);
                let q = project_shadow(world, origin);
                out.push([q.x / MU_SCALE, q.z / MU_SCALE, -q.y / MU_SCALE]);
            }
        }
        if out.is_empty() {
            out = vec![[0.0; 3]; 3];
        }
        if let Some(mut m) = meshes.get_mut(mesh) {
            *m = shadow_mesh(out);
        }
        *done = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn turn_angle2_como_el_original() {
        // Turns toward the target in steps of 10 along the short side.
        assert_eq!(turn_angle2(30.0, 90.0, 10.0), 40.0);
        assert_eq!(turn_angle2(90.0, 30.0, 10.0), 80.0);
        // Near the target, snaps to it.
        assert_eq!(turn_angle2(85.0, 90.0, 10.0), 90.0);
        // Through 0/360.
        assert_eq!(turn_angle2(5.0, 330.0, 10.0), 355.0);
        assert_eq!(turn_angle2(350.0, 20.0, 10.0), 0.0);
        // Negative angles are mapped to 0..360.
        assert_eq!(turn_angle2(-70.0, 90.0, 10.0), 300.0);
    }

    fn mover(motion: Motion, angle_z: f32) -> Mover {
        let o = MapObject { kind: 20, position: [1000.0, 1000.0, 0.0], angle: [0.0, 0.0, angle_z], scale: 1.0 };
        Mover::new(motion, &o, vec![])
    }

    #[test]
    fn puertas() {
        // Angle truncated like `(int)Angle % 360`.
        assert_eq!(mover(Motion::SwingDoor, 450.7).angle.z, 90.0);
        let mut m = mover(Motion::SwingDoor, 90.0);
        // 100 away from the character: 30 - (200 - 100)·0.5 = -20.
        assert_eq!(door_tick(&mut m, Some(Vec2::new(1100.0, 1000.0))), Some("SOUND_DOOR01"));
        assert_eq!(m.angle.z, -20.0);
        // Far away it returns with `TurnAngle2` (-20 → 340 → toward 90 along the short side: 350).
        assert_eq!(door_tick(&mut m, None), None);
        assert_eq!(m.angle.z, 350.0);
        let mut s = mover(Motion::SlidingDoor, 0.0);
        assert_eq!(door_tick(&mut s, Some(Vec2::new(1000.0, 1050.0))), Some("SOUND_DOOR02"));
        assert_eq!(s.position.x, 1300.0);
        door_tick(&mut s, None);
        assert!((s.position.x - 1240.0).abs() < 1e-3);
    }

    #[test]
    fn calavera() {
        let mut m = mover(Motion::Skull, 0.0);
        // Standing still: nothing.
        assert_eq!(skull_tick(&mut m, Some(Vec2::new(1010.0, 1000.0)), false), None);
        // Walking on top: it gets kicked to the other side.
        assert_eq!(skull_tick(&mut m, Some(Vec2::new(1010.0, 1000.0)), true), Some("SOUND_BONE2"));
        assert!((m.position.x - (1000.0 - 4.0 * 0.6)).abs() < 1e-4);
        assert!((m.angle.y - (-40.0 * 0.6)).abs() < 1e-4);
    }

    #[test]
    fn sombra() {
        // At ground level it does not tilt; at height 100 it shifts toward -X and ends up 5 above the ground.
        let o = Vec3::new(0.0, 0.0, 50.0);
        assert_eq!(project_shadow(Vec3::new(10.0, 0.0, 50.0), o), Vec3::new(10.0, 0.0, 55.0));
        let p = project_shadow(Vec3::new(0.0, 0.0, 150.0), o);
        assert!((p.x - 100.0 * 2000.0 / (100.0 - 4000.0)).abs() < 1e-3 && p.z == 55.0);
    }
}
