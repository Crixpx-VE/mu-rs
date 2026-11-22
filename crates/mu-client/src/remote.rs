//! Los demás jugadores del mapa, como los manda el servidor:
//! - `AddCharactersToScope`: aparecen (su clase, sin equipo) en su casilla;
//! - `ObjectWalked`: caminan los pasos que dio (o, si no vienen, hasta la casilla de llegada);
//! - `MapObjectOutOfScope`: desaparecen;
//! - `ObjectAnimation` (`ReceiveAction`, WSclient.cpp:3120-3353): miran a su dirección, se paran en
//!   su casilla de llegada (`TargetX/Y`) y ponen la animación de la acción (sentarse, apoyarse,
//!   curarse, los gestos del chat...). Sentados, apoyados o curándose siguen así hasta que caminan;
//!   un gesto, hasta que acaba (`PlayerStopAnimationSetting`). Al aparecer, la postura que traen en
//!   la apariencia (`Class & 0x07`, WSclient.cpp:2167-2191).
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
        if matches!(
            m.0,
            ServerMessage::AddCharactersToScope(_)
                | ServerMessage::ObjectWalked { .. }
                | ServerMessage::MapObjectOutOfScope(_)
                | ServerMessage::ObjectAnimation { .. }
        ) {
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
    /// `TargetX/Y`: la casilla de llegada del último paseo (o la suya al aparecer).
    target: (u8, u8),
    facing: f32,
    action: usize,
    frame: f32,
    stop: usize,
    walk: usize,
    materials: Vec<Handle<ModelMaterial>>,
    light: Vec3,
}

impl RemotePlayer {
    /// Animación y fotograma de ahora (para posar otra vez sus piezas, p. ej. el resaltado).
    pub fn pose_state(&self) -> (usize, f32) {
        (self.action, self.frame)
    }
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
                        r.target = (c.x, c.y);
                        continue;
                    }
                    let class = Class::from_index(game::base_class_index(c.class));
                    let (stop, walk) = (crate::character::stop_action(class, false), crate::character::walk_action(class));
                    let Some(model) = crate::character::load_character(&args, class, stop, &mut meshes, &mut materials, &mut images)
                    else {
                        continue;
                    };
                    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
                    // `Class & 0x07`: 2 sentado, 3 apoyado, 4 curándose (1, aparecer con
                    // `CreateTeleportEnd`, no se pinta todavía).
                    let pose = match c.pose {
                        2 => Some(game::action::SIT1),
                        3 => Some(game::action::POSE1),
                        4 => Some(game::action::HEALING1),
                        _ => None,
                    };
                    let action = pose.and_then(|at| crate::character::action::for_at(at, class.female()).flatten()).unwrap_or(stop);
                    let r = RemotePlayer {
                        id: c.id,
                        name: c.name.clone(),
                        class,
                        pos: Vec2::new(c.x as f32 + 0.5, c.y as f32 + 0.5),
                        path: VecDeque::new(),
                        target: (c.x, c.y),
                        facing: rotation_facing(c.rotation),
                        action,
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
                        crate::names::Selectable::player(c.name.clone(), c.hero_state),
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
                r.target = (target_x, target_y);
            }
            ServerMessage::ObjectAnimation { id, direction, action, .. } => {
                let id = id & game::OBJECT_ID_MASK;
                let Some((_, mut r, _)) = remotes.iter_mut().find(|(_, r, _)| r.id == id) else { continue };
                // `Angle[2] = (Angle - 1) * 45`, `Movement = false` y a su `TargetX/Y`.
                r.facing = rotation_facing(direction);
                r.path.clear();
                r.pos = Vec2::new(r.target.0 as f32 + 0.5, r.target.1 as f32 + 0.5);
                match crate::character::action::for_at(action, r.class.female()) {
                    Some(a) => {
                        // `SetAction` no vuelve a empezar la que ya tiene.
                        let a = a.unwrap_or(r.stop);
                        if a != r.action {
                            r.action = a;
                            r.frame = 0.0;
                        }
                    }
                    None => debug!("{}: acción {action} sin animación", r.name),
                }
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
        // Sentado, apoyado o con un gesto sigue así mientras no camine (como el héroe).
        let keep = crate::character::action::resting(r.action) || crate::character::action::emote(r.action);
        let action = if !r.path.is_empty() {
            r.walk
        } else if keep {
            r.action
        } else {
            r.stop
        };
        if action != r.action {
            r.action = action;
            r.frame = 0.0;
        }
        *tf = remote_transform(&r, &terrain);
    }
}

/// Animación, sombra y la luz del terreno donde está.
pub(crate) fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut q: Query<(&mut RemotePlayer, &crate::character::Skinned, &crate::shadow::Shadow, Option<&crate::names::Selectable>)>,
    mut visibility: Query<&mut Visibility, Without<RemotePlayer>>,
) {
    let Some(terrain) = terrain else { return };
    for (mut r, skinned, shadow, selectable) in &mut q {
        r.frame += crate::character::play_speed(r.action) * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        // Un gesto acaba: parado (`PlayerStopAnimationSetting`). Una acción que `player.bmd` no
        // tiene (un número cualquiera del servidor), también.
        let frames = skinned.action_frames(r.action) as f32;
        if frames == 0.0 || (crate::character::action::emote(r.action) && r.frame >= frames - 1.0) {
            r.action = r.stop;
            r.frame = 0.0;
        }
        let ground = terrain.0.height_at(r.pos.x, r.pos.y);
        let mut sh = crate::shadow::Builder::new(
            Some(&terrain.0),
            Vec3::new(r.pos.x * MU_SCALE, r.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(r.facing),
            r.class.game_scale(),
        );
        skinned.pose(r.action, r.frame, false, &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        // `BodyLight`: la luz del terreno en su casilla más `o->Light` (`VectorAdd(Light, o->Light,
        // c->Light)`, ZzzCharacter.cpp:8909; lo pone `SelectCharacter`, ver `names.rs`).
        let size = mu_data::terrain::SIZE;
        let (x, y) = ((r.pos.x.max(0.0) as usize).min(size - 1), (r.pos.y.max(0.0) as usize).min(size - 1));
        let light = Vec3::from_array(terrain.0.light[y * size + x]) + selectable.map_or(Vec3::ZERO, |s| s.light);
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
