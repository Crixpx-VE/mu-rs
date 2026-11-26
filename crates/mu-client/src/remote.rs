//! The other players on the map, as the server sends them:
//! - `AddCharactersToScope`: they appear on their tile, dressed from their appearance
//!   (`ChangeCharacterExt`, WSclient.cpp:2247; hero_dress.rs);
//! - `AppearanceChanged` (`ReceiveChangePlayer`, WSclient.cpp:1871-2088): one piece changes and they
//!   are dressed again;
//! - `ObjectWalked`: they walk the steps taken (or, if none come, to the destination tile);
//! - `MapObjectOutOfScope`: they disappear;
//! - `ObjectAnimation` (`ReceiveAction`, WSclient.cpp:3120-3353): they face their direction, stop on
//!   their destination tile (`TargetX/Y`) and play the action's animation (sit, lean, heal, chat
//!   emotes...). Sitting, leaning or healing lasts until they walk; an emote, until it ends
//!   (`PlayerStopAnimationSetting`). On appearing, they take the posture carried in their
//!   appearance (`Class & 0x07`, WSclient.cpp:2167-2191).
//!
//! Standing and walking, they use `SetPlayerStop` / `SetPlayerWalk` (ZzzCharacter.cpp:233-361 and
//! 576-662, character.rs): outside a safe zone they fly with wings or take the stand and walk of their
//! weapons; inside, the unarmed ones, with the weapons on the back (`RenderCharacterBackItem`,
//! ZzzCharacter.cpp:14636-14662).
//!
//! Messages sometimes arrive while the map is loading: they are queued until the game is ready.
//! Those about ourselves (the id from `GameServerEntered`, `HeroKey`; OpenMU sends us in
//! `AddCharactersToScope`) create nobody.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;
use mu_protocol::appearance::Appearance;
use mu_protocol::game::{self, ServerMessage};

use crate::Args;
use crate::account::Class;
use crate::item_view::ItemData;
use crate::model_light::ModelMaterial;
use crate::scene::GameScene;
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct RemotePlugin;

impl Plugin for RemotePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inbox>()
            .init_resource::<HeroKey>()
            .add_systems(PreUpdate, collect.after(crate::net::NetSystems))
            .add_systems(OnEnter(GameScene::CharacterSelect), |mut inbox: ResMut<Inbox>| inbox.0.clear())
            .add_systems(Update, (apply, walk, animate).chain().run_if(in_state(GameScene::Game)));
    }
}

/// Messages from other players not applied yet.
#[derive(Resource, Default)]
struct Inbox(Vec<ServerMessage>);

/// Our object id (`HeroKey`, the one from `GameServerEntered`).
#[derive(Resource, Default)]
pub struct HeroKey(pub u16);

fn collect(mut from: MessageReader<crate::net::FromServer>, mut inbox: ResMut<Inbox>, mut hero: ResMut<HeroKey>) {
    for m in from.read() {
        if let ServerMessage::GameServerEntered { player_id, .. } = m.0 {
            hero.0 = player_id & game::OBJECT_ID_MASK;
        }
        if matches!(
            m.0,
            ServerMessage::AddCharactersToScope(_)
                | ServerMessage::ObjectWalked { .. }
                | ServerMessage::MapObjectOutOfScope(_)
                | ServerMessage::ObjectAnimation { .. }
                | ServerMessage::ObjectMoved { .. }
                | ServerMessage::AppearanceChanged { .. }
        ) {
            inbox.0.push(m.0.clone());
        }
    }
}

/// Another player.
#[derive(Component)]
pub struct RemotePlayer {
    pub id: u16,
    pub name: String,
    class: Class,
    /// Position in tiles (fractional while walking).
    pos: Vec2,
    path: VecDeque<(usize, usize)>,
    /// `TargetX/Y`: the destination tile of the last walk (or its own tile on appearing).
    target: (u8, u8),
    facing: f32,
    action: usize,
    frame: f32,
    /// Its weapons and wings, for `SetPlayerStop` / `SetPlayerWalk`.
    gear: crate::character::Gear,
    materials: Vec<Handle<ModelMaterial>>,
    light: Vec3,
    /// What it shows (`ChangeCharacterExt` / `ReceiveChangePlayer`).
    look: Appearance,
    /// `c->SwordCount` (`SetPlayerAttack`).
    sword_count: u32,
    /// `o->Alpha` / `o->AlphaTarget` (teleport, corpse fading) and the alpha its materials have now.
    pub(crate) fade: crate::combat::Fade,
    /// `o->m_bySkillCount`: its Nova stage (combat.rs).
    pub(crate) skill_count: u8,
}

impl RemotePlayer {
    /// Current animation and frame (to pose its parts again, e.g. the highlight).
    pub fn pose_state(&self) -> (usize, f32) {
        (self.action, self.frame)
    }

    /// Its tile position (fractional while walking).
    pub fn position(&self) -> Vec2 {
        self.pos
    }

    pub fn class(&self) -> Class {
        self.class
    }

    /// `SetAction`: a new action from its first frame; it stops walking (combat.rs).
    pub fn set_action(&mut self, a: usize) {
        self.action = a;
        self.frame = 0.0;
        self.path.clear();
    }

    /// `SetAction` as the original does it: an action already playing keeps its frame.
    pub fn keep_or_set_action(&mut self, a: usize) {
        if self.action != a {
            self.set_action(a);
        }
    }

    /// A corpse thrown by Nova / Combo (`DeadCharacter`, combat.rs): `delta` tiles.
    pub(crate) fn push(&mut self, delta: Vec2) {
        self.pos += delta;
    }

    pub fn set_facing(&mut self, facing: f32) {
        self.facing = facing;
    }

    /// MU heading in radians.
    pub fn facing(&self) -> f32 {
        self.facing
    }

    /// `o->Light` (terrain light plus the highlight).
    pub fn light(&self) -> Vec3 {
        self.light.max(Vec3::ZERO)
    }

    /// `SetPlayerStop`: unarmed in a safe zone; outside, flying with wings or by its weapons.
    fn stop_for(&self, safe_zone: bool) -> usize {
        crate::character::stand_action(self.class, &self.gear, safe_zone)
    }

    /// `SetPlayerWalk`, the same way.
    fn walk_for(&self, safe_zone: bool) -> usize {
        crate::character::walk_action_with(self.class, &self.gear, safe_zone)
    }
}

/// Whether the weapons go on the back: in a safe zone or during the greeting..salute emotes
/// (`RenderCharacterBackItem`, ZzzCharacter.cpp:14638-14645).
fn weapons_on_back(action: usize, safe_zone: bool) -> bool {
    use crate::fx::tables::ids;
    safe_zone || (ids::PLAYER_GREETING1 as usize..=ids::PLAYER_SALUTE1 as usize).contains(&action)
}

/// Tiles per second when walking (same as the player).
const WALK_SPEED: f32 = 3.6;

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut inbox: ResMut<Inbox>,
    hero: Res<HeroKey>,
    mut remotes: Query<(Entity, &mut RemotePlayer, &crate::shadow::Shadow, Option<&Children>)>,
    mesh_children: Query<(), With<Mesh3d>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
    data: Res<ItemData>,
    opts: Res<crate::options::GameOptions>,
    (mut fx, mut sound): (ResMut<crate::fx::FxWorld>, ResMut<crate::sound::SoundQueue>),
) {
    // Until the map is loaded.
    let Some(terrain) = terrain else { return };
    for msg in std::mem::take(&mut inbox.0) {
        match msg {
            ServerMessage::AddCharactersToScope(list) => {
                for mut c in list {
                    // `Key >> 15` marks "just appeared"; the id is the low 15 bits.
                    c.id &= game::OBJECT_ID_MASK;
                    if c.id == hero.0 {
                        continue;
                    }
                    // If already present (listed again), it is placed again.
                    if let Some((_, mut r, _, _)) = remotes.iter_mut().find(|(_, r, _, _)| r.id == c.id) {
                        r.pos = Vec2::new(c.x as f32 + 0.5, c.y as f32 + 0.5);
                        r.path.clear();
                        r.target = (c.x, c.y);
                        continue;
                    }
                    let look = c.appearance;
                    let class = Class::from_index(game::base_class_index(look.class));
                    let gear = crate::hero_dress::gear_from_appearance(&look, &data);
                    let safe = crate::character::in_safe_zone(Vec2::new(c.x as f32, c.y as f32), Some(&terrain));
                    let stand = crate::character::stand_action(class, &gear, safe);
                    let dress = crate::hero_dress::dress_from_appearance(&look, &data, opts.render_level);
                    let Some(model) = crate::character::load_character_dressed(&args, class, stand, Some(&dress), &mut meshes, &mut materials, &mut images)
                    else {
                        continue;
                    };
                    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
                    // `Class & 0x07`: 2 sitting, 3 leaning, 4 healing (1, just teleported: below).
                    let pose = match look.pose {
                        2 => Some(game::action::SIT1),
                        3 => Some(game::action::POSE1),
                        4 => Some(game::action::HEALING1),
                        _ => None,
                    };
                    let action = pose.and_then(|at| crate::character::action::for_at(at, class.female()).flatten()).unwrap_or(stand);
                    let mut r = RemotePlayer {
                        id: c.id,
                        name: c.name.clone(),
                        class,
                        pos: Vec2::new(c.x as f32 + 0.5, c.y as f32 + 0.5),
                        path: VecDeque::new(),
                        target: (c.x, c.y),
                        facing: rotation_facing(c.rotation),
                        action,
                        frame: 0.0,
                        gear,
                        materials: model.materials,
                        light: Vec3::splat(-1.0),
                        look,
                        sword_count: 0,
                        fade: Default::default(),
                        skill_count: 0,
                    };
                    // `case 1: CreateTeleportEnd(o)` (MuMain WSclient.cpp:2804-2809): it just teleported.
                    if look.pose == 1 {
                        r.action = crate::fx::tables::ids::PLAYER_SKILL_TELEPORT as usize;
                        r.frame = 5.0;
                        r.fade.teleport_end();
                        let h = terrain.0.height_at(r.pos.x, r.pos.y);
                        let at = Vec3::new(r.pos.x * MU_SCALE, r.pos.y * MU_SCALE, h);
                        crate::combat::teleport_fx(&mut fx, &mut sound, at, r.facing, Vec3::ONE);
                    }
                    info!("aparece {} ({})", r.name, class.name());
                    let transform = remote_transform(&r, &terrain);
                    let mut e = commands.spawn((
                        Name::new(format!("Jugador {}", c.name)),
                        crate::scene::WorldEntity,
                        transform,
                        Visibility::default(),
                        model.skinned,
                        crate::hero_dress::EquipmentGlow(model.glow),
                        shadow,
                        r,
                        crate::names::Selectable::player(c.name.clone(), c.hero_state),
                    ));
                    // Filled by `animate` (not live until its first pose).
                    e.insert(crate::fx::ActorState(crate::fx::Actor { live: false, ..Default::default() }));
                    for child in model.children {
                        e.with_child(child);
                    }
                    // If its destination tile is not the current one, walk there.
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
                let Some((_, mut r, _, _)) = remotes.iter_mut().find(|(_, r, _, _)| r.id == id) else { continue };
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
                let Some((_, mut r, _, _)) = remotes.iter_mut().find(|(_, r, _, _)| r.id == id) else { continue };
                // `Angle[2] = (Angle - 1) * 45`, `Movement = false` and to its `TargetX/Y`.
                r.facing = rotation_facing(direction);
                r.path.clear();
                r.pos = Vec2::new(r.target.0 as f32 + 0.5, r.target.1 as f32 + 0.5);
                // `AT_ATTACK1/2`: `SetPlayerAttack` with its weapons (WSclient.cpp:3148-3157).
                if matches!(action, game::action::ATTACK1 | game::action::ATTACK2) {
                    r.sword_count += 1;
                    r.action = crate::combat::player_attack_action(&r.gear, r.sword_count - 1, 0);
                    r.frame = 0.0;
                    continue;
                }
                match crate::character::action::for_at(action, r.class.female()) {
                    Some(a) => {
                        // `SetAction` does not restart the current action.
                        let a = a.unwrap_or(r.stop_for(crate::character::in_safe_zone(r.pos, Some(&terrain))));
                        if a != r.action {
                            r.action = a;
                            r.frame = 0.0;
                        }
                    }
                    None => debug!("{}: acción {action} sin animación", r.name),
                }
            }
            // `ReceiveMovePosition`: tile and target at once, then the body glides (`JumpTime`).
            ServerMessage::ObjectMoved { id, x, y } => {
                let id = id & game::OBJECT_ID_MASK;
                let Some((e, mut r, _, _)) = remotes.iter_mut().find(|(_, r, _, _)| r.id == id) else { continue };
                let tile = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
                commands.entity(e).insert(crate::character::Jump::new(r.pos - tile));
                r.pos = tile;
                r.target = (x, y);
                r.path.clear();
            }
            ServerMessage::AppearanceChanged { id, slot, item } => {
                let id = id & game::OBJECT_ID_MASK;
                let Some((e, mut r, _, children)) = remotes.iter_mut().find(|(_, r, _, _)| r.id == id) else { continue };
                let before = r.look;
                r.look.apply_change(slot as usize, item);
                if r.look == before {
                    continue;
                }
                // Dressed again: new pieces, the same pose.
                let dress = crate::hero_dress::dress_from_appearance(&r.look, &data, opts.render_level);
                let Some(model) =
                    crate::character::load_character_dressed(&args, r.class, r.action, Some(&dress), &mut meshes, &mut materials, &mut images)
                else {
                    continue;
                };
                for c in children.into_iter().flatten() {
                    if mesh_children.contains(*c) {
                        commands.entity(*c).despawn();
                    }
                }
                // Standing, it takes the stand of its new gear (`SetPlayerStop` runs every tick).
                r.gear = crate::hero_dress::gear_from_appearance(&r.look, &data);
                if crate::character::action::standing(r.action) {
                    r.action = r.stop_for(crate::character::in_safe_zone(r.pos, Some(&terrain)));
                }
                r.materials = model.materials;
                r.light = Vec3::splat(-1.0);
                r.fade.reapply();
                // The highlight shells (names.rs) were children too: they are made again.
                commands.entity(e).remove::<crate::names::EdgeShells>().insert((model.skinned, crate::hero_dress::EquipmentGlow(model.glow)));
                for child in model.children {
                    commands.entity(e).with_child(child);
                }
            }
            ServerMessage::MapObjectOutOfScope(ids) => {
                for (e, r, shadow, _) in &remotes {
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

/// MU angle of the `Rotation` (0..7) the server sends.
fn rotation_facing(rotation: u8) -> f32 {
    crate::character::direction_facing(rotation)
}

fn remote_transform(r: &RemotePlayer, terrain: &LoadedTerrain) -> Transform {
    let h = terrain.0.height_at(r.pos.x, r.pos.y) - r.fade.sink;
    Transform {
        translation: Vec3::new(r.pos.x, h / MU_SCALE, -r.pos.y),
        rotation: Quat::from_rotation_x(-FRAC_PI_2) * Quat::from_rotation_z(r.facing),
        scale: Vec3::splat(r.class.game_scale() / MU_SCALE),
    }
}

fn walk(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut RemotePlayer, &mut Transform, Option<&mut crate::character::Jump>)>,
) {
    let Some(terrain) = terrain else { return };
    for (entity, mut r, mut tf, jump) in &mut q {
        // A new walk ends the glide (it starts from the tile).
        if let Some(mut j) = jump {
            match j.advance(time.delta_secs()).filter(|_| r.path.is_empty()) {
                Some(offset) => {
                    let tile = r.pos;
                    r.pos = tile + offset;
                    *tf = remote_transform(&r, &terrain);
                    r.pos = tile;
                    continue;
                }
                None => {
                    commands.entity(entity).remove::<crate::character::Jump>();
                }
            }
        }
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
        // Sitting, leaning or emoting continues while not walking (like the hero).
        let keep = crate::character::action::resting(r.action)
            || crate::character::action::emote(r.action)
            || crate::combat::is_attack_action(r.action)
            || r.action == crate::fx::tables::ids::PLAYER_SHOCK as usize
            || r.action == crate::fx::tables::ids::PLAYER_DIE1 as usize;
        let safe = crate::character::in_safe_zone(r.pos, Some(&terrain));
        let action = if !r.path.is_empty() {
            r.walk_for(safe)
        } else if keep {
            r.action
        } else {
            r.stop_for(safe)
        };
        if action != r.action {
            r.action = action;
            r.frame = 0.0;
        }
        *tf = remote_transform(&r, &terrain);
    }
}

/// Animation, shadow and the terrain light where it stands.
#[allow(clippy::type_complexity)]
pub(crate) fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut q: Query<(
        &mut RemotePlayer,
        &crate::character::Skinned,
        &crate::shadow::Shadow,
        Option<&crate::names::Selectable>,
        Option<&mut crate::fx::ActorState>,
    )>,
    mut visibility: Query<&mut Visibility, Without<RemotePlayer>>,
) {
    let Some(terrain) = terrain else { return };
    for (mut r, skinned, shadow, selectable, actor) in &mut q {
        use crate::fx::tables::ids::PLAYER_SKILL_HELL_BEGIN;
        let nova_charge = r.action == PLAYER_SKILL_HELL_BEGIN as usize;
        // `CharacterAnimation` halves the Nova charge (MuMain ZzzCharacter.cpp:2483-2486).
        let speed = crate::character::play_speed(r.action) * if nova_charge { 0.5 } else { 1.0 };
        r.frame += speed * crate::fx::sim::TICKS_PER_SECOND * time.delta_secs();
        // An emote ends: standing (`PlayerStopAnimationSetting`). Same for an action `player.bmd`
        // lacks (any number from the server).
        let safe = crate::character::in_safe_zone(r.pos, Some(&terrain));
        let frames = skinned.action_frames(r.action) as f32;
        // Attacks and hits end in a stand too (`PlayerStopAnimationSetting`); the death stays.
        let ends = crate::character::action::emote(r.action) || crate::combat::is_attack_action(r.action) || r.action == crate::fx::tables::ids::PLAYER_SHOCK as usize;
        if r.action == crate::fx::tables::ids::PLAYER_DIE1 as usize {
            r.frame = r.frame.min((frames - 1.0).max(0.0));
        } else if nova_charge && frames > 0.0 {
            // A `Loop` action `PlayerStopAnimationSetting` restarts (ZzzCharacter.cpp:3477, 3485-3488).
            if r.frame >= frames {
                r.frame = 0.0;
            }
        } else if frames == 0.0 || (ends && r.frame >= frames - 1.0) {
            r.action = r.stop_for(safe);
            r.frame = 0.0;
        }
        let ground = terrain.0.height_at(r.pos.x, r.pos.y);
        let mut sh = crate::shadow::Builder::new(
            Some(&terrain.0),
            Vec3::new(r.pos.x * MU_SCALE, r.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(r.facing),
            r.class.game_scale(),
        );
        let links = crate::character::LinkState {
            weapons_on_back: weapons_on_back(r.action, safe),
            safe_zone: safe,
            time: time.elapsed_secs(),
            ..default()
        };
        let bones = skinned.pose(r.action, r.frame, links, &mut meshes, &mut sh);
        if let Some(mut actor) = actor {
            // Snapshot for the effects engine and the client-side hit tests (`CharactersClient[]`).
            actor.0 = crate::fx::Actor {
                ty: crate::fx::core::MODEL_PLAYER,
                kind: crate::fx::core::KIND_PLAYER,
                key: r.id as i32,
                position: Vec3::new(r.pos.x * MU_SCALE, r.pos.y * MU_SCALE, ground),
                angle: Vec3::new(0.0, 0.0, r.facing.to_degrees()),
                scale: r.class.game_scale(),
                bones: std::sync::Arc::new(bones),
                dummy_bones: skinned.dummy_bones(),
                action: r.action,
                frame: r.frame,
                weapon: skinned.fx_weapon(),
                weapon_bone: crate::character::RIGHT_HAND_BONE,
                light: r.light,
                safe_zone: safe,
                skill_count: r.skill_count,
                ..Default::default()
            };
        }
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        // `BodyLight`: the terrain light on its tile plus `o->Light` (`VectorAdd(Light, o->Light,
        // c->Light)`, ZzzCharacter.cpp:8909; set by `SelectCharacter`, see `names.rs`).
        let size = mu_data::terrain::SIZE;
        let (x, y) = ((r.pos.x.max(0.0) as usize).min(size - 1), (r.pos.y.max(0.0) as usize).min(size - 1));
        let light = Vec3::from_array(terrain.0.light[y * size + x]) + selectable.map_or(Vec3::ZERO, |s| s.light);
        if let Some(alpha) = r.fade.take_change() {
            for h in &r.materials {
                if let Some(mut m) = materials.get_mut(h) {
                    crate::model_light::set_alpha(&mut m, alpha);
                }
            }
        }
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
