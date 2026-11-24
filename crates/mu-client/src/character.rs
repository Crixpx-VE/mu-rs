//! Playable character: animated model (`player.bmd` skeleton + armor pieces)
//! that walks around the map when clicking the ground.
//!
//! Animation runs on the CPU: every frame the vertices of each piece are recomputed
//! with the bones of the current pose. Plenty fast for a few characters.

use std::collections::VecDeque;
use std::f32::consts::FRAC_PI_2;
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::texture::AlphaKind;
use mu_data::{fs, path, texture};

use crate::Args;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

/// `player.bmd` animations (the original `PLAYER_*` enum).
pub mod action {
    use crate::fx::tables::ids;
    pub const STOP_MALE: usize = 1;
    pub const WALK_MALE: usize = 15;
    pub const SIT1: usize = ids::PLAYER_SIT1 as usize;
    /// `PLAYER_SIT1..=PLAYER_POSE_FEMALE1`: sitting, healing or leaning (stays that way until moving).
    pub fn resting(a: usize) -> bool {
        (SIT1..=ids::PLAYER_POSE_FEMALE1 as usize).contains(&a)
    }

    /// Emotes (`PLAYER_GREETING1..=PLAYER_RUSH1`): they fall in the `PLAYER_ATTACK_FIST..PLAYER_SHOCK`
    /// range of `PlayerStopAnimationSetting` (ZzzCharacter.cpp:3264-3282), so when the animation ends
    /// the character is standing again (`SetPlayerStop`).
    pub fn emote(a: usize) -> bool {
        (ids::PLAYER_GREETING1 as usize..=ids::PLAYER_RUSH1 as usize).contains(&a)
    }

    /// `ReceiveAction` (WSclient.cpp:3142-3350): the `player.bmd` animation of an `AT_*` action
    /// (female ones, one more; sitting, two). `Some(None)` = `AT_STAND1/2` (`SetPlayerStop`);
    /// `None` = none known yet (attacks and damage, no weapons or monsters yet).
    pub fn for_at(at: u8, female: bool) -> Option<Option<usize>> {
        use mu_protocol::game::action as at_;
        let f = female as i32;
        let a = match at {
            at_::STAND1 | at_::STAND2 => return Some(None),
            at_::ATTACK1 | at_::ATTACK2 | at_::DAMAGE1 => return None,
            at_::SIT1 => ids::PLAYER_SIT1 + 2 * f,
            at_::POSE1 => ids::PLAYER_POSE1 + f,
            at_::HEALING1 => ids::PLAYER_HEALING1 + f,
            at_::GREETING1 => ids::PLAYER_GREETING1 + f,
            at_::GOODBYE1 => ids::PLAYER_GOODBYE1 + f,
            at_::CLAP1 => ids::PLAYER_CLAP1 + f,
            at_::GESTURE1 => ids::PLAYER_GESTURE1 + f,
            at_::DIRECTION1 => ids::PLAYER_DIRECTION1 + f,
            at_::UNKNOWN1 => ids::PLAYER_UNKNOWN1 + f,
            at_::CRY1 => ids::PLAYER_CRY1 + f,
            at_::AWKWARD1 => ids::PLAYER_AWKWARD1 + f,
            at_::SEE1 => ids::PLAYER_SEE1 + f,
            at_::CHEER1 => ids::PLAYER_CHEER1 + f,
            at_::WIN1 => ids::PLAYER_WIN1 + f,
            at_::SMILE1 => ids::PLAYER_SMILE1 + f,
            at_::SLEEP1 => ids::PLAYER_SLEEP1 + f,
            at_::COLD1 => ids::PLAYER_COLD1 + f,
            at_::AGAIN1 => ids::PLAYER_AGAIN1 + f,
            at_::RESPECT1 => ids::PLAYER_RESPECT1,
            at_::SALUTE1 => ids::PLAYER_SALUTE1,
            at_::RUSH1 => ids::PLAYER_RUSH1,
            at_::SCISSORS => ids::PLAYER_SCISSORS,
            at_::ROCK => ids::PLAYER_ROCK,
            at_::PAPER => ids::PLAYER_PAPER,
            at_::HUSTLE => ids::PLAYER_HUSTLE,
            at_::PROVOCATION => ids::PLAYER_PROVOCATION,
            at_::LOOK_AROUND => ids::PLAYER_LOOK_AROUND,
            at_::CHEERS => ids::PLAYER_CHEERS,
            // `default: SetAction(&c->Object, Data->Action)`.
            other => other as i32,
        };
        Some(Some(a as usize))
    }

    /// `SetActionClass` (ZzzInterface.cpp:4416-4432): only applied while standing (`PLAYER_STOP_MALE..
    /// =PLAYER_STOP_RIDE_WEAPON`, `PLAYER_STOP_TWO_HAND_SWORD_TWO` or the Rage Fighter one).
    pub fn standing(a: usize) -> bool {
        let a = a as i32;
        (ids::PLAYER_STOP_MALE..=ids::PLAYER_STOP_RIDE_WEAPON).contains(&a)
            || a == ids::PLAYER_STOP_TWO_HAND_SWORD_TWO
            || a == ids::PLAYER_STOP_RAGEFIGHTER
    }
}

/// `((Angle[2] + 22.5) / 360 * 8 + 1) % 8`: the direction sent by `SendRequestAction` and the
/// single-tile `SendCharacterMove` (wsclientinline.h:454-457), with the angle in [0, 360).
pub fn facing_direction(facing: f32) -> u8 {
    let deg = facing.to_degrees().rem_euclid(360.0);
    (((deg + 22.5) / 360.0 * 8.0 + 1.0) as u32 % 8) as u8
}

/// Tiles per second when walking.
const WALK_SPEED: f32 = 3.6;
/// Animation frames per second.
const ANIM_FPS: f32 = 8.0;

/// `SetPlayerStop` without a weapon (or in a safe zone): the Elf `STOP_FEMALE`, the Summoner
/// `STOP_SUMMONER` and the rest `STOP_MALE`; with the sword in hand outside a safe zone, the Dark
/// Knight `STOP_SWORD`.
pub fn stop_action(class: crate::account::Class, sword_in_hand: bool) -> usize {
    use crate::account::Class;
    use crate::fx::tables::ids;
    match class {
        Class::Knight if sword_in_hand => ids::PLAYER_STOP_SWORD as usize,
        Class::Elf => ids::PLAYER_STOP_FEMALE as usize,
        Class::Summoner => ids::PLAYER_STOP_SUMMONER as usize,
        // MuMain ZzzCharacter.cpp:416.
        Class::Fighter => ids::PLAYER_STOP_RAGEFIGHTER as usize,
        _ => action::STOP_MALE,
    }
}

/// `SetPlayerWalk` without a weapon: `WALK_FEMALE` for women and `WALK_MALE` for the rest.
pub fn walk_action(class: crate::account::Class) -> usize {
    if class.female() { crate::fx::tables::ids::PLAYER_WALK_FEMALE as usize } else { action::WALK_MALE }
}

/// `PlaySpeed` (frames per 25 Hz tick) of the animations used by character selection and the game
/// (ZzzOpenData.cpp ≈330-361; `SetPlayerWalk` sets 0.33 for the walking ones).
pub fn play_speed(a: usize) -> f32 {
    use crate::fx::tables::ids;
    let a = a as i32;
    if a == ids::PLAYER_STOP_SWORD {
        0.26
    } else if a == ids::PLAYER_STOP_SUMMONER {
        0.24
    } else if a == ids::PLAYER_STOP_RAGEFIGHTER {
        // MuMain ZzzOpenData.cpp:243.
        0.16
    } else if (ids::PLAYER_STOP_MALE..=ids::PLAYER_STOP_RIDE_WEAPON).contains(&a) {
        0.28
    } else if (ids::PLAYER_WALK_MALE..=ids::PLAYER_WALK_CROSSBOW).contains(&a) {
        0.33
    } else if a == ids::PLAYER_HEALING1 || a == ids::PLAYER_HEALING_FEMALE1 {
        0.2
    } else if a == ids::PLAYER_SEE1 || a == ids::PLAYER_SEE_FEMALE1 {
        // ZzzOpenData.cpp:358-359 (the rest of the emotes, `PLAYER_DEFENSE1..=PLAYER_SHOCK`, at 0.32).
        0.28
    } else if action::resting(a as usize) {
        0.4
    } else {
        ANIM_FPS / crate::fx::sim::TICKS_PER_SECOND
    }
}

/// Animation speed in frames per second.
fn action_fps(a: usize) -> f32 {
    play_speed(a) * crate::fx::sim::TICKS_PER_SECOND
}

/// The mouse drives the character (to order the systems that read what is under the mouse).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeroInput;

/// The hero walks its path (`MovePath`) and, on arriving, acts (`Action(c, o, false)`).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeroWalk;

pub struct CharacterPlugin;

impl Plugin for CharacterPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CurrentSkill>()
            .add_systems(Update, spawn_player.run_if(crate::scene::load_step(crate::scene::Step::Player)))
            .init_resource::<MouseTimer>()
            .add_systems(
                Update,
                (scripted_walk, server_moves, hero_input.in_set(HeroInput), cast_skill, walk.in_set(HeroWalk), animate, light_body)
                    .chain(),
            );
    }
}

#[derive(Component)]
pub struct Player {
    /// Position in tiles (may be fractional while walking).
    pub pos: Vec2,
    /// Tiles left to walk.
    pub path: VecDeque<(usize, usize)>,
    /// Facing, in radians around the MU vertical axis.
    pub facing: f32,
    pub action: usize,
    pub frame: f32,
    /// Casting a skill: its animation rules until it ends (frames per second).
    pub casting: Option<f32>,
    /// `MOVEMENT_OPERATE`: heading to this sitting object (index in `Operates`).
    pub operate: Option<usize>,
    /// `MOVEMENT_TALK` with `TargetNpc`: heading to the NPC with this scope key (npc.rs).
    pub talk: Option<u16>,
    pub class: crate::account::Class,
    /// Stand and walk animations of its class.
    pub stop: usize,
    pub walk: usize,
}

/// `JumpTime` after `ReceiveMovePosition` (C1 15, WSclient.cpp:1673): the tile changes at once
/// (`PositionX/Y` and `TargetX/Y`), and the body glides there, `Position += (Target - Position) · 0.2`
/// per tick for 16 ticks (`PushingCharacter`, ZzzCharacter.cpp:2933-2972). `offset` is where the body
/// is drawn relative to the tile when the glide starts.
#[derive(Component)]
pub struct Jump {
    offset: Vec2,
    elapsed: f32,
}

/// `JumpTime > 15` ends the glide.
const JUMP_TICKS: f32 = 16.0;
/// `Speed = 0.2f`.
const JUMP_SPEED: f32 = 0.2;

impl Jump {
    pub fn new(offset: Vec2) -> Self {
        Self { offset, elapsed: 0.0 }
    }

    /// Advances the glide by `dt` seconds and returns the drawn offset, or `None` when it is over.
    pub fn advance(&mut self, dt: f32) -> Option<Vec2> {
        self.elapsed += dt;
        (!self.done()).then(|| self.offset_now())
    }

    /// The drawn offset after `elapsed` seconds: the per-tick `× (1 - 0.2)` at any frame rate.
    fn offset_now(&self) -> Vec2 {
        self.offset * (1.0 - JUMP_SPEED).powf(self.elapsed * crate::fx::sim::TICKS_PER_SECOND)
    }

    fn done(&self) -> bool {
        self.elapsed * crate::fx::sim::TICKS_PER_SECOND >= JUMP_TICKS
    }
}

/// `ReceiveMovePosition` for the hero: the server moved us (a teleport, or OpenMU pulling the client
/// back after rejecting a walk, `ResynchronizeClientAsync`). The current walk stops.
fn server_moves(
    mut commands: Commands,
    mut from: MessageReader<crate::net::FromServer>,
    hero_key: Res<crate::remote::HeroKey>,
    mut q: Query<(Entity, &mut Player)>,
) {
    for m in from.read() {
        let mu_protocol::game::ServerMessage::ObjectMoved { id, x, y } = m.0 else { continue };
        if id & mu_protocol::game::OBJECT_ID_MASK != hero_key.0 {
            continue;
        }
        for (e, mut p) in &mut q {
            let tile = Vec2::new(x as f32 + 0.5, y as f32 + 0.5);
            info!("the server moved the hero from ({:.1}, {:.1}) to ({x}, {y})", p.pos.x, p.pos.y);
            commands.entity(e).insert(Jump::new(p.pos - tile));
            p.pos = tile;
            p.path.clear();
            p.operate = None;
            p.talk = None;
        }
    }
}

/// Dark Knight skills on the keys (no server yet: cast in place):
/// (key, `AT_SKILL_*`, `player.bmd` action, `PlaySpeed` speed from `SetAttackSpeed`).
const SKILL_KEYS: [(KeyCode, i32, usize, f32); 10] = {
    use crate::fx::core::*;
    [
        (KeyCode::Digit1, AT_SKILL_TWISTING_SLASH, PLAYER_ATTACK_SKILL_WHEEL as usize, 0.24),
        (KeyCode::Digit2, AT_SKILL_RAGEFUL_BLOW, PLAYER_ATTACK_SKILL_FURY_STRIKE as usize, 0.38),
        (KeyCode::Digit3, AT_SKILL_DEATHSTAB, PLAYER_ATTACK_DEATHSTAB as usize, 0.25),
        (KeyCode::Digit4, AT_SKILL_IMPALE, PLAYER_ATTACK_SKILL_SPEAR as usize, 0.30),
        (KeyCode::Digit5, AT_SKILL_SWELL_LIFE, PLAYER_SKILL_VITALITY as usize, 0.34),
        (KeyCode::Digit6, AT_SKILL_FALLING_SLASH, PLAYER_ATTACK_SKILL_SWORD1 as usize, 0.30),
        (KeyCode::Digit7, AT_SKILL_LUNGE, PLAYER_ATTACK_SKILL_SWORD1 as usize + 1, 0.30),
        (KeyCode::Digit8, AT_SKILL_UPPERCUT, PLAYER_ATTACK_SKILL_SWORD1 as usize + 2, 0.27),
        (KeyCode::Digit9, AT_SKILL_CYCLONE, PLAYER_ATTACK_SKILL_SWORD1 as usize + 3, 0.30),
        (KeyCode::Digit0, AT_SKILL_SLASH, PLAYER_ATTACK_SKILL_SWORD1 as usize + 4, 0.24),
    ]
};

/// Selected skill (`Hero->CurrentSkill`): index in `SKILL_KEYS`.
#[derive(Resource, Default)]
pub struct CurrentSkill(pub Option<usize>);

/// Like the original: `1`-`0` select that hotkey's skill (`UseHotKey`) and the right button
/// casts it toward the mouse; not usable in a safe zone (`CheckAttack`). Without the skill
/// list yet, the hotkeys hold the Dark Knight's in order.
#[allow(clippy::too_many_arguments)]
fn cast_skill(
    keys: Res<ButtonInput<KeyCode>>,
    buttons: Res<ButtonInput<MouseButton>>,
    args: Res<Args>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<IsDefaultUiCamera>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut current: ResMut<CurrentSkill>,
    mut auto: Local<usize>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut players: Query<(Entity, &mut Player)>,
) {
    let Ok((entity, mut p)) = players.single_mut() else { return };
    // The hotkey skills are the Dark Knight's.
    if p.class != crate::account::Class::Knight {
        return;
    }
    if let Some(i) = SKILL_KEYS.iter().position(|(k, ..)| keys.just_pressed(*k)) {
        current.0 = Some(i);
    }
    if p.casting.is_some() {
        return;
    }
    // `--probar-habilidades`: one after another, no mouse, anywhere.
    let (index, target) = if args.skill_test {
        *auto += 1;
        ((*auto - 1) % SKILL_KEYS.len(), None)
    } else {
        let Some(i) = current.0 else { return };
        if !buttons.just_pressed(MouseButton::Right) || over_ui.0 {
            return;
        }
        let safe = terrain.as_ref().is_some_and(|t| {
            t.0.attribute((p.pos.x as usize).min(255), (p.pos.y as usize).min(255)) & mu_data::terrain::attr::SAFE_ZONE != 0
        });
        if safe {
            return;
        }
        // Ground point under the mouse.
        let hit = windows.single().ok().zip(cameras.single().ok()).zip(terrain.as_ref()).and_then(|((w, (cam, tf)), t)| {
            let ray = cam.viewport_to_world(tf, w.cursor_position()?).ok()?;
            raycast_terrain(&t.0, ray)
        });
        (i, hit)
    };
    let (_, skill, action, speed) = SKILL_KEYS[index];
    p.path.clear();
    p.pos = Vec2::new(p.pos.x.floor() + 0.5, p.pos.y.floor() + 0.5);
    // Face the target.
    if let Some(t) = target {
        let d = t - p.pos;
        if d.length() > 1e-3 {
            p.facing = d.x.atan2(-d.y);
        }
    }
    p.action = action;
    p.frame = 0.0;
    // `PlaySpeed` is in original frames per tick (25 per second).
    p.casting = Some(speed * crate::fx::sim::TICKS_PER_SECOND);
    // No monsters yet: the target is the mouse point (or 2 tiles ahead).
    let (s, c) = p.facing.sin_cos();
    let at = target.unwrap_or(Vec2::new(p.pos.x + 2.0 * s, p.pos.y - 2.0 * c));
    world.casts.push(crate::fx::core::Cast::new(entity, skill, None, Vec3::new(at.x * MU_SCALE, at.y * MU_SCALE, 0.0)));
}

/// Skeleton + pieces, shared by the animation.
#[derive(Component)]
pub struct Skinned {
    skeleton: Bmd,
    /// (piece model, mesh index, Bevy mesh being updated)
    parts: Vec<(usize, usize, Handle<Mesh>)>,
    models: Vec<Bmd>,
    /// The right-hand weapon and its meshes.
    weapon: Option<(Bmd, Vec<(usize, Handle<Mesh>)>)>,
}

impl Skinned {
    /// Places the pieces and the weapon (in hand or on the back) in pose `action`/`frame` and adds
    /// their vertices to the shadow. Returns the bones of that pose.
    pub fn pose(
        &self,
        action: usize,
        frame: f32,
        weapon_on_back: bool,
        meshes: &mut Assets<Mesh>,
        sh: &mut crate::shadow::Builder,
    ) -> Vec<bmd::Mat34> {
        let bones = self.skeleton.bone_matrices_at(action, frame);
        for (model, mesh, handle) in &self.parts {
            let posed = self.models[*model].pose_with(*mesh, &bones);
            sh.add(&posed.positions);
            if let Some(mut m) = meshes.get_mut(handle) {
                m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        if let Some((w, handles)) = &self.weapon {
            let wbones = weapon_bones(w, &bones, weapon_on_back);
            for (i, handle) in handles {
                let posed = w.pose_with(*i, &wbones);
                sh.add(&posed.positions);
                if let Some(mut m) = meshes.get_mut(handle) {
                    m.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                    m.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
                }
            }
        }
        bones
    }

    /// Frames of animation `action` of `player.bmd` (0 if missing).
    pub fn action_frames(&self, action: usize) -> usize {
        self.skeleton.actions.get(action).map_or(0, |a| a.frames)
    }

    /// The bones of pose `action`/`frame` (the same ones [`Skinned::pose`] uses).
    pub fn bones_at(&self, action: usize, frame: f32) -> Vec<bmd::Mat34> {
        self.skeleton.bone_matrices_at(action, frame)
    }

    /// The body piece meshes, without the weapon: (piece model, mesh index).
    pub fn body_meshes(&self) -> impl Iterator<Item = (&Bmd, usize)> + '_ {
        self.parts.iter().map(|(model, mesh, _)| (&self.models[*model], *mesh))
    }
}

/// A loaded character: skeleton and pieces, the meshes (children of the character entity)
/// and their materials (to light them).
pub struct CharacterModel {
    pub skinned: Skinned,
    pub children: Vec<(Mesh3d, MeshMaterial3d<crate::model_light::ModelMaterial>)>,
    pub materials: Vec<Handle<crate::model_light::ModelMaterial>>,
    /// Bones of the initial pose.
    pub bones: Vec<bmd::Mat34>,
    /// Equipment glow materials whose color changes over time (hero_dress.rs).
    pub glow: Vec<(Handle<crate::model_light::ModelMaterial>, mu_data::items::glow::GlowLight)>,
}

/// Worn equipment replacing the class pieces (`BodyPart[].Type` and `Weapon[0].Type` with the item
/// model): model path inside `Data` and the item (for its glow). Filled by hero_dress.rs.
#[derive(Default, Clone)]
pub struct Dress {
    /// Helm, armor, pants, gloves and boots (in [`BODY_PARTS`] order).
    pub parts: [Option<(std::path::PathBuf, mu_data::items::Item)>; 5],
    /// The right-hand weapon.
    pub weapon: Option<(std::path::PathBuf, mu_data::items::Item)>,
    /// `g_pOption->GetRenderLevel()`.
    pub render_level: u8,
}

/// Loads `player.bmd` and the class's unequipped pieces (`{Helm,Armor,Pant,Glove,Boot}ClassNN`),
/// in pose `action` (frame 0); the Dark Knight with the sword in hand. `None` if the skeleton
/// is missing.
pub fn load_character(
    args: &Args,
    class: crate::account::Class,
    action: usize,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<crate::model_light::ModelMaterial>,
    images: &mut Assets<Image>,
) -> Option<CharacterModel> {
    load_character_dressed(args, class, action, None, meshes, materials, images)
}

/// Like [`load_character`], with the `dress` equipment (without it, the Dark Knight short sword).
pub fn load_character_dressed(
    args: &Args,
    class: crate::account::Class,
    action: usize,
    dress: Option<&Dress>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<crate::model_light::ModelMaterial>,
    images: &mut Assets<Image>,
) -> Option<CharacterModel> {
    let dir = args.data.join("Player");
    let mut glow = crate::hero_dress::GlowBuilder::default();
    let skeleton = match fs::read(&dir, "player.bmd").and_then(|b| bmd::read(&b)) {
        Ok(s) => s,
        Err(e) => {
            error!("no se pudo cargar player.bmd: {e}");
            return None;
        }
    };

    let bones = skeleton.bone_matrices(action, 0);
    let mut models = vec![];
    let mut parts = vec![];
    let mut children = vec![];
    let mut body_materials = vec![];
    for (k, part) in BODY_PARTS.into_iter().enumerate() {
        let name = &format!("{part}Class{:02}.bmd", class.skin_file());
        let Ok(class_model) = fs::read(&dir, name).and_then(|b| bmd::read(&b)) else {
            warn!("falta la pieza {name}");
            continue;
        };
        // The equipment piece (if any and readable) instead of the class piece.
        let equipped = dress.and_then(|d| d.parts[k].as_ref()).and_then(|(path, item)| {
            let (pdir, file) = (args.data.join(path.parent()?), path.file_name()?.to_str()?);
            Some((fs::read(&pdir, file).and_then(|b| bmd::read(&b)).ok()?, pdir, *item))
        });
        let (model, pdir, item) = match equipped {
            Some((m, pdir, item)) => (m, pdir, Some(item)),
            None => (class_model.clone(), dir.clone(), None),
        };
        let model_index = models.len();
        for i in 0..model.meshes.len() {
            let posed = model.pose_with(i, &bones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs);
            let handle = meshes.add(mesh);
            let texture = crate::hero_dress::part_texture(&model.meshes[i].texture, item.is_some(), &class_model);
            let Some(texture) = texture else { continue };
            let material = materials.add(part_material(&pdir, &texture, images));
            body_materials.push(material.clone());
            children.push((Mesh3d(handle.clone()), MeshMaterial3d(material.clone())));
            if let (Some(item), Some(d)) = (item, dress) {
                children.extend(glow.passes(&args.data, &handle, &model.meshes[i], &item, d.render_level, &material, materials, images));
            }
            parts.push((model_index, i, handle));
        }
        models.push(model);
    }

    // Right-hand weapon (like `RenderLinkObject` with `Link` = true): the equipped one or, without
    // equipment, the Dark Knight short sword.
    let item_dir = args.data.join("Item");
    let knight = class == crate::account::Class::Knight;
    let dressed_weapon = dress.and_then(|d| d.weapon.as_ref()).and_then(|(path, item)| {
        let (wdir, file) = (args.data.join(path.parent()?), path.file_name()?.to_str()?);
        Some((fs::read(&wdir, file).and_then(|b| bmd::read(&b)).ok()?, wdir, *item))
    });
    let default_weapon = (knight && dress.is_none())
        .then(|| fs::read(&item_dir, WEAPON_FILE).and_then(|b| bmd::read(&b)).ok())
        .flatten()
        .map(|w| (w, item_dir.clone(), None));
    let weapon_item = dressed_weapon.as_ref().map(|w| w.2);
    let weapon_dir = dressed_weapon.as_ref().map_or(item_dir.clone(), |w| w.1.clone());
    let weapon = dressed_weapon.map(|(w, d, i)| (w, d, Some(i))).or(default_weapon).map(|(w, _, _)| {
        let wbones = weapon_bones(&w, &bones, false);
        let mut handles = vec![];
        for i in 0..w.meshes.len() {
            let posed = w.pose_with(i, &wbones);
            if posed.positions.is_empty() {
                continue;
            }
            let mesh = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
            );
            let material = materials.add(part_material(&weapon_dir, &w.meshes[i].texture, images));
            body_materials.push(material.clone());
            children.push((Mesh3d(mesh.clone()), MeshMaterial3d(material.clone())));
            if let (Some(item), Some(d)) = (weapon_item, dress) {
                children.extend(glow.passes(&args.data, &mesh, &w.meshes[i], &item, d.render_level, &material, materials, images));
            }
            handles.push((i, mesh));
        }
        (w, handles)
    });
    if knight && dress.is_none() && weapon.is_none() {
        warn!("falta el arma Item/{WEAPON_FILE}");
    }
    Some(CharacterModel {
        skinned: Skinned { skeleton, parts, models, weapon },
        children,
        materials: body_materials,
        bones,
        glow: glow.animated,
    })
}

/// Unequipped body pieces (`BodyPart[BODYPART_HELM..BOOTS]`).
const BODY_PARTS: [&str; 5] = ["Helm", "Armor", "Pant", "Glove", "Boot"];

fn spawn_player(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut stats: ResMut<crate::mainframe::CharacterStats>,
) {
    let class = hero.map_or(crate::account::Class::Knight, |h| {
        info!("entra {} ({} nivel {})", h.name, h.class.name(), h.level);
        h.class
    });
    let (stop, walk) = (stop_action(class, false), walk_action(class));
    let Some(CharacterModel { skinned, children, materials: body_materials, bones, .. }) =
        load_character(&args, class, stop, &mut meshes, &mut materials, &mut images)
    else {
        return;
    };

    // With a server, where it says (`CharacterInformation`); otherwise `--spawn` or the map center.
    let from_server = info.as_ref().map(|i| [i.0.x as f32 + 0.5, i.0.y as f32 + 0.5]);
    let start = from_server.or(args.spawn).unwrap_or(if args.world == 1 { [133.0, 127.0] } else { [128.0, 128.0] });
    let mut pos = Vec2::from_array(start);
    if let Some(t) = &terrain {
        pos = nearest_walkable(&t.0, pos).unwrap_or(pos);
    }
    // `Direction` (0..7, like walking steps) → MU angle.
    let facing = info.as_ref().map_or(0.0, |i| direction_facing(i.0.direction));
    if let Some(i) = &info {
        let i = &i.0;
        *stats = crate::mainframe::CharacterStats {
            hp: i.health as u32,
            hp_max: i.max_health as u32,
            mp: i.mana as u32,
            mp_max: i.max_mana as u32,
            ag: i.ability as u32,
            ag_max: i.max_ability as u32,
            sd: i.shield as u32,
            sd_max: i.max_shield as u32,
            ..default()
        };
    }

    let player = Player {
        pos,
        path: VecDeque::new(),
        facing,
        action: stop,
        frame: 0.0,
        casting: None,
        operate: None,
        talk: None,
        class,
        stop,
        walk,
    };
    let actor = actor_state(&player, &bones, terrain.as_deref());
    let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
    let mut e = commands.spawn((
        Name::new("Jugador"),
        crate::scene::WorldEntity,
        player_transform(&player, terrain.as_deref()),
        Visibility::default(),
        player,
        skinned,
        BodyMaterials(body_materials),
        shadow,
        actor,
    ));
    for child in children {
        e.with_child(child);
    }
}

/// Initial Dark Knight weapon: the short sword (`MODEL_SWORD + 1` = `Item/Sword02.bmd`).
const WEAPON_FILE: &str = "Sword02.bmd";
const WEAPON_MODEL: i32 = crate::fx::core::MODEL_SWORD + 1;
/// `Weapon[0].LinkBone`: the right hand of `player.bmd`.
pub const RIGHT_HAND_BONE: usize = 33;

/// Bones of the weapon in hand: a weapon in hand is placed with `Link = false` (`PlanAttachments`,
/// ZzzCharacter.cpp ≈14257), i.e. directly with the hand bone matrix. (The
/// (70°, 0, 90°) rotation offset by (-20, 5, 40) of `PlaceLinkedPart` is for weapons on the back.)
///
/// In a safe zone the original carries it on the back (`AreWeaponsOnBack`): bone 47 with `Link`
/// (`PlaceLinkedPart`: `AngleMatrix(70°, 0, 90°)` offset by (-20, 5, 40) for swords).
fn weapon_bones(weapon: &Bmd, player_bones: &[bmd::Mat34], on_back: bool) -> Vec<bmd::Mat34> {
    let local = weapon.bone_matrices(0, 0);
    let parent = if on_back {
        let Some(back) = player_bones.get(BACK_BONE) else { return local };
        let mut m = bmd::angle_matrix([70f32.to_radians(), 0.0, 90f32.to_radians()]);
        m[0][3] = -20.0;
        m[1][3] = 5.0;
        m[2][3] = 40.0;
        bmd::concat(back, &m)
    } else {
        let Some(hand) = player_bones.get(RIGHT_HAND_BONE) else { return local };
        *hand
    };
    local.iter().map(|b| bmd::concat(&parent, b)).collect()
}

/// `CreateWeaponBlur` for the Dark Knight with a sword: in sword skills (SWORD1-5) from
/// frame 3 on, the trail goes from (0, -20, 0) to (0, -120, 0) of the weapon bone
/// (`BlurType` 1), with `motion_blur_r` (`BlurMapping` 2), white and short. Every frame the pose is
/// recomputed 10 times along the last stretch of animation (`PlaySpeed`) so the trail is continuous.
fn weapon_blur(entity: Entity, p: &Player, s: &Skinned, terrain: Option<&LoadedTerrain>, world: &mut crate::fx::FxWorld, frame: u64) {
    use crate::fx::core::PLAYER_ATTACK_SKILL_SWORD1;
    let (Some(fps), Some(_)) = (p.casting, &s.weapon) else { return };
    let sword = PLAYER_ATTACK_SKILL_SWORD1 as usize;
    if !(sword..sword + 5).contains(&p.action) || p.frame < 3.0 {
        return;
    }
    let play_speed = fps / crate::fx::sim::TICKS_PER_SECOND;
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let origin = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h);
    let rot = Quat::from_rotation_z(p.facing);
    const INTER: usize = 10;
    let mut f = p.frame - play_speed;
    for _ in 0..INTER {
        let bones = s.skeleton.bone_matrices_at(p.action, f);
        if let Some(m) = bones.get(RIGHT_HAND_BONE) {
            let a = Vec3::from_array(bmd::transform_point(m, [0.0, -20.0, 0.0]));
            let b = Vec3::from_array(bmd::transform_point(m, [0.0, -120.0, 0.0]));
            let k = p.class.game_scale();
            world.create_blur(entity, origin + rot * (a * k), origin + rot * (b * k), Vec3::ONE, 2, true, frame);
        }
        f += play_speed / INTER as f32;
    }
}

/// Back bone where weapons go in a safe zone (`backBone` of `PlanAttachments`).
const BACK_BONE: usize = 47;

/// Does the weapon go on the back? In a safe zone, except while casting a skill (the original
/// does not allow them in a safe zone; here it is allowed so they can be tested in town).
fn weapon_on_back(p: &Player, terrain: Option<&LoadedTerrain>) -> bool {
    let safe = terrain.is_some_and(|t| {
        let (x, y) = (p.pos.x as usize, p.pos.y as usize);
        t.0.attribute(x.min(255), y.min(255)) & mu_data::terrain::attr::SAFE_ZONE != 0
    });
    safe && p.casting.is_none()
}

/// Character snapshot for the effects engine (MU position, rotation, bones, weapon).
fn actor_state(p: &Player, bones: &[bmd::Mat34], terrain: Option<&LoadedTerrain>) -> crate::fx::ActorState {
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    crate::fx::ActorState(crate::fx::Actor {
        position: Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, h),
        angle: Vec3::new(0.0, 0.0, p.facing.to_degrees()),
        scale: p.class.game_scale(),
        bones: std::sync::Arc::new(bones.to_vec()),
        action: p.action,
        frame: p.frame,
        weapon: Some((WEAPON_MODEL, 0)),
        weapon_bone: RIGHT_HAND_BONE,
        live: true,
        pos_sword: Vec3::ZERO,
    })
}

/// `--walk-to`: walk order without the mouse. In capture mode it is given 1 s before capturing,
/// so the shot catches the character walking.
fn scripted_walk(
    args: Res<Args>,
    time: Res<Time<Real>>,
    terrain: Option<Res<LoadedTerrain>>,
    mut done: Local<bool>,
    mut players: Query<&mut Player>,
) {
    let (false, Some(target), Some(t), Ok(mut p)) = (*done, args.walk_to, terrain, players.single_mut()) else {
        return;
    };
    if args.screenshot.is_some() && time.elapsed_secs() < args.screenshot_delay - 1.0 {
        return;
    }
    *done = true;
    let from = (p.pos.x as usize, p.pos.y as usize);
    match path::find_path(from, (target[0] as usize, target[1] as usize), |x, y| t.0.walkable(x, y), 20_000) {
        Some(path) => p.path = path.into_iter().skip(1).collect(),
        None => warn!("--walk-to: no hay camino hasta {target:?}"),
    }
}

pub(crate) fn part_material(dir: &Path, texture_name: &str, images: &mut Assets<Image>) -> crate::model_light::ModelMaterial {
    let tex = texture::load(dir, &bmd::texture_file_name(texture_name)).ok();
    // Like objects: alpha test and depth, as in the original.
    let alpha_mode = match tex.as_ref().map(|t| t.alpha_kind()) {
        Some(AlphaKind::Cutout | AlphaKind::Blend) => AlphaMode::Mask(crate::objects::ALPHA_REF),
        _ => AlphaMode::Opaque,
    };
    let base = StandardMaterial {
        base_color_texture: tex.map(|t| images.add(to_bevy_image(&t))),
        unlit: true,
        cull_mode: None,
        alpha_mode,
        ..default()
    };
    crate::model_light::model_material(base, crate::model_light::ModelLight::lit([1.0; 3]))
}

/// Body and weapon materials, to give them the light of the character's location.
#[derive(Component)]
pub(crate) struct BodyMaterials(pub(crate) Vec<Handle<crate::model_light::ModelMaterial>>);

/// The hero's in-game `c->Light`: `AbilityLight` = (1, 1, 1), without the terrain light
/// (ZzzCharacter.cpp:8967-8993; the skills that tint it do not exist yet). Other
/// characters do get the terrain light (remote.rs).
fn light_body(
    players: Query<&BodyMaterials, With<Player>>,
    mut materials: ResMut<Assets<crate::model_light::ModelMaterial>>,
    mut last: Local<Vec4>,
) {
    let Ok(body) = players.single() else { return };
    let light = Vec4::ONE;
    if light == *last {
        return;
    }
    *last = light;
    for h in &body.0 {
        if let Some(mut m) = materials.get_mut(h) {
            m.extension.light = light;
        }
    }
}

/// The original `MouseUpdateTime` / `MouseUpdateTimeMax` / `MouseLButtonPush`, in 25 Hz ticks.
/// A resource because `Action` also sets `MouseUpdateTimeMax` (npc.rs).
#[derive(Resource, Default)]
pub(crate) struct MouseTimer {
    time: u32,
    pub(crate) max: u32,
    /// `MouseLButtonPush`: unhandled click (lost when the button is released).
    push: bool,
    /// The button was pressed in the game area (not over the bar or a window).
    held: bool,
    /// Accumulated tick fraction.
    acc: f32,
}

/// `PathFinding2` → `GetPath`: the path is cut to `MAX_PATH_FIND` (15) tiles counting the starting
/// one; with the button held it is requested again and continues.
const MAX_PATH_FIND: usize = 15;

/// Path from `from` to `to` (including the start tile), if one exists and has more than one tile.
fn hero_path(t: &mu_data::terrain::Terrain, from: (usize, usize), to: (usize, usize)) -> Option<Vec<(usize, usize)>> {
    let mut p = path::find_path(from, to, |x, y| t.walkable(x, y), 20_000)?;
    p.truncate(MAX_PATH_FIND);
    (p.len() > 1).then_some(p)
}

/// `SendMove`: the time until another click is handled depends on the path length; the path goes to
/// the server (`SendCharacterMove`: start tile, direction and one step per tile).
fn send_move(
    player: &mut Player,
    path: Vec<(usize, usize)>,
    timer: &mut MouseTimer,
    net: &mut crate::net::Net,
    npc: &mut crate::npc::NpcTalk,
) {
    // `IsImpossibleSendMoveInterface` (ZzzInterface.cpp:3751-3754): not with the Chaos Machine open.
    if npc.dialog.blocks_move() {
        return;
    }
    timer.max = match path.len() {
        0..=2 => 0,
        3 => 5,
        n => 10 + (n as u32 - 2) * 3,
    };
    if net.connected() {
        let tiles: Vec<(u8, u8)> = path.iter().map(|&(x, y)| (x.min(255) as u8, y.min(255) as u8)).collect();
        let directions: Vec<u8> =
            tiles.windows(2).filter_map(|w| mu_protocol::game::direction_between(w[0], w[1])).collect();
        if let (Some(&(sx, sy)), Some(&first)) = (tiles.first(), directions.first()) {
            net.send(mu_protocol::game::ClientMessage::Walk { source_x: sx, source_y: sy, rotation: first, directions });
        }
    }
    player.path = path.into_iter().skip(1).collect();
    // `UpdateSendMoveInterface` (ZzzInterface.cpp:3786): walking closes the NPC windows.
    npc.after_send_move(net);
}

/// MU angle (that of `Player::facing`) of a step direction (0..7).
pub fn direction_facing(direction: u8) -> f32 {
    let (dx, dy) = mu_protocol::game::DIRECTIONS[(direction & 7) as usize];
    (dx as f32).atan2(-(dy as f32))
}

/// The mouse in game (`MoveHero`, ZzzInterface.cpp ≈7340-7636), once per tick:
/// - Each time `MouseUpdateTime` reaches `MouseUpdateTimeMax`, the click (or the held button)
///   sends the character toward the mouse again: holding the button makes it follow
///   the mouse.
/// - Over a sitting object (and standing still), it walks to it (`MOVEMENT_OPERATE`).
/// - Over an NPC, it walks next to it and talks on arriving (`MOVEMENT_TALK`, npc.rs).
/// - On the ground, `DontMove` (cursor `CursorDontMove`) if the tile has no ground, is water...
///   (attribute ≥ `TW_NOGROUND` without `TW_ACTION` or `TW_HEIGHT`).
fn hero_input(
    time: Res<Time>,
    buttons: Res<ButtonInput<MouseButton>>,
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<IsDefaultUiCamera>>,
    terrain: Option<Res<LoadedTerrain>>,
    over_ui: Res<crate::mainframe::PointerOverUi>,
    operates: Res<crate::operate::Operates>,
    mut pointer: ResMut<crate::operate::Pointer>,
    mut timer: ResMut<MouseTimer>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut players: Query<&mut Player>,
    mut net: ResMut<crate::net::Net>,
    gate: Res<crate::gate::GateState>,
    mut npc: crate::npc::NpcTalk,
) {
    use mu_data::terrain::attr;
    let (Some(t), Ok(mut player)) = (terrain, players.single_mut()) else { return };
    if buttons.just_pressed(MouseButton::Left) && !over_ui.0 {
        timer.push = true;
        timer.held = true;
    }
    if !buttons.pressed(MouseButton::Left) {
        timer.push = false;
        timer.held = false;
    }
    let ray = windows.single().ok().zip(cameras.single().ok()).and_then(|(w, (camera, tf))| {
        let ray = camera.viewport_to_world(tf, w.cursor_position()?).ok()?;
        Some(ray)
    });

    timer.acc += time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND;
    // After a stall, no more than a few ticks are caught up.
    timer.acc = timer.acc.min(3.0);
    while timer.acc >= 1.0 {
        timer.acc -= 1.0;
        let mut success = false;
        if timer.time >= timer.max {
            if timer.push {
                timer.push = false;
                success = true;
            }
            if timer.held {
                success = true;
            }
        }
        // Not over the UI (`MouseOnWindow`), nor while casting a skill, nor while crossing a
        // gate (`LoadingWorld > 30`, ZzzInterface.cpp:7192-7195).
        if success && !over_ui.0 && player.casting.is_none() && !gate.blocks_input() {
            timer.time = 0;
            let here = (player.pos.x as usize, player.pos.y as usize);
            let moving = !player.path.is_empty();
            if let Some((i, op)) = pointer.operate.and_then(|i| operates.0.get(i).map(|o| (i, o))) {
                // `wall == TW_HEIGHT || wall < TW_CHARACTER`, and only while standing still.
                let (tx, ty) = op.tile();
                let wall = t.0.attribute(tx.max(0) as usize, ty.max(0) as usize);
                if (wall == attr::HEIGHT || wall < attr::CHARACTER) && !moving {
                    player.operate = Some(i);
                    player.talk = None;
                    match hero_path(&t.0, here, (tx as usize, ty as usize)) {
                        Some(p) => send_move(&mut player, p, &mut timer, &mut net, &mut npc),
                        None => operate_action(&mut player, op, &mut sound, &mut net),
                    }
                }
            } else if let Some((key, tile)) = npc.clicked_npc() {
                // `SelectedNpc` (ZzzInterface.cpp:7528-7571): `MOVEMENT_TALK` with `TargetNpc`, walking
                // toward the NPC's tile (`PathFinding2` ends next to it) or talking right away if
                // there is no path (`Action(c, o, true)`). Not yet: the hero's `eBuff_CrywolfNPCHide`
                // (keeps the movement type, 7533) and the 550-unit reach of the Kanturu entrance NPC
                // (`MODEL_KANTURU2ND_ENTER_NPC`, 7543-7563).
                // (`Action` sets `TargetNpc = -1`, 3584: a talk done now leaves nothing pending.)
                player.operate = None;
                match npc.path_to(&t.0, here, tile) {
                    Some(p) => {
                        player.talk = Some(key);
                        send_move(&mut player, p, &mut timer, &mut net, &mut npc);
                    }
                    None => {
                        player.talk = None;
                        npc.act(key);
                    }
                }
            } else if let Some(hit) = ray.and_then(|r| raycast_terrain(&t.0, r)) {
                let to = (hit.x.max(0.0) as usize, hit.y.max(0.0) as usize);
                let a = t.0.attribute(to.0, to.1);
                pointer.dont_move = a >= attr::NO_GROUND && a & attr::ACTION == 0 && a & attr::HEIGHT == 0;
                if let Some(p) = (here != to || !moving).then(|| hero_path(&t.0, here, to)).flatten() {
                    player.operate = None;
                    player.talk = None;
                    send_move(&mut player, p, &mut timer, &mut net, &mut npc);
                }
            }
        }
        timer.time += 1;
    }
}

/// `Action` with `MOVEMENT_OPERATE` (ZzzInterface.cpp:3587-3737): within one tile of the object
/// it sits, leans or heals (women with their own animations), facing the object's direction if
/// needed, and tells the server (`SendRequestAction`) so others see it. Except when healing,
/// it first reports being on the object's tile (single-tile `SendCharacterMove`, no steps) and
/// `SOUND_DROP_ITEM01` plays.
fn operate_action(p: &mut Player, op: &crate::operate::Operate, sound: &mut crate::sound::SoundQueue, net: &mut crate::net::Net) {
    use crate::operate::Act;
    use mu_protocol::game::{ClientMessage, action as at};
    let (tx, ty) = op.tile();
    let (hx, hy) = (p.pos.x as i32, p.pos.y as i32);
    if (hx - tx).abs().max((hy - ty).abs()) > 1 {
        return;
    }
    if op.rule.face && op.rule.act != Act::Nothing {
        p.facing = op.angle.to_radians();
    }
    let at = match op.rule.act {
        Act::Sit => Some(at::SIT1),
        Act::Pose => Some(at::POSE1),
        Act::Healing => Some(at::HEALING1),
        Act::Nothing => None,
    };
    let dir = facing_direction(p.facing);
    if op.rule.act != Act::Healing && net.connected() {
        let (x, y) = (tx.clamp(0, 255) as u8, ty.clamp(0, 255) as u8);
        net.send(ClientMessage::Walk { source_x: x, source_y: y, rotation: dir, directions: Vec::new() });
    }
    if let Some(at) = at {
        if let Some(Some(a)) = action::for_at(at, p.class.female()) {
            p.action = a;
            p.frame = 0.0;
        }
        if net.connected() {
            net.send(ClientMessage::Animation { rotation: dir, action: at });
        }
    }
    if op.rule.act != Act::Healing {
        p.path.clear();
        sound.play_object("SOUND_DROP_ITEM01");
    }
}

/// A chat emote (`SetActionClass` + `SendRequestAction` from `CheckChatText`,
/// ZzzInterface.cpp:4416-4547): the animation only while standing, and the server notice twice
/// if it is applied (once in `SetActionClass` and once after, like the original) or once if not.
pub fn chat_emote(p: &mut Player, at: u8, net: &mut crate::net::Net) {
    use mu_protocol::game::ClientMessage;
    let dir = facing_direction(p.facing);
    let connected = net.connected();
    if action::standing(p.action) && p.path.is_empty() && p.casting.is_none() {
        if let Some(Some(a)) = action::for_at(at, p.class.female()) {
            p.action = a;
            p.frame = 0.0;
        }
        if connected {
            net.send(ClientMessage::Animation { rotation: dir, action: at });
        }
    }
    if connected {
        net.send(ClientMessage::Animation { rotation: dir, action: at });
    }
}

/// Terrain point (in MU tiles) under a camera ray.
fn raycast_terrain(t: &mu_data::terrain::Terrain, ray: Ray3d) -> Option<Vec2> {
    let ground = |p: Vec3| t.height_at(p.x, -p.z) / MU_SCALE;
    let mut prev = 0.0;
    let mut d = 0.0;
    while d < 400.0 {
        let p = ray.get_point(d);
        if p.y <= ground(p) {
            // Refine between the previous step and this one.
            let (mut a, mut b) = (prev, d);
            for _ in 0..12 {
                let m = (a + b) / 2.0;
                if ray.get_point(m).y <= ground(ray.get_point(m)) { b = m } else { a = m }
            }
            let hit = ray.get_point(b);
            return Some(Vec2::new(hit.x, -hit.z));
        }
        prev = d;
        d += 0.25;
    }
    None
}

fn walk(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    operates: Res<crate::operate::Operates>,
    mut sound: ResMut<crate::sound::SoundQueue>,
    mut net: ResMut<crate::net::Net>,
    mut talk: ResMut<crate::npc::TalkRequest>,
    mut commands: Commands,
    mut q: Query<(Entity, &mut Player, &mut Transform, Option<&mut Jump>)>,
) {
    for (entity, mut p, mut tf, mut jump) in &mut q {
        // A new walk ends the glide: the walk starts from the tile.
        if jump.is_some() && !p.path.is_empty() {
            commands.entity(entity).remove::<Jump>();
            jump = None;
        }
        let had_path = !p.path.is_empty();
        let mut budget = WALK_SPEED * time.delta_secs();
        while budget > 0.0 {
            let Some(&(nx, ny)) = p.path.front() else { break };
            let target = Vec2::new(nx as f32 + 0.5, ny as f32 + 0.5);
            let delta = target - p.pos;
            let dist = delta.length();
            if dist > 1e-4 {
                // MU: angle 0 = facing -Y; grows counterclockwise.
                p.facing = delta.x.atan2(-delta.y);
            }
            if dist <= budget {
                p.pos = target;
                budget -= dist;
                p.path.pop_front();
            } else {
                p.pos += delta / dist * budget;
                budget = 0.0;
            }
        }
        // Sitting or leaning stays until walking again; an emote, until it ends (`animate`).
        let action = if !p.path.is_empty() {
            p.walk
        } else if action::resting(p.action) || action::emote(p.action) {
            p.action
        } else {
            p.stop
        };
        if p.casting.is_none() && action != p.action {
            p.action = action;
            p.frame = 0.0;
        }
        // Reached the sitting object (`MovePath` ends → `Action`).
        if had_path && p.path.is_empty() && p.casting.is_none() {
            if let Some(op) = p.operate.take().and_then(|i| operates.0.get(i)) {
                operate_action(&mut p, op, &mut sound, &mut net);
            }
            // `MOVEMENT_TALK` (ZzzInterface.cpp:7282-7295 → 3397). TODO: an elf with a bow and no
            // arrows only stops (`!CheckArrow()`, 7292-7293); equipment is not checked here yet.
            if let Some(key) = p.talk.take() {
                talk.0 = Some(key);
            }
        }
        *tf = player_transform(&p, terrain.as_deref());
        if let Some(j) = jump.as_deref_mut() {
            match j.advance(time.delta_secs()) {
                None => {
                    commands.entity(entity).remove::<Jump>();
                }
                Some(offset) => {
                    // Drawn where the glide is (the terrain height follows, `RequestTerrainHeight`).
                    let tile = p.pos;
                    p.pos = tile + offset;
                    *tf = player_transform(&p, terrain.as_deref());
                    p.pos = tile;
                }
            }
        }
    }
}

fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut world: ResMut<crate::fx::FxWorld>,
    mut render_frame: Local<u64>,
    mut q: Query<(Entity, &mut Player, &Skinned, &mut crate::fx::ActorState, &crate::shadow::Shadow)>,
    mut visibility: Query<&mut Visibility, Without<Player>>,
    args: Res<Args>,
    mut sound: ResMut<crate::sound::SoundQueue>,
) {
    *render_frame += 1;
    for (entity, mut p, s, mut actor, shadow) in &mut q {
        weapon_blur(entity, &p, s, terrain.as_deref(), &mut world, *render_frame);
        let before = p.frame;
        // `SetPlayerWalk`: the Rage Fighter walks at 0.32 instead of 0.33.
        let fps = if p.class == crate::account::Class::Fighter && p.action == p.walk {
            0.32 * crate::fx::sim::TICKS_PER_SECOND
        } else {
            action_fps(p.action)
        };
        p.frame += p.casting.unwrap_or(fps) * time.delta_secs();
        if p.action == p.walk {
            footsteps(&p, before, s, terrain.as_deref(), args.world as i32 - 1, &mut sound);
        }
        // The skill ends when its animation ends: back to standing. So does an emote
        // (`PlayerStopAnimationSetting`).
        if p.casting.is_some() || action::emote(p.action) {
            let frames = s.skeleton.actions.get(p.action).map_or(1, |a| a.frames) as f32;
            if p.frame >= frames - 1.0 {
                p.casting = None;
                p.action = p.stop;
                p.frame = 0.0;
            }
        }
        let ground = terrain.as_ref().map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
        let mut sh = crate::shadow::Builder::new(
            terrain.as_deref().map(|t| &t.0),
            Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, ground),
            Quat::from_rotation_z(p.facing),
            p.class.game_scale(),
        );
        let bones = s.pose(p.action, p.frame, weapon_on_back(&p, terrain.as_deref()), &mut meshes, &mut sh);
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());
        *actor = actor_state(&p, &bones, terrain.as_deref());
    }
}

/// Footsteps (`PlayWalkSound`): when passing frames 1.5 and 4.5 of the walk animation.
fn footsteps(p: &Player, before: f32, s: &Skinned, terrain: Option<&LoadedTerrain>, world: i32, sound: &mut crate::sound::SoundQueue) {
    let Some(act) = s.skeleton.actions.get(p.action) else { return };
    // Walk animations repeat the first frame at the end.
    let n = if act.lock_positions.is_some() { act.frames.saturating_sub(1).max(1) } else { act.frames.max(1) } as f32;
    let (a, b) = (before.rem_euclid(n), p.frame.rem_euclid(n));
    let crossed = |k: f32| if b >= a { a < k && b >= k } else { a < k || b >= k };
    if !(crossed(1.5) || crossed(4.5)) {
        return;
    }
    let Some(t) = terrain else { return };
    let size = mu_data::terrain::SIZE;
    let (x, y) = ((p.pos.x.max(0.0) as usize).min(size - 1), (p.pos.y.max(0.0) as usize).min(size - 1));
    let tile = t.0.mapping.layer1[y * size + x];
    let safe = t.0.attribute(x, y) & mu_data::terrain::attr::SAFE_ZONE != 0;
    if let Some(id) = crate::sound::footstep(world, tile, safe) {
        sound.play(&id);
    }
}

/// Character transform: position on the terrain and rotation toward its facing.
pub fn player_transform(p: &Player, terrain: Option<&LoadedTerrain>) -> Transform {
    let h = terrain.map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let to_bevy = Quat::from_rotation_x(-FRAC_PI_2);
    Transform {
        translation: Vec3::new(p.pos.x, h / MU_SCALE, -p.pos.y),
        rotation: to_bevy * Quat::from_rotation_z(p.facing),
        scale: Vec3::splat(p.class.game_scale() / MU_SCALE),
    }
}

fn nearest_walkable(t: &mu_data::terrain::Terrain, p: Vec2) -> Option<Vec2> {
    let (cx, cy) = (p.x as i32, p.y as i32);
    for r in 0..20i32 {
        for dy in -r..=r {
            for dx in -r..=r {
                let (x, y) = (cx + dx, cy + dy);
                if x >= 0 && y >= 0 && t.walkable(x as usize, y as usize) {
                    return Some(Vec2::new(x as f32 + 0.5, y as f32 + 0.5));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PushingCharacter`: 20% of the remaining distance per tick, over after 16 ticks.
    #[test]
    fn server_move_glide() {
        let tick = 1.0 / crate::fx::sim::TICKS_PER_SECOND;
        let mut j = Jump::new(Vec2::new(10.0, 0.0));
        let first = j.advance(tick).unwrap();
        assert!((first.x - 8.0).abs() < 1e-4, "{first}");
        // The same at 60 fps: two half-ticks equal one tick.
        let mut k = Jump::new(Vec2::new(10.0, 0.0));
        k.advance(tick / 2.0);
        assert!((k.advance(tick / 2.0).unwrap().x - 8.0).abs() < 1e-4);
        for _ in 1..15 {
            assert!(j.advance(tick).is_some());
        }
        assert!(j.advance(tick).is_none(), "JumpTime > 15 ends it");
    }
    use crate::fx::tables::ids;
    use mu_protocol::game::action as at;

    #[test]
    fn direccion_de_ida_y_vuelta() {
        // The direction sent is the one the receiver turns by `(direction - 1) * 45`.
        for d in 0..8u8 {
            assert_eq!(facing_direction(direction_facing(d)), d);
            // Up to 22.5° either way it is still the same one.
            assert_eq!(facing_direction(direction_facing(d) + 0.39), d);
            assert_eq!(facing_direction(direction_facing(d) - 0.39), d);
        }
        assert_eq!(facing_direction(0.0), 1);
    }

    #[test]
    fn acciones_recibidas() {
        let a = |at, female| action::for_at(at, female).flatten().map(|a| a as i32);
        assert_eq!(a(at::SIT1, false), Some(ids::PLAYER_SIT1));
        assert_eq!(a(at::SIT1, true), Some(ids::PLAYER_SIT_FEMALE1));
        assert_eq!(a(at::POSE1, true), Some(ids::PLAYER_POSE_FEMALE1));
        assert_eq!(a(at::HEALING1, true), Some(ids::PLAYER_HEALING_FEMALE1));
        assert_eq!(a(at::GREETING1, true), Some(ids::PLAYER_GREETING_FEMALE1));
        assert_eq!(a(at::GESTURE1, false), Some(ids::PLAYER_GESTURE1));
        assert_eq!(a(at::DIRECTION1, false), Some(ids::PLAYER_DIRECTION1));
        assert_eq!(a(at::RESPECT1, true), Some(ids::PLAYER_RESPECT1), "sin versión de mujer");
        assert_eq!(a(at::CHEERS, true), Some(ids::PLAYER_CHEERS));
        assert_eq!(action::for_at(at::STAND1, false), Some(None));
        assert_eq!(action::for_at(at::ATTACK1, false), None);
        for at in [at::SIT1, at::POSE1, at::HEALING1] {
            assert!(action::resting(a(at, false).unwrap() as usize) && action::resting(a(at, true).unwrap() as usize));
        }
        for at in at::GREETING1..=at::RUSH1 {
            assert!(action::emote(a(at, false).unwrap() as usize), "{at}");
        }
        assert!(action::standing(ids::PLAYER_STOP_FEMALE as usize) && !action::standing(ids::PLAYER_SIT1 as usize));
    }
}
