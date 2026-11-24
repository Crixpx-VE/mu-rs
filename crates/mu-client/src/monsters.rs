//! Map monsters and NPCs: the model for each server monster number (`CreateMonster`, see
//! `mu_data::monsters`), animated with its actions (`MONSTER01_*`) and its `PlaySpeed`, with terrain light
//! and walking tile by tile.
//!
//! Driven by orders ([`MonsterOrder`], or [`Monsters`] from a system): appear on a tile facing
//! a direction, change action, walk and disappear. The network sends them (`AddNpcsToScope`,
//! `ObjectMoved`, `ObjectAnimation`...); with `--monstruos-prueba` [`test_monsters`] sends them.
//!
//! Animation like the player's: the original advances `PlaySpeed` frames per 25 Hz tick
//! (`CharacterAnimation`, ZzzCharacter.cpp:2280); here the same amount advances per game second and the pose
//! is interpolated between frames. When an animation ends, `BMD::PlayAnimation`
//! (ZzzBMD.cpp:408-479) and `EtcStopAnimationSetting` (ZzzCharacter.cpp:3290-3335) apply.
//!
//! Not yet: weapons in the hands (only type and level are stored), the armor of character-shaped
//! NPCs (comes with the class parts), each monster's own effects (`MoveMonsterVisual`,
//! named bones, `CreateJoint`...), sounds and head turning (`BoneHead`).

use std::collections::{HashMap, HashSet, VecDeque};
use std::f32::consts::FRAC_PI_2;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::PrimitiveTopology;
use bevy::prelude::*;
use mu_data::bmd::{self, Bmd};
use mu_data::monsters::{self as data, Def, Model, Part, SubType, action};
use mu_data::objects::render::{self, LookBlend, LookColor, MeshInfo, MeshLook, ObjectDraw, Saw};
use mu_data::texture::AlphaKind;
use mu_data::{fs, texture};

use crate::Args;
use crate::fx::tables::ids;
use crate::model_light::{ModelLight, ModelMaterial, model_material};
use crate::scene::GameScene;
use crate::terrain::{LoadedTerrain, MU_SCALE, to_bevy_image};

pub struct MonstersPlugin;

impl Plugin for MonstersPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MonsterOrder>()
            .init_resource::<Pending>()
            .init_resource::<Cache>()
            .add_systems(PreUpdate, from_server.after(crate::net::NetSystems))
            .add_systems(OnEnter(GameScene::CharacterSelect), |mut p: ResMut<Pending>| p.0.clear())
            .add_systems(
                Update,
                (collect, test_monsters, apply, walk, animate).chain().run_if(in_state(GameScene::Game)),
            );
    }
}

/// What a monster (or NPC) can be told to do. `key` is its server object number.
#[derive(Message, Clone, Debug)]
pub enum MonsterOrder {
    /// `CreateMonster(number, x, y, key)` facing `direction` (0..7, the step directions).
    Spawn { key: u16, number: u16, x: u8, y: u8, direction: u8 },
    Action { key: u16, action: MonsterAction },
    /// Walk to the tile (finds the path, like `MoveMonsterClient` with `PathFinding2`).
    WalkTo { key: u16, x: u8, y: u8 },
    /// Take these steps (directions 0..7) from where it stands.
    Walk { key: u16, directions: Vec<u8> },
    /// Face this direction (0..7).
    Turn { key: u16, direction: u8 },
    Despawn { key: u16 },
}

impl MonsterOrder {
    fn key(&self) -> u16 {
        match self {
            Self::Spawn { key, .. }
            | Self::Action { key, .. }
            | Self::WalkTo { key, .. }
            | Self::Walk { key, .. }
            | Self::Turn { key, .. }
            | Self::Despawn { key } => *key,
        }
    }
}

/// Monster actions. Character-shaped ones (`MODEL_PLAYER`) translate them to their own like
/// `SetPlayerStop` / `SetPlayerWalk` / `SetPlayerAttack` / `SetPlayerShock` / `SetPlayerDie`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // The network will use them.
pub enum MonsterAction {
    Stop,
    Stop2,
    Attack1,
    Attack2,
    Shock,
    Die,
    /// A model action as is (`MONSTER01_*`, or `PLAYER_*` for character-shaped ones).
    Raw(u16),
}

/// To send orders from any system.
#[derive(SystemParam)]
#[allow(dead_code)] // The network will use it; for now only `--monstruos-prueba`.
pub struct Monsters<'w> {
    orders: MessageWriter<'w, MonsterOrder>,
}

#[allow(dead_code)]
impl Monsters<'_> {
    pub fn spawn_monster(&mut self, key: u16, number: u16, x: u8, y: u8, direction: u8) {
        self.orders.write(MonsterOrder::Spawn { key, number, x, y, direction });
    }
    pub fn set_action(&mut self, key: u16, action: MonsterAction) {
        self.orders.write(MonsterOrder::Action { key, action });
    }
    pub fn walk_to(&mut self, key: u16, x: u8, y: u8) {
        self.orders.write(MonsterOrder::WalkTo { key, x, y });
    }
    pub fn walk_path(&mut self, key: u16, directions: Vec<u8>) {
        self.orders.write(MonsterOrder::Walk { key, directions });
    }
    pub fn turn(&mut self, key: u16, direction: u8) {
        self.orders.write(MonsterOrder::Turn { key, direction });
    }
    pub fn despawn(&mut self, key: u16) {
        self.orders.write(MonsterOrder::Despawn { key });
    }
}

/// Orders waiting for the map to be loaded.
#[derive(Resource, Default)]
struct Pending(Vec<MonsterOrder>);

fn collect(mut from: MessageReader<MonsterOrder>, mut pending: ResMut<Pending>) {
    pending.0.extend(from.read().cloned());
}

/// What the server sends about monsters and NPCs:
/// - `AddMonstersToScope` (`ReceiveCreateMonsterViewport`, WSclient.cpp:2512-2639): appears on its tile
///   facing `Path >> 4` and, if its destination tile differs, walks to it (`PathFinding2`);
/// - `ObjectWalked` (`ReceiveMoveCharacter`): walks to the destination tile from wherever it is;
/// - `MapObjectOutOfScope`: disappears.
///
/// Player messages also come through here: orders with a `key` that belongs to no
/// monster do nothing.
fn from_server(mut from: MessageReader<crate::net::FromServer>, mut pending: ResMut<Pending>) {
    use mu_protocol::game::{OBJECT_ID_MASK, ServerMessage};
    for m in from.read() {
        match &m.0 {
            ServerMessage::AddMonstersToScope(list) => {
                for s in list {
                    let key = s.id & OBJECT_ID_MASK;
                    pending.0.push(MonsterOrder::Spawn { key, number: s.number, x: s.x, y: s.y, direction: s.rotation });
                    if (s.target_x, s.target_y) != (s.x, s.y) {
                        pending.0.push(MonsterOrder::WalkTo { key, x: s.target_x, y: s.target_y });
                    }
                }
            }
            ServerMessage::ObjectWalked { id, target_x, target_y, .. } => {
                pending.0.push(MonsterOrder::WalkTo { key: id & OBJECT_ID_MASK, x: *target_x, y: *target_y });
            }
            ServerMessage::MapObjectOutOfScope(ids) => {
                pending.0.extend(ids.iter().map(|id| MonsterOrder::Despawn { key: id & OBJECT_ID_MASK }));
            }
            _ => {}
        }
    }
}

/// Where the action speeds come from.
#[derive(Clone, Copy)]
enum Speeds {
    Monster(&'static data::MonsterModel),
    Npc(&'static data::NpcModel),
    /// `player.bmd`: those of `character::play_speed`.
    Player,
    /// Models without a table (`MODEL_WARCRAFT`, trap objects): the original sets no `PlaySpeed` for them.
    Unset,
}

/// A monster or NPC on the map.
#[derive(Component)]
pub struct Monster {
    pub key: u16,
    pub number: u16,
    pub def: &'static Def,
    /// Position in tiles (fractional while walking).
    pub pos: Vec2,
    path: VecDeque<(usize, usize)>,
    /// MU angle (`o->Angle[2]`, in radians).
    facing: f32,
    pub action: usize,
    frame: f32,
    /// The animation with `Loop` ended: it stays on its last frame (`MONSTER01_DIE`).
    held: bool,
    pub dead: bool,
    skeleton: Arc<Bmd>,
    parts: Vec<RigPart>,
    speeds: Speeds,
    /// `player.bmd` actions (character-shaped ones).
    player: Option<PlayerActions>,
    /// Materials and how the light reaches them.
    materials: Vec<(Handle<ModelMaterial>, LookColor)>,
    light: Vec3,
    scale: f32,
    /// `c->MoveSpeed` in tiles per second.
    speed: f32,
    /// `o->Timer` (rises 0.15 per tick while moving, `MoveCharacterPosition`).
    timer: f32,
    shadow: bool,
}

/// A mesh of the model (or of a part) posed with the skeleton's bones.
struct RigPart {
    model: Arc<Bmd>,
    mesh: usize,
    handle: Handle<Mesh>,
    /// Goes into the shadow (`RenderBodyShadow` draws neither the `BlendMesh` nor the `HiddenMesh`).
    shadow: bool,
}

/// Models and textures already loaded (each monster has its own meshes: posed on the CPU).
#[derive(Resource, Default)]
struct Cache {
    bmds: HashMap<PathBuf, Option<Arc<Bmd>>>,
    images: HashMap<PathBuf, Option<(Handle<Image>, AlphaKind)>>,
}

impl Cache {
    fn bmd(&mut self, data: &Path, dir: &str, file: &str) -> Option<Arc<Bmd>> {
        let dir = find_dir(data, dir);
        let path = dir.join(file);
        self.bmds
            .entry(path)
            .or_insert_with(|| match fs::read(&dir, file).and_then(|b| bmd::read(&b)) {
                Ok(m) => Some(Arc::new(m)),
                Err(e) => {
                    warn!("monstruo: no se pudo cargar {file}: {e}");
                    None
                }
            })
            .clone()
    }

    /// Texture `name` of a mesh, searched in `dirs` in order (like `OpenTexture`, which, if it does not
    /// find it, uses the already loaded one with the same name: LoadData.cpp:82-90).
    fn texture(&mut self, data: &Path, dirs: &[&str], name: &str, images: &mut Assets<Image>) -> Option<(Handle<Image>, AlphaKind)> {
        let file = bmd::texture_file_name(name);
        let path = dirs.iter().find_map(|d| fs::find(&find_dir(data, d), &file))?;
        self.images
            .entry(path.clone())
            .or_insert_with(|| {
                let t = texture::load(path.parent()?, path.file_name()?.to_str()?).map_err(|e| warn!("{}: {e}", path.display())).ok()?;
                Some((images.add(to_bevy_image(&t)), t.alpha_kind()))
            })
            .clone()
    }
}

/// Folder under `Data`, case-insensitive (`Npc`/`NPC`...).
fn find_dir(data: &Path, rel: &str) -> PathBuf {
    let mut d = data.to_path_buf();
    for part in rel.split('/').filter(|p| !p.is_empty()) {
        d = fs::find(&d, part).unwrap_or_else(|| d.join(part));
    }
    d
}

/// A part that gets drawn: file and texture folders.
struct Piece {
    dir: String,
    file: String,
    textures: Vec<String>,
    /// `MODEL_LITTLESANTA + k`: the digit its first texture carries (ZzzOpenData.cpp:2170-2183).
    texture_digit: Option<u8>,
}

/// Skeleton, parts and speeds of the `def` model on map `world_active`.
fn sources(def: &Def, world_active: i32) -> Option<((String, String), Vec<Piece>, Speeds)> {
    let piece = |dir: &str, file: &str, textures: &[&str]| Piece {
        dir: dir.into(),
        file: file.into(),
        textures: textures.iter().map(|s| s.to_string()).collect(),
        texture_digit: None,
    };
    Some(match def.model {
        Model::Monster(i) => {
            let file = data::monster_file(i);
            let tex = data::monster_texture_dir(i, world_active);
            let speeds = data::monster_model(i).map_or(Speeds::Unset, Speeds::Monster);
            (("Monster".into(), file.clone()), vec![piece("Monster", &file, &[tex])], speeds)
        }
        Model::Npc(name) => {
            let npc = data::npc_model(name)?;
            // `OpenNpc` ends with `OpenTexture(Type, "Npc\\")` (ZzzOpenData.cpp:2271): `Npc` first.
            let mut main = piece(npc.dir, npc.file, &["Npc", npc.textures]);
            if npc.texture_patch {
                let k = name.rsplit_once('+').and_then(|(_, k)| k.parse::<u8>().ok()).unwrap_or(0);
                main.texture_digit = Some(k + 1);
            }
            let mut pieces = vec![main];
            for part in def.body {
                if let Part::Npc(p) = part
                    && let Some((dir, file, tex)) = data::npc_part(p)
                {
                    pieces.push(piece(dir, file, &["Npc", tex]));
                }
            }
            ((npc.dir.into(), npc.file.into()), pieces, Speeds::Npc(npc))
        }
        Model::Other(name) => {
            let (dir, file, tex) = data::other_model(name)?;
            ((dir.into(), file.into()), vec![piece(dir, file, &[tex])], Speeds::Unset)
        }
        Model::Object(kind) => {
            let rel = mu_data::objects::model_path((world_active + 1) as u16, kind as i16)?;
            let dir = rel.parent()?.to_str()?.replace('\\', "/");
            let file = rel.file_name()?.to_str()?.to_string();
            ((dir.clone(), file.clone()), vec![piece(&dir, &file, &[&dir])], Speeds::Unset)
        }
        Model::Player => {
            let mut pieces = Vec::new();
            if let SubType::Model(name) = def.sub_type
                && let Some((dir, file, tex)) = data::other_model(name)
            {
                // `RenderPartObject(o, o->SubType, ...)` with the player's bones (ZzzCharacter.cpp:9185-9188).
                pieces.push(piece(dir, file, &[tex]));
            } else {
                // Armor (`BodyPart`) are items: until they are loaded, the class 0 unequipped
                // parts (`Class` of `CreateCharacterPointer`).
                for part in ["Helm", "Armor", "Pant", "Glove", "Boot"] {
                    pieces.push(piece("Player", &format!("{part}Class01.bmd"), &["Player"]));
                }
            }
            (("Player".into(), "player.bmd".into()), pieces, Speeds::Player)
        }
    })
}

/// What is needed to create a monster's entity.
struct Built {
    skeleton: Arc<Bmd>,
    parts: Vec<RigPart>,
    children: Vec<(Mesh3d, MeshMaterial3d<ModelMaterial>)>,
    materials: Vec<(Handle<ModelMaterial>, LookColor)>,
    speeds: Speeds,
}

#[allow(clippy::too_many_arguments)]
fn build(
    def: &Def,
    world_active: i32,
    data_dir: &Path,
    cache: &mut Cache,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ModelMaterial>,
    images: &mut Assets<Image>,
    start_action: usize,
) -> Option<Built> {
    let ((sdir, sfile), pieces, speeds) = sources(def, world_active)?;
    let skeleton = cache.bmd(data_dir, &sdir, &sfile)?;
    let bones = skeleton.bone_matrices(start_action.min(skeleton.actions.len().saturating_sub(1)), 0);
    let stream_mesh = def.stream_mesh.map_or_else(
        || match def.model {
            Model::Monster(i) => data::monster_model(i).map_or(-1, |m| m.stream_mesh as i32),
            _ => -1,
        },
        |s| s as i32,
    );
    let draw = ObjectDraw {
        blend_mesh: def.blend_mesh as i32,
        stream_mesh,
        tex_coord: [Saw::ZERO; 2],
        hidden_mesh: def.hidden_mesh as i32,
    };
    let mut out = Built { skeleton: skeleton.clone(), parts: Vec::new(), children: Vec::new(), materials: Vec::new(), speeds };
    for p in pieces {
        let Some(model) = cache.bmd(data_dir, &p.dir, &p.file) else { continue };
        let dirs: Vec<&str> = p.textures.iter().map(String::as_str).collect();
        for i in 0..model.meshes.len() {
            let mesh = &model.meshes[i];
            let mut tex_name = mesh.texture.clone();
            if let (Some(k), 0) = (p.texture_digit, i)
                && tex_name.len() >= 9
                && tex_name.is_char_boundary(9)
            {
                tex_name.insert(9, char::from(b'0' + k % 10));
            }
            // `OpenTexture`: `hid*` textures are `BITMAP_HIDE` (LoadData.cpp:67) and `RenderMesh` does not
            // draw them (ZzzBMD.cpp:947).
            if tex_name.starts_with("hid") {
                continue;
            }
            let posed = model.pose_with(i, &bones);
            if posed.positions.is_empty() {
                continue;
            }
            let handle = meshes.add(
                Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals)
                    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, posed.uvs),
            );
            let image = cache.texture(data_dir, &dirs, &tex_name, images);
            let info = MeshInfo {
                index: i,
                texture_index: mesh.texture_index as i32,
                script: bmd::texture_script(&mesh.texture),
                has_alpha: image.as_ref().is_some_and(|i| i.1 != AlphaKind::Opaque),
            };
            // `RenderBody(RENDER_TEXTURE, o->Alpha, o->BlendMesh, o->BlendMeshLight, ...)` from `RenderObject`.
            let looks = render::resolve_all(render::DEFAULT_PASSES, &draw, &info);
            let in_shadow = i as i32 != draw.hidden_mesh && mesh.texture_index as i32 != draw.blend_mesh;
            for (pass, look) in &looks {
                let image = (look.textured && look.texture.is_none()).then(|| image.as_ref().map(|i| i.0.clone())).flatten();
                let m = materials.add(look_material(image, look, *pass));
                out.materials.push((m.clone(), look.color));
                out.children.push((Mesh3d(handle.clone()), MeshMaterial3d(m)));
            }
            if !looks.is_empty() {
                out.parts.push(RigPart { model: model.clone(), mesh: i, handle, shadow: in_shadow });
            }
        }
    }
    Some(out)
}

/// Material of a mesh in one pass (like `objects::object_material`, with the light set later).
fn look_material(image: Option<Handle<Image>>, look: &MeshLook, pass: usize) -> ModelMaterial {
    let alpha_mode = match look.blend {
        LookBlend::Opaque => AlphaMode::Opaque,
        LookBlend::AlphaTest => AlphaMode::Mask(crate::objects::ALPHA_REF),
        LookBlend::Add | LookBlend::Subtract => AlphaMode::Add,
    };
    let base = StandardMaterial {
        base_color_texture: image,
        unlit: true,
        cull_mode: None,
        alpha_mode,
        depth_bias: pass as f32 * 2.0,
        ..default()
    };
    let light = match look.color {
        LookColor::Lit => ModelLight::lit([1.0; 3]),
        LookColor::Flat | LookColor::BlendLight => ModelLight::flat([1.0; 3]),
    };
    model_material(base, light.with_chrome(look.chrome).with_subtract(look.blend == LookBlend::Subtract))
}

/// The `player.bmd` actions of a character-shaped monster, depending on its weapons.
#[derive(Clone, Copy, Debug, PartialEq)]
struct PlayerActions {
    stop: usize,
    walk: usize,
    attack: [usize; 2],
}

/// `GetEquipedBowType` (CharacterManager.cpp:281-292): bow in the left hand (except the quiver, `BOW+7`) or
/// crossbow in the right (except the bolts, `BOW+15`).
fn bow_type(def: &Def) -> u8 {
    let is = |w: Option<data::Item>, f: &dyn Fn(u16) -> bool| w.is_some_and(|w| w.group == 4 && f(w.index));
    if is(def.weapons[1], &|i| i != 7) {
        1
    } else if is(def.weapons[0], &|i| i >= 8 && i != 15) {
        2
    } else {
        0
    }
}

/// `SetPlayerStop`, `SetPlayerWalk` and `SetPlayerAttack` (ZzzCharacter.cpp:173-395, 399-946, 948-1107) for
/// a monster without mount or wings, `Class` 0 and `Run` < 40. `TwoHand` comes from the item table
/// (`ItemAttribute`), not read yet: all count as one-handed.
fn player_actions(def: &Def, safe_zone: bool) -> PlayerActions {
    let [w0, w1] = def.weapons;
    let two_hand = false;
    let g0 = w0.map(|w| (w.group, w.index));
    let sword_like = |g: Option<(u8, u16)>| g.is_some_and(|(g, _)| g <= 2);
    let id = |v: i32| v as usize;
    // 257 flies (`MonsterIndex == 257`, ZzzCharacter.cpp:245).
    if def.number == 257 {
        return PlayerActions { stop: id(ids::PLAYER_STOP_FLY), walk: id(ids::PLAYER_FLY), attack: [id(ids::PLAYER_ATTACK_FIST); 2] };
    }
    let bow = bow_type(def);
    let unarmed = (w0.is_none() && w1.is_none()) || safe_zone;
    let stop = if unarmed {
        ids::PLAYER_STOP_MALE
    } else if sword_like(g0) {
        if !two_hand { ids::PLAYER_STOP_SWORD } else { ids::PLAYER_STOP_TWO_HAND_SWORD }
    } else if matches!(g0, Some((3, 1 | 2))) {
        ids::PLAYER_STOP_SPEAR
    } else if matches!(g0, Some((3, _))) {
        if !two_hand { ids::PLAYER_STOP_SWORD } else { ids::PLAYER_STOP_SCYTHE }
    } else if matches!(g0, Some((5, 14..=20))) {
        ids::PLAYER_STOP_WAND
    } else if matches!(g0, Some((5, _))) {
        if !two_hand { ids::PLAYER_STOP_SWORD } else { ids::PLAYER_STOP_SCYTHE }
    } else if bow == 1 {
        ids::PLAYER_STOP_BOW
    } else if bow == 2 {
        ids::PLAYER_STOP_CROSSBOW
    } else {
        ids::PLAYER_STOP_MALE
    };
    let walk = if unarmed {
        ids::PLAYER_WALK_MALE
    } else if sword_like(g0) {
        if !two_hand { ids::PLAYER_WALK_SWORD } else { ids::PLAYER_WALK_TWO_HAND_SWORD }
    } else if matches!(g0, Some((5, 14..=20))) {
        ids::PLAYER_WALK_WAND
    } else if matches!(g0, Some((5, _))) {
        if !two_hand { ids::PLAYER_WALK_SWORD } else { ids::PLAYER_WALK_SCYTHE }
    } else if matches!(g0, Some((3, 1))) {
        // ZzzCharacter.cpp:641: `MODEL_SPEAR+2+MAX_ITEM_INDEX` never matches: only `SPEAR+1`.
        ids::PLAYER_WALK_SPEAR
    } else if matches!(g0, Some((3, _))) {
        ids::PLAYER_WALK_SCYTHE
    } else if bow == 1 {
        ids::PLAYER_WALK_BOW
    } else if bow == 2 {
        ids::PLAYER_WALK_CROSSBOW
    } else {
        ids::PLAYER_WALK_MALE
    };
    let attack = if w0.is_none() && w1.is_none() {
        [ids::PLAYER_ATTACK_FIST; 2]
    } else if sword_like(g0) {
        if sword_like(w1.map(|w| (w.group, w.index))) {
            [ids::PLAYER_ATTACK_SWORD_RIGHT1, ids::PLAYER_ATTACK_SWORD_LEFT1]
        } else {
            [ids::PLAYER_ATTACK_SWORD_RIGHT1, ids::PLAYER_ATTACK_SWORD_RIGHT1 + 1]
        }
    } else if sword_like(w1.map(|w| (w.group, w.index))) {
        [ids::PLAYER_ATTACK_SWORD_LEFT1, ids::PLAYER_ATTACK_SWORD_LEFT1 + 1]
    } else if matches!(g0, Some((5, _))) {
        [ids::PLAYER_ATTACK_SWORD_RIGHT1, ids::PLAYER_ATTACK_SWORD_RIGHT1 + 1]
    } else if matches!(g0, Some((3, 1 | 2))) {
        [ids::PLAYER_ATTACK_SPEAR1; 2]
    } else if matches!(g0, Some((3, _))) {
        [ids::PLAYER_ATTACK_SCYTHE1, ids::PLAYER_ATTACK_SCYTHE1 + 1]
    } else if bow == 1 {
        [if def.wing.is_some() { ids::PLAYER_ATTACK_FLY_BOW } else { ids::PLAYER_ATTACK_BOW }; 2]
    } else if bow == 2 {
        [if def.wing.is_some() { ids::PLAYER_ATTACK_FLY_CROSSBOW } else { ids::PLAYER_ATTACK_CROSSBOW }; 2]
    } else {
        [ids::PLAYER_ATTACK_FIST; 2]
    };
    PlayerActions { stop: id(stop), walk: id(walk), attack: attack.map(id) }
}

impl Monster {
    /// `Actions[action].PlaySpeed` of its model, with what `CharacterAnimation` changes
    /// (ZzzCharacter.cpp:2322-2347) and what its `case` sets by hand (`Models[...].Actions[i].PlaySpeed`).
    fn play_speed(&self) -> f32 {
        let a = self.action;
        if let Some(&(_, s)) = self.def.model_speeds.iter().find(|(i, _)| *i as usize == a) {
            return s;
        }
        let mut s = match self.speeds {
            Speeds::Monster(m) => m.play_speed(a),
            Speeds::Npc(n) => n.play_speed(a),
            Speeds::Player => crate::character::play_speed(a),
            Speeds::Unset => data::UNSET_PLAY_SPEED,
        };
        match self.def.model {
            Model::Monster(64) if a == action::DIE && self.frame > 6.0 => s *= 4.0,
            Model::Monster(87) if a == action::DIE => s /= 2.0,
            _ => {}
        }
        s.max(0.0)
    }

    /// `Actions[action].Loop`.
    fn holds(&self) -> bool {
        match self.speeds {
            Speeds::Monster(m) => m.holds(self.action),
            // `OpenPlayers` (ZzzOpenData.cpp:380-381).
            Speeds::Player => self.action == ids::PLAYER_DIE1 as usize || self.action == ids::PLAYER_DIE2 as usize,
            _ => false,
        }
    }

    fn stop_action(&self) -> usize {
        self.player.map_or(action::STOP1, |p| p.stop)
    }

    fn walk_action(&self) -> usize {
        self.player.map_or(action::WALK, |p| p.walk)
    }

    /// `SetAction`: changes the action (if the model has it) and starts at frame 0.
    fn set_action(&mut self, a: usize) {
        if a >= self.skeleton.actions.len() {
            return;
        }
        if a != self.action {
            self.action = a;
            self.frame = 0.0;
        }
        self.held = false;
    }

    fn order(&mut self, a: MonsterAction) {
        let pa = self.player;
        let id = match (a, pa) {
            (MonsterAction::Stop, _) => self.stop_action(),
            (MonsterAction::Stop2, None) => action::STOP2,
            (MonsterAction::Stop2, Some(p)) => p.stop,
            (MonsterAction::Attack1, None) => action::ATTACK1,
            (MonsterAction::Attack2, None) => action::ATTACK2,
            (MonsterAction::Attack1, Some(p)) => p.attack[0],
            (MonsterAction::Attack2, Some(p)) => p.attack[1],
            // `SetPlayerShock` (ZzzCharacter.cpp:1286-1294): monsters do not flinch while attacking.
            (MonsterAction::Shock, None) => {
                if (action::ATTACK1..=action::ATTACK2).contains(&self.action) {
                    return;
                }
                action::SHOCK
            }
            (MonsterAction::Shock, Some(_)) => ids::PLAYER_SHOCK as usize,
            (MonsterAction::Die, None) => action::DIE,
            (MonsterAction::Die, Some(_)) => ids::PLAYER_DIE1 as usize,
            (MonsterAction::Raw(a), _) => a as usize,
        };
        if matches!(a, MonsterAction::Die) {
            self.dead = true;
            self.path.clear();
        } else if self.dead && matches!(a, MonsterAction::Stop | MonsterAction::Raw(_)) {
            self.dead = false;
        }
        self.set_action(id);
    }
}

/// `SetPlayerDie` (ZzzCharacter.cpp:1359-1395): skeletons and models 25 and 30 have no death
/// animation: they vanish (`o->Live = false`) and bones or stones fly out (effects not translated yet).
fn vanishes_on_death(def: &Def) -> bool {
    matches!(def.sub_type, SubType::Model("MODEL_SKELETON1" | "MODEL_SKELETON2" | "MODEL_SKELETON3")) && def.model == Model::Player
        || matches!(def.model, Model::Monster(25 | 30))
}

/// `BMD::PlayAnimation` (ZzzBMD.cpp:425-478) advancing `speed` frames. Returns `false` when the
/// animation ends (restarts, or stays on the last frame if it has `Loop`).
fn play_animation(frame: &mut f32, speed: f32, frames: usize, locked: bool, hold: bool) -> bool {
    if frames <= 1 {
        return true;
    }
    *frame += speed;
    if *frame <= 0.0 {
        *frame += frames as f32 - 1.0;
    }
    if hold {
        if *frame >= frames as f32 {
            *frame = frames as f32 - 0.01;
            return false;
        }
    } else {
        let key = if locked { frames - 1 } else { frames }.max(1);
        if *frame >= key as f32 {
            let f = frame.floor();
            *frame = (f as usize % key) as f32 + (*frame - f);
            return false;
        }
    }
    true
}

/// The frame to draw: with `Loop` finished, the last one (`bone_matrices_at` would wrap to the first).
fn pose_frame(frame: f32, frames: usize, held: bool) -> f32 {
    if held { frames.saturating_sub(1) as f32 } else { frame }
}

#[allow(clippy::too_many_arguments)]
fn apply(
    mut commands: Commands,
    args: Res<Args>,
    terrain: Option<Res<LoadedTerrain>>,
    mut pending: ResMut<Pending>,
    mut cache: ResMut<Cache>,
    mut monsters: Query<(Entity, &mut Monster, &crate::shadow::Shadow)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    shadow_material: Res<crate::shadow::ShadowMaterial>,
    mut rng: Local<u32>,
) {
    let Some(terrain) = terrain else { return };
    let world_active = args.world as i32 - 1;
    // Commands only take effect after this system, so within one batch the query still shows the
    // monsters despawned here (`gone`) and does not show the ones spawned here (`fresh`). OpenMU
    // sends "out of scope" and "in scope" for the same key in one packet, and a monster that enters
    // the view already walking comes with a WalkTo right after its Spawn.
    let mut gone: HashSet<Entity> = HashSet::new();
    let mut fresh: HashMap<u16, [Entity; 2]> = HashMap::new();
    // Orders for a monster spawned in this batch wait for the next frame, when it exists.
    let mut later = Vec::new();
    for order in std::mem::take(&mut pending.0) {
        if !matches!(order, MonsterOrder::Spawn { .. } | MonsterOrder::Despawn { .. }) && fresh.contains_key(&order.key()) {
            later.push(order);
            continue;
        }
        match order {
            MonsterOrder::Spawn { key, number, x, y, direction } => {
                // If it already existed, it is removed and created again (`CreateCharacter` reuses the one for its `Key`).
                for (e, m, shadow) in &monsters {
                    if m.key == key && gone.insert(e) {
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                    }
                }
                if let Some(old) = fresh.remove(&key) {
                    old.into_iter().for_each(|e| commands.entity(e).despawn());
                }
                let variants = data::defs(number, world_active);
                let def = &variants[(next_rand(&mut rng) as usize) % variants.len()];
                let safe = terrain.0.attribute(x as usize, y as usize) & mu_data::terrain::attr::SAFE_ZONE != 0;
                let player = (def.model == Model::Player).then(|| player_actions(def, safe));
                let start = def.start_action.map(|a| a as usize).unwrap_or_else(|| player.map_or(action::STOP1, |p| p.stop));
                let Some(built) = build(def, world_active, &args.data, &mut cache, &mut meshes, &mut materials, &mut images, start)
                else {
                    warn!("monstruo {number} ({}): sin modelo ({:?})", def.display_name(), def.model);
                    continue;
                };
                let (dx, dy) = def.tile_offset;
                let tile = Vec2::new((x as i32 + dx as i32) as f32 + 0.5, (y as i32 + dy as i32) as f32 + 0.5);
                let kind = def.kind();
                let shadow_on = def.render_shadow
                    && kind != data::Kind::Trap
                    && !matches!(number, 25 | 22 | 42 | 242 | 59 | 63 | 152)
                    && !matches!(def.model, Model::Monster(60 | 61))
                    && !(def.model == Model::Player && def.hide_shadow);
                let facing = def.angle[2].map_or_else(|| crate::character::direction_facing(direction), f32::to_radians);
                let mut m = Monster {
                    key,
                    number,
                    def,
                    pos: tile,
                    path: VecDeque::new(),
                    facing,
                    action: start,
                    frame: def.start_frame.unwrap_or(0.0),
                    held: false,
                    dead: false,
                    skeleton: built.skeleton,
                    parts: built.parts,
                    speeds: built.speeds,
                    player,
                    materials: built.materials,
                    light: Vec3::splat(-1.0),
                    scale: def.scale_in(world_active),
                    // `MoveCharacterPosition` (ZzzCharacter.cpp:6201-6227): `MoveSpeed` units per tick.
                    speed: def.move_speed as f32 * crate::fx::sim::TICKS_PER_SECOND / MU_SCALE,
                    timer: 0.0,
                    shadow: shadow_on,
                };
                if start >= m.skeleton.actions.len() {
                    m.action = 0;
                }
                info!("aparece {} ({number}, {:?}) en ({x}, {y})", def.display_name(), def.model);
                let shadow = crate::shadow::spawn(&mut commands, &mut meshes, &shadow_material);
                let shadow_entity = shadow.1;
                let transform = monster_transform(&m, &terrain);
                let mut e = commands.spawn((
                    Name::new(format!("Monstruo {number} {}", def.display_name())),
                    crate::scene::WorldEntity,
                    transform,
                    Visibility::default(),
                    shadow,
                    crate::names::Selectable {
                        kind: if kind == data::Kind::Npc { crate::names::Kind::Npc } else { crate::names::Kind::Monster },
                        name: def.display_name().to_string(),
                        pk: 3,
                        level: def.level_in(world_active),
                        gm: false,
                        dead: false,
                        guild: None,
                        bbox_min: Vec3::from_array(bounding_box(def).0),
                        bbox_max: Vec3::from_array(bounding_box(def).1),
                        edge_scale: def.edge_scale,
                        light: Vec3::ZERO,
                    },
                ));
                for child in built.children {
                    e.with_child(child);
                }
                e.insert(m);
                fresh.insert(key, [e.id(), shadow_entity]);
            }
            MonsterOrder::Action { key, action } => {
                for (e, mut m, shadow) in &mut monsters {
                    if m.key != key || gone.contains(&e) {
                        continue;
                    }
                    if action == MonsterAction::Die && vanishes_on_death(m.def) {
                        gone.insert(e);
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                        continue;
                    }
                    m.order(action);
                }
            }
            MonsterOrder::WalkTo { key, x, y } => {
                for (_, mut m, _) in &mut monsters {
                    if m.key != key || m.dead {
                        continue;
                    }
                    let from = (m.pos.x as usize, m.pos.y as usize);
                    match mu_data::path::find_path(from, (x as usize, y as usize), |x, y| terrain.0.walkable(x, y), 20_000) {
                        Some(p) => m.path = p.into_iter().skip(1).collect(),
                        None => m.path.clear(),
                    }
                }
            }
            MonsterOrder::Walk { key, directions } => {
                for (_, mut m, _) in &mut monsters {
                    if m.key != key || m.dead {
                        continue;
                    }
                    let (mut x, mut y) = (m.pos.x as i32, m.pos.y as i32);
                    m.path.clear();
                    for d in &directions {
                        let (dx, dy) = mu_protocol::game::DIRECTIONS[(d & 7) as usize];
                        x += dx as i32;
                        y += dy as i32;
                        m.path.push_back((x.clamp(0, 255) as usize, y.clamp(0, 255) as usize));
                    }
                }
            }
            MonsterOrder::Turn { key, direction } => {
                for (_, mut m, _) in &mut monsters {
                    if m.key == key {
                        m.facing = crate::character::direction_facing(direction);
                    }
                }
            }
            MonsterOrder::Despawn { key } => {
                for (e, m, shadow) in &monsters {
                    if m.key == key && gone.insert(e) {
                        info!("desaparece {} ({})", m.def.display_name(), m.number);
                        commands.entity(e).despawn();
                        commands.entity(shadow.1).despawn();
                    }
                }
                if let Some(new) = fresh.remove(&key) {
                    new.into_iter().for_each(|e| commands.entity(e).despawn());
                }
            }
        }
    }
    pending.0.extend(later);
}

/// Selection box (`BoundingBoxMin/Max` from `CreateCharacterPointer`, ZzzCharacter.cpp:11397-11502,
/// or the one the `case` sets).
fn bounding_box(def: &Def) -> ([f32; 3], [f32; 3]) {
    let mut min = [-60.0, -60.0, 0.0];
    let mut max = match def.model {
        Model::Player | Model::Monster(70..=72) => [40.0, 40.0, 120.0],
        Model::Monster(2 | 6 | 9 | 20 | 19 | 17) => [50.0, 50.0, 80.0],
        Model::Monster(11 | 31 | 39 | 42 | 44) => [70.0, 70.0, 250.0],
        Model::Monster(37) => [100.0, 100.0, 150.0],
        Model::Monster(61) => {
            min = [-120.0, -120.0, 0.0];
            [100.0, 100.0, 300.0]
        }
        Model::Monster(60) => {
            min = [-90.0, -50.0, 0.0];
            [90.0, 50.0, 200.0]
        }
        Model::Monster(150) => {
            min = [-150.0, -150.0, 0.0];
            [150.0, 150.0, 400.0]
        }
        Model::Monster(151..=153) => {
            min = [-100.0, -100.0, 0.0];
            [100.0, 100.0, 200.0]
        }
        _ => [50.0, 50.0, 150.0],
    };
    if let Some(b) = def.bbox_max {
        max = b;
    }
    (min, max)
}

fn next_rand(state: &mut u32) -> u32 {
    if *state == 0 {
        *state = 0x9E37_79B9;
    }
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state >> 8
}

/// Monster height in MU: the ground (or the `case`'s `RequestTerrainHeight + h`) and the Budge
/// Dragon bobbing (`MoveCharacterPosition`, ZzzCharacter.cpp:6222-6226).
fn monster_height(m: &Monster, terrain: &LoadedTerrain) -> f32 {
    let ground = terrain.0.height_at(m.pos.x, m.pos.y);
    let mut h = m.def.height.map_or(ground + m.def.z_offset, |k| ground + k);
    if m.def.model == Model::Monster(2) {
        h += -m.timer.sin().abs() * 70.0 + 70.0;
    }
    h
}

fn monster_transform(m: &Monster, terrain: &LoadedTerrain) -> Transform {
    let h = monster_height(m, terrain);
    let [ax, ay] = [m.def.angle[0].unwrap_or(0.0), m.def.angle[1].unwrap_or(0.0)].map(f32::to_radians);
    let rot = Quat::from_euler(EulerRot::ZYX, m.facing, ay, ax);
    Transform {
        translation: Vec3::new(m.pos.x, h / MU_SCALE, -m.pos.y),
        rotation: Quat::from_rotation_x(-FRAC_PI_2) * rot,
        scale: Vec3::splat(m.scale / MU_SCALE),
    }
}

fn walk(time: Res<Time>, terrain: Option<Res<LoadedTerrain>>, mut q: Query<(&mut Monster, &mut Transform)>) {
    let Some(terrain) = terrain else { return };
    let dt = time.delta_secs();
    for (mut m, mut tf) in &mut q {
        let moving = !m.path.is_empty() && !m.dead;
        let mut budget = m.speed * dt;
        while budget > 0.0 && !m.dead {
            let Some(&(nx, ny)) = m.path.front() else { break };
            let target = Vec2::new(nx as f32 + 0.5, ny as f32 + 0.5);
            let delta = target - m.pos;
            let dist = delta.length();
            if dist > 1e-4 {
                m.facing = delta.x.atan2(-delta.y);
            }
            if dist <= budget {
                m.pos = target;
                budget -= dist;
                m.path.pop_front();
            } else {
                m.pos += delta / dist * budget;
                budget = 0.0;
            }
        }
        if moving {
            m.timer += 0.15 * crate::fx::sim::TICKS_PER_SECOND * dt;
            // Monster `SetPlayerWalk`: `MONSTER01_WALK` (ZzzCharacter.cpp:725).
            let w = m.walk_action();
            if m.action != w {
                m.set_action(w);
            }
        } else if !m.dead && m.action == m.walk_action() {
            // On arrival, `SetPlayerStop` (ZzzCharacter.cpp:366-373).
            let s = m.stop_action();
            m.set_action(s);
        }
        *tf = monster_transform(&m, &terrain);
    }
}

/// Animation, mesh pose, shadow and terrain light.
#[allow(clippy::type_complexity)]
fn animate(
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut q: Query<(&mut Monster, &crate::shadow::Shadow, Option<&crate::names::Selectable>)>,
    mut visibility: Query<&mut Visibility, Without<Monster>>,
) {
    let Some(terrain) = terrain else { return };
    let dt = time.delta_secs();
    for (mut m, shadow, selectable) in &mut q {
        let Some(act) = m.skeleton.actions.get(m.action) else { continue };
        let (frames, locked) = (act.frames, act.lock_positions.is_some());
        if !m.held {
            let speed = m.play_speed() * crate::fx::sim::TICKS_PER_SECOND * dt;
            let hold = m.holds();
            let mut frame = m.frame;
            let play = play_animation(&mut frame, speed, frames, locked, hold);
            m.frame = frame;
            if !play {
                if hold {
                    m.held = true;
                } else if m.player.is_none() {
                    // `EtcStopAnimationSetting` (ZzzCharacter.cpp:3307-3335).
                    let a = m.action;
                    if matches!(a, action::STOP2 | action::SHOCK | action::ATTACK1 | action::ATTACK2)
                        || matches!(a, action::ATTACK3 | action::ATTACK4 | action::ATTACK5)
                        || a == action::APEAR && matches!(m.def.model, Model::Monster(118 | 119 | 150))
                    {
                        m.set_action(action::STOP1);
                    }
                } else if m.action != m.stop_action() && m.action != m.walk_action() {
                    // `PlayerStopAnimationSetting` (ZzzCharacter.cpp:3264-3282): attacks and hits end in stop.
                    let s = m.stop_action();
                    m.set_action(s);
                }
            }
        }
        let frame = pose_frame(m.frame, frames, m.held);
        let bones = m.skeleton.bone_matrices_at(m.action, frame);
        let h = monster_height(&m, &terrain);
        let mut sh = crate::shadow::Builder::new(
            Some(&terrain.0),
            Vec3::new(m.pos.x * MU_SCALE, m.pos.y * MU_SCALE, h),
            Quat::from_rotation_z(m.facing),
            m.scale,
        );
        for part in &m.parts {
            let posed = part.model.pose_with(part.mesh, &bones);
            if m.shadow && part.shadow {
                sh.add(&posed.positions);
            }
            if let Some(mut mesh) = meshes.get_mut(&part.handle) {
                mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, posed.positions);
                mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, posed.normals);
            }
        }
        sh.finish(shadow, &mut meshes, visibility.get_mut(shadow.1).ok());

        // `c->Light` = terrain light + `o->Light` (ZzzCharacter.cpp:8906-8913); 55 is fixed at 0.6.
        let size = mu_data::terrain::SIZE;
        let (x, y) = ((m.pos.x.max(0.0) as usize).min(size - 1), (m.pos.y.max(0.0) as usize).min(size - 1));
        let mut light = Vec3::from_array(terrain.0.light[y * size + x]) + selectable.map_or(Vec3::ZERO, |s| s.light);
        if m.def.model == Model::Monster(55) {
            light = Vec3::splat(0.6);
        }
        if light != m.light {
            m.light = light;
            let glow = m.def.blend_mesh_light;
            for (h, color) in &m.materials {
                let Some(mut mat) = materials.get_mut(h) else { continue };
                match color {
                    LookColor::Lit => mat.extension.light = light.extend(1.0),
                    LookColor::Flat => mat.extension.params.tint = light.extend(1.0),
                    // `glColor3f(BodyLight · BlendMeshLight)`.
                    LookColor::BlendLight => mat.extension.params.tint = (light * glow).extend(1.0),
                }
            }
        }
    }
}

/// `--monstruos-prueba`: a few of the map's monsters and NPCs (those MuMain's `Monsters.json` says
/// appear on it) around the hero, changing action every few seconds to show their animations.
fn test_monsters(
    args: Res<Args>,
    time: Res<Time>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&crate::character::Player>,
    mut monsters: Monsters,
    mut state: Local<Option<TestState>>,
) {
    if !args.monster_test {
        return;
    }
    let (Some(terrain), Ok(hero)) = (terrain, players.single()) else { return };
    let world_active = args.world as i32 - 1;
    let st = state.get_or_insert_with(|| {
        let mut numbers: Vec<u16> = Vec::new();
        let mut npcs: Vec<u16> = Vec::new();
        for d in data::DEFS.iter() {
            if !d.seen_in.contains(&(world_active.clamp(0, 255) as u8)) || numbers.contains(&d.number) || npcs.contains(&d.number) {
                continue;
            }
            if d.kind() == data::Kind::Npc {
                if npcs.len() < 3 {
                    npcs.push(d.number);
                }
            } else if d.kind() == data::Kind::Monster && numbers.len() < 8 {
                numbers.push(d.number);
            }
        }
        if numbers.is_empty() {
            // No map data: the Lorencia ones.
            numbers = vec![0, 1, 2, 3, 4, 6, 14];
        }
        numbers.extend(npcs);
        let center = (hero.pos.x as i32, hero.pos.y as i32);
        let mut spots = Vec::new();
        'ring: for r in 3..12 {
            for k in 0..(r * 8) {
                let a = k as f32 / (r * 8) as f32 * std::f32::consts::TAU;
                let (x, y) = (center.0 + (a.cos() * r as f32).round() as i32, center.1 + (a.sin() * r as f32).round() as i32);
                if (0..256).contains(&x) && (0..256).contains(&y) && terrain.0.walkable(x as usize, y as usize) {
                    let p = (x as u8, y as u8);
                    if spots.iter().all(|s: &(u8, u8)| (s.0 as i32 - p.0 as i32).abs() + (s.1 as i32 - p.1 as i32).abs() >= 3) {
                        spots.push(p);
                        if spots.len() == numbers.len() {
                            break 'ring;
                        }
                    }
                }
            }
        }
        let list: Vec<(u16, u16, (u8, u8))> =
            numbers.iter().zip(spots).enumerate().map(|(i, (&n, s))| (TEST_KEY + i as u16, n, s)).collect();
        info!("--monstruos-prueba: {:?}", list.iter().map(|(_, n, _)| n).collect::<Vec<_>>());
        for &(key, number, (x, y)) in &list {
            monsters.spawn_monster(key, number, x, y, (key % 8) as u8);
        }
        TestState { list, step: 0, timer: 0.0 }
    });
    st.timer += time.delta_secs();
    if st.timer < 3.0 {
        return;
    }
    st.timer = 0.0;
    st.step += 1;
    // Stop → walk two tiles and back → attack 1 → attack 2 → hit → stop 2 → die → reappear.
    for (i, &(key, number, (x, y))) in st.list.iter().enumerate() {
        let step = (st.step + i) % 8;
        match step {
            0 => monsters.spawn_monster(key, number, x, y, (key % 8) as u8),
            1 => monsters.walk_to(key, x.saturating_add(2), y),
            2 => monsters.walk_to(key, x, y),
            3 => monsters.set_action(key, MonsterAction::Attack1),
            4 => monsters.set_action(key, MonsterAction::Attack2),
            5 => monsters.set_action(key, MonsterAction::Shock),
            6 => monsters.set_action(key, MonsterAction::Stop2),
            _ => monsters.set_action(key, MonsterAction::Die),
        }
    }
}

/// `Key` of the test monsters (far from the server's).
const TEST_KEY: u16 = 30_000;

struct TestState {
    list: Vec<(u16, u16, (u8, u8))>,
    step: usize,
    timer: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animacion_como_play_animation() {
        // Without `Loop`: restarts and reports it.
        let mut f = 9.8;
        assert!(!play_animation(&mut f, 0.5, 10, false, false));
        assert!((f - 0.3).abs() < 1e-5);
        // With locked positions the loop has one frame fewer.
        let mut f = 8.8;
        assert!(!play_animation(&mut f, 0.5, 10, true, false));
        assert!((f - 0.3).abs() < 1e-5);
        // With `Loop` (dying): stays at the end.
        let mut f = 9.8;
        assert!(!play_animation(&mut f, 0.5, 10, false, true));
        assert!((f - 9.99).abs() < 1e-5);
        assert_eq!(pose_frame(f, 10, true), 9.0);
        // In the middle it keeps going.
        let mut f = 2.0;
        assert!(play_animation(&mut f, 0.34, 10, false, false));
        assert!((f - 2.34).abs() < 1e-5);
    }

    #[test]
    fn acciones_de_los_esqueletos() {
        // 14: sword and shield → stop and walk with sword, sword attacks.
        let skel = data::defs(14, 0)[0];
        let a = player_actions(&skel, false);
        assert_eq!(a.stop, ids::PLAYER_STOP_SWORD as usize);
        assert_eq!(a.walk, ids::PLAYER_WALK_SWORD as usize);
        assert_eq!(a.attack[0], ids::PLAYER_ATTACK_SWORD_RIGHT1 as usize);
        // In a safe zone, no weapons in hand.
        assert_eq!(player_actions(&skel, true).stop, ids::PLAYER_STOP_MALE as usize);
        // 15: bow in the left hand.
        let archer = data::defs(15, 0)[0];
        let a = player_actions(&archer, false);
        assert_eq!((a.stop, a.walk, a.attack[0]), (ids::PLAYER_STOP_BOW as usize, ids::PLAYER_WALK_BOW as usize, ids::PLAYER_ATTACK_BOW as usize));
        assert!(vanishes_on_death(&skel));
        assert!(!vanishes_on_death(&data::defs(0, 0)[0]));
    }

    /// Every number in the table has a source for its model.
    #[test]
    fn todos_tienen_modelo() {
        for d in data::DEFS.iter() {
            let ok = sources(d, 0).is_some() || matches!(d.model, Model::Object(_));
            assert!(ok || matches!(d.model, Model::Other("MODEL_XMAS_EVENT_CHANGE_GIRL")), "{} {:?}", d.number, d.model);
        }
    }
}
