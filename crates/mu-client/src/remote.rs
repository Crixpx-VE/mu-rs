//! Los demás jugadores del mapa, como los manda el servidor:
//! - `AddCharactersToScope`: aparecen (su clase, sin equipo) en su casilla;
//! - `ObjectWalked`: caminan los pasos que dio (o, si no vienen, hasta la casilla de llegada);
//! - `MapObjectOutOfScope`: desaparecen.
//!
//! Los mensajes llegan a veces mientras se carga el mapa: se guardan hasta que el juego está listo.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use mu_protocol::game::{self, ServerMessage};

use crate::Args;
use crate::account::Class;
use crate::model_light::ModelMaterial;
use crate::scene::GameScene;
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inbox>()
            .add_systems(PreUpdate, collect.after(crate::net::NetSystems))
            .add_systems(OnEnter(GameScene::CharacterSelect), |mut inbox: ResMut<Inbox>| inbox.0.clear())
            .add_systems(Update, (apply, walk, animate).chain().run_if(in_state(GameScene::Game)));
    }
}

/// Mensajes de otros jugadores aún sin aplicar.
#[derive(Resource, Default)]
struct Inbox(Vec<ServerMessage>);

fn collect(mut from: MessageReader<crate::net::FromServer>, mut inbox: ResMut<Inbox>) {
    for m in from.read() {
        if matches!(m.0, ServerMessage::AddCharactersToScope(_) | ServerMessage::ObjectWalked { .. } | ServerMessage::MapObjectOutOfScope(_)) {
            inbox.0.push(m.0.clone());
        }
    }
}

/// Otro jugador.
#[derive(Component)]
pub struct RemotePlayer {
    pub id: u16,
    pub name: String,
    class: Class,
    /// Posición en casillas (fraccionaria mientras camina).
    pos: Vec2,
    path: VecDeque<(usize, usize)>,
    facing: f32,
    action: usize,
    frame: f32,
    stop: usize,
    walk: usize,
    materials: Vec<Handle<ModelMaterial>>,
    light: Vec3,
}

/// Casillas por segundo al caminar (las del jugador).
const WALK_SPEED: f32 = 3.6;

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut inbox: ResMut<Inbox>,
    mut remotes: Query<(Entity, &mut RemotePlayer, &crate::shadow::Shadow)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
) {
    // Hasta que el mapa esté cargado.
    let Some(terrain) = terrain else { return };
    for msg in std::mem::take(&mut inbox.0) {
        match msg {
            ServerMessage::AddCharactersToScope(list) => {
                for mut c in list {
                    // `Key >> 15` marca "recién aparecido"; el id son los 15 bits de abajo.
                    c.id &= game::OBJECT_ID_MASK;
                    // Si ya estaba (otra vez en la lista), se coloca de nuevo.
                    if let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == c.id) {
                        r.pos = Vec2::new(c.x as f32 + 0.5, c.y as f32 + 0.5);
                        r.path.clear();
                        continue;
                    }
                    let class = Class::from_index(game::base_class_index(c.class));
                    let (stop, walk) = (crate::character::stop_action(class, false), crate::character::walk_action(class));
                    let Some(model) = crate::character::load_character(&args, class, stop, &mut meshes, &mut materials, &mut images)
                    else {
                        continue;
                    };
                    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
                    let r = RemotePlayer {
                        id: c.id,
                        name: c.name.clone(),
                        class,
                        pos: Vec2::new(c.x as f32 + 0.5, c.y as f32 + 0.5),
                        path: VecDeque::new(),
                        facing: rotation_facing(c.rotation),
                        action: stop,
                        frame: 0.0,
                        stop,
                        walk,
                        materials: model.materials,
                        light: Vec3::splat(-1.0),
                    };
                    info!("aparece {} ({})", r.name, class.name());
                    let transform = remote_transform(&r, &terrain);
                    let mut e = commands.spawn((
                        Name::new(format!("Jugador {}", c.name)),
                        crate::scene::WorldEntity,
                        transform,
                        Visibility::default(),
                        model.skinned,
                        shadow,
                        r,
                    ));
                    for child in model.children {
                        e.with_child(child);
                    }
                    // Si su casilla de llegada no es la de ahora, que camine hasta ella.
                    if (c.target_x, c.target_y) != (c.x, c.y) {
                        inbox.0.push(ServerMessage::ObjectWalked {
                            id: c.id,
                            target_x: c.target_x,
                            target_y: c.target_y,
                            rotation: c.rotation,
                            directions: Vec::new(),
                        });
                    }
                }
            }
            ServerMessage::ObjectWalked { id, target_x, target_y, rotation, directions } => {
                let id = id & game::OBJECT_ID_MASK;
                let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == id) else { continue };
                let from = (r.pos.x as usize, r.pos.y as usize);
                let mut path = VecDeque::new();
                if directions.is_empty() {
                    let to = (target_x as usize, target_y as usize);
                    if let Some(p) = mu_data::path::find_path(from, to, |x, y| terrain.0.walkable(x, y), 20_000) {
                        path = p.into_iter().skip(1).collect();
                    }
                } else {
                    let (mut x, mut y) = (from.0 as i32, from.1 as i32);
                    for d in directions {
                        let (dx, dy) = game::DIRECTIONS[(d & 7) as usize];
                        x += dx as i32;
                        y += dy as i32;
                        path.push_back((x.clamp(0, 255) as usize, y.clamp(0, 255) as usize));
                    }
                }
                if path.is_empty() {
                    r.facing = rotation_facing(rotation);
                }
                r.path = path;
            }
            ServerMessage::MapObjectOutOfScope(ids) => {
                for (e, r, shadow) in &remotes {
                    if ids.contains(&r.id) {
                        info!("desaparece {}", r.name);
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                    }
                }
            }
            _ => {}
        }
    }
}

/// Ángulo de MU de la `Rotation` (0..7) que manda el servidor.
fn rotation_facing(rotation: u8) -> f32 {
    crate::character::direction_facing(rotation)
}

fn remote_transform(r: &RemotePlayer, terrain: &LoadedTerrain) -> Transform {
    let h = terrain.0.height_at(r.pos.x, r.pos.y);
    Transform {
        translation: Vec3::new(r.pos.x, h / MU_SCALE, -r.pos.y),
        rotation: Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_z(r.facing),
        scale: Vec3::splat(r.class.game_scale() / MU_SCALE),
    }
}

fn walk(time: Res<Time>, terrain: Option<Res<LoadedTerrain>>, mut q: Query<(&mut RemotePlayer, &mut Transform)>) {
    let Some(terrain) = terrain else { return };
    for (mut r, mut tf) in &mut q {
        let mut budget = WALK_SPEED * time.delta_secs();
        while budget > 0.0 {
            let Some(&(nx, ny)) = r.path.front() else { break };
            let target = Vec2::new(nx as f32 + 0.5, ny as f32 + 0.5);
            let delta = target - r.pos;
            let dist = delta.length();
            if dist > 1e-4 {
                r.facing = delta.x.atan2(-delta.y);
            }
            if dist <= budget {
                r.pos = target;
                budget -= dist;
                r.path.pop_front();
            } else {
                r.pos += delta / dist * budget;
                budget = 0.0;
            }
        }
        let action = if r.path.is_empty() { r.stop } else { r.walk };
        if action != r.action {
            r.action = action;
            r.frame = 0.0;
        }
        *tf = remote_transform(&r, &terrain);
    }
}

/// Animación, sombra y la luz del terreno donde está.
fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut q: Query<(&mut RemotePlayer, &crate::character::Skinned, &crate::shadow::Shadow)>,
    mut visibility: Query<&mut Visibility, Without<RemotePlayer>>,
) {
    let Some(terrain) = terrain else { return };
    for (mut r, skinned, shadow) in &mut q {
        r.frame += crate::character::play_speed(r.action) * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        let ground = terrain.0.height_at(r.pos.x, r.pos.y);
        let mut sh = crate::shadow::Builder::new(
            Some(&terrain.0),
            Vec3::new(r.pos.x * MU_SCALE, r.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(r.facing),
            r.class.game_scale(),
        );
        skinned.pose(r.action, r.frame, false, &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        // `BodyLight`: la luz del terreno en su casilla.
        let size = mu_data::terrain::SIZE;
        let (x, y) = ((r.pos.x.max(0.0) as usize).min(size - 1), (r.pos.y.max(0.0) as usize).min(size - 1));
        let light = Vec3::from_array(terrain.0.light[y * size + x]);
        if light != r.light {
            r.light = light;
            for h in &r.materials {
                if let Some(mut m) = materials.get_mut(h) {
                    m.extension.light = light.extend(1.0);
                }
            }
        }
    }
}
