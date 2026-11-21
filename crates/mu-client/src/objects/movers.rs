//! Objetos del mapa que se mueven por su cuenta y sombras de `RenderBodyShadow`.
//!
//! - Puertas de Devias 20, 65, 88 (giran) y 86 (se desliza) al acercarse el personaje:
//!   `CreateObject` (`ZzzObject.cpp:4793-4800`) y `MoveObject` (4004-4047).
//! - Calaveras de Lost Tower 38/39 que salen despedidas al pisarlas: `CheckSkull`
//!   (`ZzzEffectFireLeave.cpp:88-116`).
//! - Sombra negra en el suelo de Blood Castle 28/29: `RenderObject` (`ZzzObject.cpp:1059-1076`) +
//!   `BMD::RenderBodyShadow` (`ZzzBMD.cpp:2190-2241`).
//!
//! Lo de `MoveObject` va a 25 ticks por segundo; al dibujar se interpola entre los dos últimos.

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

/// Objetos que se mueven (sus entidades comparten posición y giro).
#[derive(Resource, Default)]
pub struct Movers(pub Vec<Mover>, f32);

pub struct Mover {
    motion: Motion,
    /// `o->HeadTargetAngle`: la posición inicial (las puertas vuelven a ella).
    home: Vec3,
    /// `o->HeadAngle`: en las puertas, el giro inicial; en las calaveras, lo que gira cada tick.
    head_angle: Vec3,
    /// `o->Direction` (calaveras).
    direction: Vec3,
    position: Vec3,
    angle: Vec3,
    /// Estado del tick anterior, para interpolar.
    prev_position: Vec3,
    prev_angle: Vec3,
    scale: f32,
    entities: Vec<Entity>,
}

impl Mover {
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
        }
    }
}

/// `TurnAngle2(angle, a, d)` (`ZzzAI.cpp:75-97`): gira `angle` hacia `a` de `d` en `d` grados.
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

/// Un tick de una puerta (`MoveObject`, `ZzzObject.cpp:4008-4046`). Devuelve el sonido que pide.
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

/// `CheckSkull` (`ZzzEffectFireLeave.cpp:88-116`): `walking` = el personaje camina o corre.
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

/// `Hero->Object.CurrentAction` entre `PLAYER_WALK_MALE` y `PLAYER_RUN_RIDE_WEAPON` (o las carreras del
/// Rage Fighter).
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
            };
            match sound {
                Some(s @ "SOUND_BONE2") => sounds.play_object(s),
                Some(s) => sounds.play(s),
                None => {}
            }
        }
        let position = m.prev_position.lerp(m.position, between);
        // Las puertas giran por el camino corto (`TurnAngle2` da la vuelta por 0/360).
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

// --- Sombras ------------------------------------------------------------------------------------------

/// `RenderBodyShadow` de un objeto: lo que hace falta para rehacerla con su animación.
pub struct ShadowSource {
    pub model: Arc<bmd::Bmd>,
    /// Grupo de animación (`AnimatedModels`) si está animado.
    pub group: Option<usize>,
    /// Mallas que proyecta.
    pub meshes: Vec<usize>,
    /// Posición (MU), giro y escala del objeto.
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: f32,
    /// Altura del terreno bajo el objeto (`RequestTerrainHeight`).
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
        // `EnableAlphaTest` + `glColor4f(0, 0, 0, 1)`, sin textura ni escribir profundidad.
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
            // La malla cambia con la animación: su caja inicial no vale.
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

/// Un vértice ya transformado (MU) proyectado como `RenderBodyShadow` con `BodyOrigin` en el suelo:
/// `x += z·(x + 2000)/(z − 4000)` relativo al origen, y `z = 5` sobre él.
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
        // Gira de 10 en 10 hacia el objetivo por el lado corto.
        assert_eq!(turn_angle2(30.0, 90.0, 10.0), 40.0);
        assert_eq!(turn_angle2(90.0, 30.0, 10.0), 80.0);
        // Cerca del objetivo, salta a él.
        assert_eq!(turn_angle2(85.0, 90.0, 10.0), 90.0);
        // Por 0/360.
        assert_eq!(turn_angle2(5.0, 330.0, 10.0), 355.0);
        assert_eq!(turn_angle2(350.0, 20.0, 10.0), 0.0);
        // Ángulos negativos: se pasan a 0..360.
        assert_eq!(turn_angle2(-70.0, 90.0, 10.0), 300.0);
    }

    fn mover(motion: Motion, angle_z: f32) -> Mover {
        let o = MapObject { kind: 20, position: [1000.0, 1000.0, 0.0], angle: [0.0, 0.0, angle_z], scale: 1.0 };
        Mover::new(motion, &o, vec![])
    }

    #[test]
    fn puertas() {
        // Ángulo truncado como `(int)Angle % 360`.
        assert_eq!(mover(Motion::SwingDoor, 450.7).angle.z, 90.0);
        let mut m = mover(Motion::SwingDoor, 90.0);
        // A 100 del personaje: 30 - (200 - 100)·0.5 = -20.
        assert_eq!(door_tick(&mut m, Some(Vec2::new(1100.0, 1000.0))), Some("SOUND_DOOR01"));
        assert_eq!(m.angle.z, -20.0);
        // Lejos vuelve con `TurnAngle2` (-20 → 340 → hacia 90 por el lado corto: 350).
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
        // Quieto: nada.
        assert_eq!(skull_tick(&mut m, Some(Vec2::new(1010.0, 1000.0)), false), None);
        // Caminando encima: sale despedida hacia el otro lado.
        assert_eq!(skull_tick(&mut m, Some(Vec2::new(1010.0, 1000.0)), true), Some("SOUND_BONE2"));
        assert!((m.position.x - (1000.0 - 4.0 * 0.6)).abs() < 1e-4);
        assert!((m.angle.y - (-40.0 * 0.6)).abs() < 1e-4);
    }

    #[test]
    fn sombra() {
        // A ras de suelo no se inclina; a 100 de altura se va hacia -X y queda a 5 del suelo.
        let o = Vec3::new(0.0, 0.0, 50.0);
        assert_eq!(project_shadow(Vec3::new(10.0, 0.0, 50.0), o), Vec3::new(10.0, 0.0, 55.0));
        let p = project_shadow(Vec3::new(0.0, 0.0, 150.0), o);
        assert!((p.x - 100.0 * 2000.0 / (100.0 - 4000.0)).abs() < 1e-3 && p.z == 55.0);
    }
}
