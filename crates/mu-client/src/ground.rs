//! Items lying on the ground (`Items[]`, ZzzObject.cpp and ZzzInventory.cpp), dropping the held item
//! and picking items up. The server is the only authority over items and zen (AGENTS.md): the client
//! only asks, and the inventory and the zen change when the server answers.
//!
//! - **Appear / disappear**: `ReceiveCreateItemViewport` (C2 20, WSclient.cpp:5558-5586) creates the
//!   item at the centre of its tile (`CreateItem`, ZzzObject.cpp:6111-6324): a fresh drop
//!   (`CreateFlag`, the key's high bit) starts 180 above the terrain with `Gravity` 20 and plays its
//!   sound (`SOUND_DROP_GOLD01` for zen, `SOUND_JEWEL01`/`02` for the jewels, `SOUND_DROP_ITEM01`);
//!   one already lying there starts on the terrain. `ReceiveDeleteItemViewport` (C2 21) removes it,
//!   and so does a map change (`ClearItems` when the world loads).
//! - **Falling** (`MoveItems`, ZzzObject.cpp:6344-6368, every 25 Hz tick): `Position[2] += Gravity`,
//!   `Gravity -= 6`, until 30 above the terrain (70 for weapons); while falling it turns
//!   (`Angle[0]`, or `Angle[1]` for shields, = `-Gravity · 10`), on landing it lies as `ItemAngle`
//!   says. Drawn interpolated between ticks.
//! - **Drawing** (`RenderItems`, 6389-6470): the item's model (or the `MODEL_EVENT` one `CreateItem`
//!   picks), armor with the `player.bmd` skeleton and `ItemHeight`, with its glow
//!   (`RenderPartObject` with the item's level and options, `HideSkin`), lit by the terrain light plus
//!   `o->Light` (0.3; 0.2 for all and 1.5 for the one under the mouse once `SelectItem` runs). Zen is a
//!   pile of `sqrt(amount) / 2` coins (3 to 80) around its point (`RandomTable`). In Icarus items bob
//!   (`10 · sin((i · 1237 + WorldTime) · 0.002)`).
//! - **Names** (`CNewUINameWindow::RenderName`, NewUINameWindow.cpp:154-182, and `RenderItemName`): the
//!   one under the mouse at `(MouseX, MouseY - 15)`; while Alt is held, or after tapping it
//!   (`m_bShowItemName` toggles on each press, 72-78), every other visible one 15 above the projection
//!   of its point raised 30.
//! - **Dropping** (`CNewUIMyInventory::UpdateMouseEvent`, NewUIMyInventory.cpp:458-533): a left press on
//!   the game area (`CheckMouseIn(0, 0, GetScreenWidth(), 429)`) with an item held sends
//!   `SendRequestDropItem` for the tile under the mouse; refused with a message for valuable items
//!   (`IsHighValueItem`, `GlobalText[269]`) and the ones that cannot be dropped (`IsDropBan`, 1915),
//!   both sent back to their place; nothing happens with the shop, vault or Chaos Machine open nor
//!   for a Mark of Lord in the Illusion Temple. The item stays on the mouse until `ReceiveDropItem`
//!   (inventory.rs).
//! - **Picking up**: a click on an item walks to its tile (`MOVEMENT_GET`, ZzzInterface.cpp:7572-7584)
//!   and, on arriving within 1.5 tiles of it (`CheckTile`), sends `SendRequestGetItem` (3368-3394);
//!   with the inventory full it says so (375) and the item jumps. Space picks up the first visible
//!   item within 300 units right away (`CNewUIHotKey::AutoGetItem`, NewUIHotKey.cpp:429-459). One
//!   request at a time (`SendGetItem`). The answer (`ReceiveGetItem`, WSclient.cpp:5607-5661) puts the
//!   item in the inventory (inventory.rs) or sets the zen (move_window.rs), writes "<name> Obtained"
//!   (or "<n> Zen Obtained") and plays `SOUND_GET_ITEM01` or the jewel sounds; with `FOR_WORK` the
//!   item also vanishes at once (`Items[ItemKey].Object.Live = false`).
//!
//! Not done yet: the sparkles (`CreateShiny`), the medals' landing effects (`MODEL_EVENT+8/9`), the
//! Hellas and Kanturu heights, the hover's `LightEnable = false` (here it is only the 1.5 light), the
//! exact `OBB` (a ±30 box around the point), the harmony check before dropping
//! (`Jewel_Of_Harmony_Option`, [`Item`] does not keep it) and the hero's death (`Hero->Dead`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use bevy::ui::{UiScale, UiSystems};
use mu_data::items::ground::{GroundName, ground_name, obtained_name};
use mu_data::items::model::{self, GroundModel, GroundPose, ScreenPose};
use mu_data::items::{Item, group, kind, price};
use mu_protocol::game::{ClientMessage, DroppedItem, ServerMessage, pick_up};

use crate::chat_log::MsgType;
use crate::inventory::{Inventory, InventorySet, Source};
use crate::item_draw::{self, PassAnim};
use crate::item_view::{ItemData, ViewCache};
use crate::model_light::ModelMaterial;
use crate::scene::GameScene;
use crate::sound::SoundQueue;
use crate::terrain::{LoadedTerrain, MU_SCALE};

pub struct GroundPlugin;

impl Plugin for GroundPlugin {
    fn build(&self, app: &mut App) {
        let game = || in_state(GameScene::Game);
        app.init_resource::<GroundItems>()
            .init_resource::<GetRequest>()
            .add_systems(OnEnter(GameScene::Game), spawn_label_root)
            .add_systems(OnExit(GameScene::Game), (crate::scene::despawn::<LabelRoot>, crate::scene::despawn::<GroundEntity>, leave_game))
            .add_systems(PreUpdate, from_server.after(crate::net::NetSystems).before(crate::move_window::net_messages))
            .add_systems(
                PreUpdate,
                space_key.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(game()),
            )
            .add_systems(Update, drop_on_ground.after(InventorySet::Mouse).before(InventorySet::Draw).run_if(game()))
            .add_systems(Update, (get_item.after(crate::character::HeroWalk), move_items, draw_items).chain().run_if(game()))
            .add_systems(
                PostUpdate,
                layout_labels.after(crate::camera::CameraMoved).before(UiSystems::Prepare).run_if(game()),
            );
    }
}

// --- State ----------------------------------------------------------------------------------------

/// `GlobalText` (Text_eng.bmd).
mod txt {
    pub const EXPENSIVE: &str = "You are not allowed to drop this expensive item"; // 269
    pub const INVENTORY_FULL: &str = "Inventory is full."; // 375
    pub const ZEN: &str = "Zen"; // 224
    pub const OBTAINED: &str = "Obtained"; // 918
    pub const CANNOT_DROP: &str = "This item cannot be dropped."; // 1915
}

/// `ItemObjectAttribute`'s `o->Light` (ZzzObject.cpp:5288), and the ones `SelectItem` sets
/// (ZzzInterface.cpp:7646-7675).
const LIGHT_START: f32 = 0.3;
const LIGHT_SELECT_ALL: f32 = 0.2;
const LIGHT_HOVERED: f32 = 1.5;

/// One of `Items[]`.
pub struct GroundItem {
    pub item: Item,
    /// `Item.Level` of a zen drop: the amount.
    pub zen: Option<u32>,
    pub tile: (u8, u8),
    /// `CreateFlag`: it falls with its sound.
    fresh: bool,
    /// `o->Position` (MU units) and `o->Angle` now and one tick ago (to interpolate); `None` until
    /// the terrain is there.
    pos: Option<(Vec3, Vec3)>,
    angle: ([f32; 3], [f32; 3]),
    gravity: f32,
    pose: GroundPose,
    /// `o->Light`.
    light: f32,
    name: GroundName,
    drawn: Option<Drawn>,
}

/// The entities of a drawn item.
struct Drawn {
    root: Entity,
    /// One per coin of a zen pile (one for anything else), with its offset from the point.
    copies: Vec<(Entity, Vec3)>,
    /// Materials lit by the place (`follows_light`), and the light they have.
    lit: Vec<Handle<ModelMaterial>>,
    light: Vec3,
    animated: Vec<PassAnim>,
}

impl GroundItem {
    fn new(d: &DroppedItem, names: &dyn Fn(usize) -> Option<String>) -> Option<Self> {
        let item = Item::from_bytes(&d.item)?;
        let zen = d.zen();
        let pose = model::ground_pose(&item);
        Some(Self {
            item,
            zen,
            tile: (d.x, d.y),
            fresh: d.fresh,
            pos: None,
            angle: (pose.angle, pose.angle),
            gravity: 0.0,
            pose,
            light: LIGHT_START,
            name: ground_name(&item, zen, names),
            drawn: None,
        })
    }

    /// `o->Position` (MU), if placed.
    pub fn position(&self) -> Option<Vec3> {
        self.pos.map(|p| p.1)
    }

    /// `CreateItem`'s height: 180 above the terrain with `Gravity` 20 when fresh (the medals 3 with 50,
    /// 6293-6312), on the terrain otherwise.
    fn place(&mut self, terrain: &mu_data::terrain::Terrain) {
        let (x, y) = (self.tile.0 as f32 + 0.5, self.tile.1 as f32 + 0.5);
        let ground = terrain.height_at(x, y);
        let medal = matches!(model::ground_model(&self.item).0, GroundModel::Event(8 | 9));
        let (z, g) = match (self.fresh, medal) {
            (true, true) => (ground + 3.0, 50.0),
            (true, false) => (ground + 180.0, 20.0),
            (false, _) => (ground, 0.0),
        };
        let p = Vec3::new(x * MU_SCALE, y * MU_SCALE, z);
        self.pos = Some((p, p));
        self.gravity = g;
    }

    /// One `MoveItems` tick.
    fn tick(&mut self, terrain: &mu_data::terrain::Terrain) {
        let Some((_, mut p)) = self.pos else { return };
        let before = p;
        self.angle.0 = self.angle.1;
        p.z += self.gravity;
        self.gravity -= 6.0;
        let height = terrain.height_at(p.x / MU_SCALE, p.y / MU_SCALE) + model::ground_lift(&self.item);
        if p.z <= height {
            p.z = height;
            self.angle.1 = self.pose.angle;
        } else if model::falls_on_second_axis(&self.item) {
            self.angle.1[1] = -self.gravity * 10.0;
        } else {
            self.angle.1[0] = -self.gravity * 10.0;
        }
        self.pos = Some((before, p));
    }
}

/// `Items[]`, `SendGetItem` and `ItemKey`.
#[derive(Resource, Default)]
pub struct GroundItems {
    pub items: BTreeMap<u16, GroundItem>,
    /// `SendGetItem`: the drop whose pick-up waits for its answer.
    pending_get: Option<u16>,
    /// `Items[ItemKey].Item` of the last request (for the "Obtained" line).
    last_get: Option<Item>,
    /// `m_bShowItemName`.
    show_names: bool,
    /// Accumulated 25 Hz tick fraction.
    acc: f32,
    /// `RandomTable` (Winmain.cpp:1508-1510: `rand() % 360` after `srand(time)`).
    random: Vec<i32>,
}

impl GroundItems {
    /// `ReceiveCreateItemViewport`: returns the sound of a fresh drop and the entity of the item that
    /// had that key before, if drawn (`CreateItem` overwrites `Items[Key]`).
    fn create(&mut self, d: &DroppedItem, names: &dyn Fn(usize) -> Option<String>) -> (Option<(&'static str, bool)>, Option<Entity>) {
        let Some(g) = GroundItem::new(d, names) else { return (None, None) };
        let sound = d.fresh.then(|| drop_sound(&g.item));
        let old = self.items.insert(d.id, g).and_then(|o| o.drawn).map(|o| o.root);
        (sound, old)
    }

    /// `SendRequestGetItem(key)`: only while no other pick-up waits for its answer.
    fn request(&mut self, id: u16, net: &mut crate::net::Net) -> bool {
        if self.pending_get.is_some() || !net.connected() {
            return false;
        }
        self.pending_get = Some(id);
        self.last_get = self.items.get(&id).map(|g| g.item);
        net.send(ClientMessage::PickUpItem { id });
        true
    }

    /// `SelectItem` (ZzzInterface.cpp:7646-7675): every item gets `o->Light` 0.2 and the first one the
    /// mouse ray crosses 1.5. Its `OBB` is approximated by the ±30 `BoundingBox` around its point.
    pub fn select(&mut self, ray: (Vec3, Vec3)) -> Option<u16> {
        for g in self.items.values_mut() {
            g.light = LIGHT_SELECT_ALL;
        }
        let (id, g) = self.items.iter_mut().find(|(_, g)| {
            g.position().is_some_and(|p| crate::operate::ray_hits_box(ray.0, ray.1, p - Vec3::splat(30.0), p + Vec3::splat(30.0)))
        })?;
        g.light = LIGHT_HOVERED;
        Some(*id)
    }

    fn clear(&mut self, commands: &mut Commands) {
        for g in self.items.values_mut() {
            if let Some(d) = g.drawn.take() {
                commands.entity(d.root).despawn();
            }
        }
        self.items.clear();
        self.pending_get = None;
    }

    fn random_table(&mut self) -> &[i32] {
        if self.random.is_empty() {
            // MSVC's `rand` with a time seed.
            let mut seed = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_secs() as u32);
            self.random = (0..100)
                .map(|_| {
                    seed = seed.wrapping_mul(214_013).wrapping_add(2_531_011);
                    ((seed >> 16) & 0x7FFF) as i32 % 360
                })
                .collect();
        }
        &self.random
    }
}

/// `CreateItem`'s drop sound (ZzzObject.cpp:6116-6136): the zen without an object, the rest with it.
fn drop_sound(item: &Item) -> (&'static str, bool) {
    let t = item.kind();
    let p = |i| kind(group::POTION, i);
    if t == p(15) {
        ("SOUND_DROP_GOLD01", false)
    } else if [p(13), p(14), p(16), kind(group::WING, 15), p(22), p(31)].contains(&t) {
        ("SOUND_JEWEL01", true)
    } else if t == p(41) {
        ("SOUND_JEWEL02", true)
    } else {
        ("SOUND_DROP_ITEM01", true)
    }
}

/// `ReceiveGetItem`'s sound for the received item (WSclient.cpp:5650-5657).
fn get_sound(item: Option<&Item>) -> &'static str {
    let t = item.map_or(usize::MAX, Item::kind);
    let p = |i| kind(group::POTION, i);
    // `INDEX_COMPILED_CELE` / `_SOUL`: `ITEM_WING+30` / `+31`.
    if [p(13), p(14), p(16), kind(group::WING, 15), p(22), kind(group::WING, 30), kind(group::WING, 31), p(31)].contains(&t) {
        "SOUND_JEWEL01"
    } else if t == p(41) {
        "SOUND_JEWEL02"
    } else {
        "SOUND_GET_ITEM01"
    }
}

/// `Action(c, o, ...)` with `MOVEMENT_GET` waiting to happen: (`ItemKey`, `g_bAutoGetItem`). Set by
/// the click when there is no path and by `walk` when the hero arrives (character.rs), or by Space.
#[derive(Resource, Default)]
pub struct GetRequest(pub Option<(u16, bool)>);

/// What the click on a ground item needs, for `hero_input` (character.rs).
#[derive(bevy::ecs::system::SystemParam)]
pub struct GroundClick<'w> {
    items: Res<'w, GroundItems>,
    hovered: Res<'w, crate::names::Hovered>,
    request: ResMut<'w, GetRequest>,
}

impl GroundClick<'_> {
    /// `SelectedItem`: its key and tile (`Position / TERRAIN_SCALE`).
    pub fn clicked_item(&self) -> Option<(u16, (usize, usize))> {
        let id = self.hovered.item?;
        let p = self.items.items.get(&id)?.position()?;
        Some((id, ((p.x / MU_SCALE).max(0.0) as usize, (p.y / MU_SCALE).max(0.0) as usize)))
    }

    /// `Action(c, o, true)` now.
    pub fn act(&mut self, id: u16) {
        self.request.0 = Some((id, false));
    }
}

// --- Server ----------------------------------------------------------------------------------------

/// The ground item packets and `ReceiveGetItem`'s messages and sound. Before move_window.rs sets the
/// zen, to know how much was picked up.
#[allow(clippy::too_many_arguments)]
fn from_server(
    mut commands: Commands,
    mut from: MessageReader<crate::net::FromServer>,
    mut ground: ResMut<GroundItems>,
    data: Res<ItemData>,
    info: Option<Res<crate::account::HeroInfo>>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut sound: ResMut<SoundQueue>,
) {
    let names = |t: usize| data.table.as_ref().and_then(|tb| tb.by_kind(t)).map(|a| a.name.clone());
    let mut money = info.as_ref().map(|i| i.0.money);
    for m in from.read() {
        match &m.0 {
            ServerMessage::ItemsDropped(list) => {
                for d in list {
                    let (fresh, old) = ground.create(d, &names);
                    if let Some(e) = old {
                        commands.entity(e).despawn();
                    }
                    match fresh {
                        Some((s, true)) => sound.play_object(s),
                        Some((s, false)) => sound.play(s),
                        None => {}
                    }
                }
            }
            ServerMessage::ItemDropRemoved(ids) => {
                for id in ids {
                    // `Items[Key].Object.Live = false`.
                    if let Some(d) = ground.items.remove(id).and_then(|g| g.drawn) {
                        commands.entity(d.root).despawn();
                    }
                }
            }
            ServerMessage::MapChanged { is_map_change: true, .. } => ground.clear(&mut commands),
            // `ReceiveGetItem`: every C3 22 ends the pending request (`SendGetItem = -1`).
            // `NOT_GET_ITEM` does nothing else, nor OpenMU's 4-byte 0xFE (zen limit), which carries no zen.
            ServerMessage::ItemPickUpFailed { reason } if *reason != pick_up::STACKED => {
                ground.pending_get = None;
            }
            // `GET_ITEM_MULTI` (stacked; its packet carries no item, the sound is the plain one) or into
            // a slot: "<name> Obtained" with the requested item's name.
            ServerMessage::ItemPickUpFailed { .. } | ServerMessage::ItemPickedUp { .. } => {
                let pending = ground.pending_get.take();
                if let Some(name) = ground.last_get.map(|i| obtained_name(&i, &names)) {
                    chat.add("", &format!("{name} {}", txt::OBTAINED), MsgType::System, MsgType::All, &metrics);
                }
                let received = match &m.0 {
                    ServerMessage::ItemPickedUp { item, .. } => Item::from_bytes(item),
                    _ => None,
                };
                sound.play(get_sound(received.as_ref()));
                vanish(&mut ground, pending, &mut commands);
            }
            ServerMessage::InventoryMoneyUpdate { money: new } => {
                let pending = ground.pending_get.take();
                // `getGold = Gold - backupGold`: only when it grew.
                if let Some(old) = money
                    && *new > old
                {
                    chat.add("", &format!("{} {} {}", new - old, txt::ZEN, txt::OBTAINED), MsgType::System, MsgType::All, &metrics);
                }
                money = Some(*new);
                sound.play(get_sound(None));
                vanish(&mut ground, pending, &mut commands);
            }
            _ => {}
        }
    }
}

/// `Items[ItemKey].Object.Live = false` (`FOR_WORK`, WSclient.cpp:5645-5647), only for the item this
/// answer is about (the original also hides the last requested one when the zen changes for another
/// reason; the server removes it with C2 21 anyway).
fn vanish(ground: &mut GroundItems, pending: Option<u16>, commands: &mut Commands) {
    if let Some(d) = pending.and_then(|id| ground.items.remove(&id)).and_then(|g| g.drawn) {
        commands.entity(d.root).despawn();
    }
}

fn leave_game(mut ground: ResMut<GroundItems>, mut request: ResMut<GetRequest>) {
    // The entities go with `GroundEntity`.
    ground.items.clear();
    ground.pending_get = None;
    ground.show_names = false;
    request.0 = None;
}

// --- Dropping -------------------------------------------------------------------------------------

/// `CNewUIMyInventory::UpdateMouseEvent`'s drop (NewUIMyInventory.cpp:458-533). After the inventory's
/// own mouse handling: a press on the grid or the equipment never gets here (they are inside the
/// window, outside `GetScreenWidth()`).
#[allow(clippy::too_many_arguments)]
fn drop_on_ground(
    windows: Query<&Window>,
    cameras: Query<(&Camera, &GlobalTransform), With<IsDefaultUiCamera>>,
    terrain: Option<Res<LoadedTerrain>>,
    mouse: Res<ButtonInput<MouseButton>>,
    args: Res<crate::Args>,
    (game_ui, move_window, char_info): (Res<crate::options::GameUi>, Res<crate::move_window::MoveWindow>, Res<crate::char_info::CharInfoWindow>),
    dialog: Res<crate::npc::NpcDialog>,
    hero: Option<Res<crate::account::Hero>>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut inv: ResMut<Inventory>,
    mut net: ResMut<crate::net::Net>,
) {
    if !inv.open || !mouse.just_pressed(MouseButton::Left) || game_ui.modal() {
        return;
    }
    let Some(picked) = inv.picked else { return };
    let Ok(window) = windows.single() else { return };
    let Some(c) = crate::mainframe::cursor_virtual(window) else { return };
    // `GetScreenWidth()`: 640 - 190 with the inventory, 640 - 380 with the character window too.
    let width = 640.0 - 190.0 + inv.dx;
    if !(0.0..width).contains(&c.x) || !(0.0..429.0).contains(&c.y) {
        return;
    }
    // Windows above it take the click first.
    if game_ui.blocks(c) || move_window.blocks(c) || chat.blocks(c) || char_info.blocks(c) {
        return;
    }
    use crate::npc::WindowKind::*;
    // NPC shop, vault, Chaos Machine, Devil Square and Blood Castle (no trade or personal shops yet).
    if matches!(dialog.kind(), Some(Shop | Vault | ChaosMachine | DevilSquare | BloodCastle)) {
        return;
    }
    let name = hero.as_ref().map_or(String::new(), |h| h.name.clone());
    let mut error = |text: &str| {
        chat.add(&name, text, MsgType::Error, MsgType::All, &metrics);
    };
    if price::is_high_value_item(&picked.item) {
        error(txt::EXPENSIVE);
        inv.backup_picked();
        return;
    }
    if price::is_drop_ban(&picked.item) {
        error(txt::CANNOT_DROP);
        inv.backup_picked();
        return;
    }
    // `ITEM_POTION+28` in the Illusion Temple (`IsCursedTemple`: `WD_45CURSEDTEMPLE_LV1..LV6`,
    // folders 46 to 51).
    if picked.item.kind() == kind(group::POTION, 28) && (46..=51).contains(&args.world) {
        return;
    }
    // `RenderTerrainTile(SelectXF, SelectYF, ...)`: the tile under the mouse, if any.
    let hit = cameras.single().ok().zip(terrain.as_deref()).and_then(|((cam, tf), t)| {
        let ray = cam.viewport_to_world(tf, window.cursor_position()?).ok()?;
        crate::character::raycast_terrain(&t.0, ray)
    });
    let Some(hit) = hit else { return };
    let (tx, ty) = (hit.x.clamp(0.0, 255.0) as u8, hit.y.clamp(0.0, 255.0) as u8);
    let slot = match picked.from {
        // TODO: not while the hero is dead (`Hero->Dead == false`): there is no death yet.
        Source::Grid { .. } | Source::Equipment(_) => crate::inventory::source_slot(picked.from),
        Source::Vault { .. } => return,
    };
    // One drop at a time (the original sends again on every click while the item is held).
    if inv.drop_pending.is_some() || !net.connected() {
        return;
    }
    info!("suelo: tira el objeto del hueco {slot} en ({tx}, {ty})");
    net.send(ClientMessage::DropItem { x: tx, y: ty, slot });
    inv.drop_pending = Some(slot);
}

// --- Picking up -----------------------------------------------------------------------------------

/// `CNewUIHotKey::AutoGetItem` (NewUIHotKey.cpp:429-459): Space, with nothing held, the chat closed
/// and the mouse in the game area, picks up the first visible item within 300 units of the hero.
#[allow(clippy::too_many_arguments)]
fn space_key(
    keys: Res<ButtonInput<KeyCode>>,
    windows: Query<&Window>,
    game_ui: Res<crate::options::GameUi>,
    chat: Res<crate::chat::Chat>,
    inv: Res<Inventory>,
    ground: Res<GroundItems>,
    players: Query<&crate::character::Player>,
    terrain: Option<Res<LoadedTerrain>>,
    mut request: ResMut<GetRequest>,
) {
    if !keys.just_pressed(KeyCode::Space) || inv.picked.is_some() || chat.input.open || game_ui.modal() {
        return;
    }
    let width = if inv.open { 640.0 - 190.0 + inv.dx } else { 640.0 };
    let in_area = windows.single().ok().and_then(crate::mainframe::cursor_virtual).is_some_and(|c| (0.0..width).contains(&c.x) && (0.0..429.0).contains(&c.y));
    let Ok(p) = players.single() else { return };
    if !in_area {
        return;
    }
    let ground_z = terrain.as_ref().map_or(0.0, |t| t.0.height_at(p.pos.x, p.pos.y));
    let hero = Vec3::new(p.pos.x * MU_SCALE, p.pos.y * MU_SCALE, ground_z);
    if let Some((id, _)) = ground.items.iter().find(|(_, g)| g.position().is_some_and(|q| (q - hero).length() < 300.0)) {
        request.0 = Some((*id, true));
    }
}

/// `Action(c, o, ...)` with `MOVEMENT_GET` (ZzzInterface.cpp:3368-3394): within 1.5 tiles of the item's
/// tile (`CheckTile`, 1375-1388; not for Space), zen is asked for at once; anything else only with
/// room in the inventory (`FindEmptySlot`), otherwise "Inventory is full." and the item jumps.
#[allow(clippy::too_many_arguments)]
fn get_item(
    mut request: ResMut<GetRequest>,
    mut ground: ResMut<GroundItems>,
    data: Res<ItemData>,
    inv: Res<Inventory>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&crate::character::Player>,
    mut timer: ResMut<crate::character::MouseTimer>,
    hero: Option<Res<crate::account::Hero>>,
    (mut chat, metrics): (ResMut<crate::chat::Chat>, Res<crate::chat::TextMetrics>),
    mut net: ResMut<crate::net::Net>,
) {
    let Some((id, auto)) = request.0.take() else { return };
    let Some((pos, zen, item)) = ground.items.get(&id).and_then(|g| Some((g.position()?, g.zen, g.item))) else { return };
    let (tx, ty) = ((pos.x / MU_SCALE).floor(), (pos.y / MU_SCALE).floor());
    if !auto {
        let Ok(p) = players.single() else { return };
        // `o->Position - (TargetX · TERRAIN_SCALE + TERRAIN_SCALE · 0.5, ...)`.
        let d = Vec2::new(p.pos.x - (tx + 0.5), p.pos.y - (ty + 0.5)) * MU_SCALE;
        if d.length() > MU_SCALE * 1.5 {
            return;
        }
        timer.max = 6;
    }
    if zen.is_some() {
        ground.request(id, &mut net);
    } else if inv.find_empty(&data, &item).is_none() {
        let name = hero.as_ref().map_or(String::new(), |h| h.name.clone());
        chat.add(&name, txt::INVENTORY_FULL, MsgType::System, MsgType::All, &metrics);
        // `Position[2] = RequestTerrainHeight + 3`, `Gravity = 50`.
        if let (Some(t), Some(g)) = (terrain.as_deref(), ground.items.get_mut(&id))
            && let Some((_, p)) = g.pos.as_mut()
        {
            p.z = t.0.height_at(p.x / MU_SCALE, p.y / MU_SCALE) + 3.0;
            g.gravity = 50.0;
        }
    } else {
        ground.request(id, &mut net);
    }
}

// --- Movement and drawing -------------------------------------------------------------------------

/// The 25 Hz `MoveItems`.
fn move_items(time: Res<Time>, terrain: Option<Res<LoadedTerrain>>, mut ground: ResMut<GroundItems>) {
    let Some(t) = terrain else { return };
    let ground = &mut *ground;
    for g in ground.items.values_mut().filter(|g| g.pos.is_none()) {
        g.place(&t.0);
    }
    ground.acc = (ground.acc + time.delta_secs() * crate::fx::sim::TICKS_PER_SECOND).min(3.0);
    while ground.acc >= 1.0 {
        ground.acc -= 1.0;
        for g in ground.items.values_mut() {
            g.tick(&t.0);
        }
    }
}

/// The root of a ground item's entities.
#[derive(Component)]
struct GroundEntity;

/// `RenderItems`: spawns what is missing and places everything.
#[allow(clippy::too_many_arguments)]
fn draw_items(
    mut commands: Commands,
    args: Res<crate::Args>,
    time: Res<Time<Real>>,
    terrain: Option<Res<LoadedTerrain>>,
    data: Res<ItemData>,
    opts: Res<crate::options::GameOptions>,
    mut cache: ResMut<ViewCache>,
    mut ground: ResMut<GroundItems>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ModelMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut transforms: Query<&mut Transform, With<GroundCopy>>,
    mut roots: Query<&mut Transform, (With<GroundEntity>, Without<GroundCopy>)>,
) {
    let Some(t) = terrain else { return };
    let world_time = time.elapsed_secs() * 1000.0;
    let alpha = ground.acc.clamp(0.0, 1.0);
    let random = ground.random_table().to_vec();
    let size = mu_data::terrain::SIZE;
    for (&id, g) in ground.items.iter_mut() {
        let Some((before, now)) = g.pos else { continue };
        if g.drawn.as_ref().is_none_or(|d| d.copies.is_empty()) {
            if let Some(old) = g.drawn.take() {
                commands.entity(old.root).despawn();
            }
            g.drawn = spawn_item(&mut commands, &args.data, id, g, &random, &data, opts.render_level, &mut cache, (&mut meshes, &mut materials, &mut images), world_time);
        }
        let Some(d) = g.drawn.as_mut() else { continue };
        let mut p = before.lerp(now, alpha);
        let angle: [f32; 3] = std::array::from_fn(|k| g.angle.0[k] + (g.angle.1[k] - g.angle.0[k]) * alpha);
        // `WD_10HEAVEN` (Icarus, folder 11): they bob.
        if args.world == 11 {
            p.z += 10.0 * ((id as f32 * 1237.0 + world_time) * 0.002).sin();
        }
        if let Ok(mut tf) = roots.get_mut(d.root) {
            let to_bevy = Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2);
            tf.translation = to_bevy * p / MU_SCALE;
        }
        for (e, offset) in &d.copies {
            if let Ok(mut tf) = transforms.get_mut(*e) {
                *tf = crate::objects::mu_transform(*offset, Vec3::from_array(angle), g.pose.scale);
            }
        }
        // `RequestTerrainLight(Position) + o->Light`.
        let (x, y) = (((p.x / MU_SCALE).max(0.0) as usize).min(size - 1), ((p.y / MU_SCALE).max(0.0) as usize).min(size - 1));
        let light = Vec3::from_array(t.0.light[y * size + x]) + Vec3::splat(g.light);
        if light != d.light {
            d.light = light;
            for h in &d.lit {
                if let Some(mut m) = materials.get_mut(h) {
                    m.extension.light = light.extend(1.0);
                }
            }
        }
        for a in &d.animated {
            a.update(&mut materials, world_time);
        }
    }
}

/// A coin of a zen pile (or the only copy of any other item).
#[derive(Component)]
struct GroundCopy;

/// The model, its passes and, for zen, the pile (`RenderItems`, ZzzObject.cpp:6420-6440).
#[allow(clippy::too_many_arguments)]
fn spawn_item(
    commands: &mut Commands,
    data_dir: &Path,
    id: u16,
    g: &GroundItem,
    random: &[i32],
    data: &ItemData,
    render_level: u8,
    cache: &mut ViewCache,
    (meshes, materials, images): (&mut Assets<Mesh>, &mut Assets<ModelMaterial>, &mut Assets<Image>),
    world_time: f32,
) -> Option<Drawn> {
    let (m, level) = model::ground_model(&g.item);
    // What `RenderPartObject` gets: the drawn model with the item's level and options (zen: its
    // `Level` is the amount, so `(Level >> 3) & 15` of it).
    let (path, drawn_item): (PathBuf, Item) = match m {
        GroundModel::Item(t) => {
            let base = Item { group: (t / mu_data::items::MAX_ITEM_INDEX) as u8, index: (t % mu_data::items::MAX_ITEM_INDEX) as u16, level, ..g.item };
            let base = match g.zen {
                Some(z) => Item { level: ((z >> 3) & 15) as u8, ..base },
                None => base,
            };
            (data.model_path(&base)?, base)
        }
        GroundModel::Event(n) => {
            let f = model::event_model_file(n)?;
            // The glow has no case for the event models: the item's own level and options.
            (Path::new(f.dir).join(f.name), g.item)
        }
    };
    let pose = ScreenPose {
        anchor: [0.0; 2],
        offset: [0.0; 3],
        angle: g.pose.angle,
        scale: g.pose.scale,
        body_height: g.pose.body_height,
        player_skeleton: g.pose.player_skeleton,
    };
    let parts = cache.parts(data_dir, &path, &pose, meshes, images)?;
    let num_meshes = parts.iter().map(|p| p.info.index + 1).max().unwrap_or(0);
    let ctx = mu_data::items::glow::ItemCtx { hide_skin: true, num_meshes, white_light: false };
    let passes = mu_data::items::glow::item_passes(&drawn_item, [1.0; 3], render_level, &ctx);
    let (blend_mesh, hidden_mesh) = match m {
        GroundModel::Item(_) => model::item_object_meshes(&drawn_item),
        GroundModel::Event(_) => (-1, -1),
    };
    let draw = mu_data::objects::render::ObjectDraw { blend_mesh, stream_mesh: -1, tex_coord: [mu_data::objects::render::Saw::ZERO; 2], hidden_mesh };
    let mut looks = Vec::new();
    let mut lit = Vec::new();
    let mut animated = Vec::new();
    for part in parts.iter() {
        let bitmap = |b| cache.bitmap(data_dir, b, images);
        for mp in item_draw::mesh_passes(&passes, &draw, &part.info, part.image.as_ref(), false, bitmap, materials, world_time) {
            animated.extend(mp.anim);
            if mp.follows_light {
                lit.push(mp.handle.clone());
            }
            looks.push((part.mesh.clone(), mp.handle));
        }
    }
    // The zen pile: `Count = (int)sqrtf(Level) / 2` (3 to 80) coins, the first at the point.
    let offsets: Vec<Vec3> = match g.zen {
        Some(z) => {
            let count = ((z as f32).sqrt() as i32 / 2).clamp(3, 80);
            let i = id as i32;
            std::iter::once(Vec3::ZERO)
                .chain((1..count).map(|j| {
                    let a = (random[((i * 20 + j) % 100) as usize] % 360) as f32;
                    let r = (random[((i + j) % 100) as usize] % (count + 20)) as f32;
                    // `VectorRotate((r, 0, 0), AngleMatrix((0, 0, a)))`.
                    let (s, c) = a.to_radians().sin_cos();
                    Vec3::new(r * c, r * s, 0.0)
                }))
                .collect()
        }
        None => vec![Vec3::ZERO],
    };
    let root = commands.spawn((Name::new(format!("Objeto en el suelo {id}")), GroundEntity, Transform::default(), Visibility::default())).id();
    let mut copies = Vec::new();
    for offset in offsets {
        let copy = commands
            .spawn((GroundCopy, crate::objects::mu_transform(offset, Vec3::from_array(g.pose.angle), g.pose.scale), Visibility::default()))
            .with_children(|c| {
                for (mesh, mat) in &looks {
                    c.spawn((Mesh3d(mesh.clone()), MeshMaterial3d(mat.clone())));
                }
            })
            .id();
        commands.entity(root).add_child(copy);
        copies.push((copy, offset));
    }
    Some(Drawn { root, copies, lit, light: Vec3::splat(-1.0), animated })
}

// --- Names ----------------------------------------------------------------------------------------

#[derive(Component)]
struct LabelRoot;

/// A name over an item (`None` = the one at the mouse).
#[derive(Component)]
struct Label(Option<u16>);

fn spawn_label_root(mut commands: Commands) {
    commands.spawn((
        Name::new("Nombres de los objetos"),
        LabelRoot,
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        // `CNewUINameWindow` is the lowest layer, like names.rs.
        GlobalZIndex(-2),
    ));
}

fn label_font(px: f32, bold: bool) -> TextFont {
    TextFont {
        font_size: FontSize::Px(px),
        weight: if bold { FontWeight::BOLD } else { FontWeight::NORMAL },
        font_smoothing: FontSmoothing::AntiAliased,
        ..default()
    }
}

/// `RenderName`'s item part: the hovered one at the mouse and, with Alt (held or toggled), the rest.
/// `RenderText(x, y, Name, 0, 0, RT3_WRITE_CENTER)`: centred on `x`, its top at `y`.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn layout_labels(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    (windows, scale, cameras): (Query<&Window>, Res<UiScale>, Query<(&Camera, &Transform), (With<Camera3d>, With<IsDefaultUiCamera>)>),
    hovered: Res<crate::names::Hovered>,
    mut ground: ResMut<GroundItems>,
    root: Query<Entity, With<LabelRoot>>,
    mut labels: Query<(Entity, &Label, &mut Node, &mut Visibility, &mut BackgroundColor, &ComputedNode, &Children)>,
    mut texts: Query<(&mut Text, &mut TextFont, &mut TextColor, &mut LineHeight)>,
) {
    let (Ok(window), Ok(root)) = (windows.single(), root.single()) else { return };
    // `IsPress(VK_MENU)` toggles `m_bShowItemName`; `IsRepeat(VK_MENU)` shows them while held.
    let alt = [KeyCode::AltLeft, KeyCode::AltRight];
    if keys.any_just_pressed(alt) {
        ground.show_names = !ground.show_names;
    }
    let all = ground.show_names || keys.any_pressed(alt);
    let rate = scale.0;
    let metrics = crate::names::Metrics::new(window.width(), rate);
    let camera = cameras.single().ok().map(|(c, tf)| (c, GlobalTransform::from(*tf)));
    let mouse = window.cursor_position().map(|c| c / rate);
    // (owner, text, point)
    let mut want: Vec<(Option<u16>, &GroundName, Vec2)> = Vec::new();
    if let (Some(id), Some(m)) = (hovered.item, mouse)
        && let Some(g) = ground.items.get(&id)
    {
        want.push((None, &g.name, Vec2::new(m.x, m.y - 15.0)));
    }
    if all {
        for (id, g) in &ground.items {
            if Some(*id) == hovered.item {
                continue;
            }
            let Some(p) = g.position() else { continue };
            // `Projection(Position + (0, 0, 30))` → `ScreenX`, `ScreenY`.
            let p = p + Vec3::Z * 30.0;
            let px = camera.and_then(|(c, gt)| c.world_to_viewport(&gt, Vec3::new(p.x, p.z, -p.y) / MU_SCALE).ok());
            if let Some(px) = px {
                want.push((Some(*id), &g.name, Vec2::new(px.x / rate, px.y / rate - 15.0)));
            }
        }
    }
    let mut used = Vec::new();
    for (owner, name, at) in &want {
        let existing = labels.iter_mut().find(|l| l.1.0 == *owner);
        let Some((e, _, mut node, mut vis, mut bg, computed, children)) = existing else {
            let text = commands
                .spawn((
                    Text::new(name.text.clone()),
                    label_font(metrics.font, name.bold),
                    LineHeight::Px(metrics.font),
                    TextColor(Color::srgb_u8(name.color[0], name.color[1], name.color[2])),
                    TextLayout::no_wrap(),
                ))
                .id();
            let label = commands
                .spawn((
                    Label(*owner),
                    Node { position_type: PositionType::Absolute, ..default() },
                    BackgroundColor(Color::srgb_u8(name.background[0], name.background[1], name.background[2])),
                    Visibility::Hidden,
                ))
                .add_child(text)
                .id();
            commands.entity(root).add_child(label);
            continue;
        };
        used.push(e);
        if let Some(&child) = children.first()
            && let Ok((mut t, mut f, mut c, mut lh)) = texts.get_mut(child)
        {
            if t.0 != name.text {
                t.0 = name.text.clone();
            }
            let font = label_font(metrics.font, name.bold);
            if *f != font {
                *f = font;
            }
            c.set_if_neq(TextColor(Color::srgb_u8(name.color[0], name.color[1], name.color[2])));
            lh.set_if_neq(LineHeight::Px(metrics.font));
        }
        bg.set_if_neq(BackgroundColor(Color::srgb_u8(name.background[0], name.background[1], name.background[2])));
        // The width measured in the previous frame (a new label shows on the next one).
        let w = computed.size().x * computed.inverse_scale_factor();
        let (l, t) = (Val::Px((at.x - w / 2.0).floor()), Val::Px(at.y.floor()));
        if node.left != l || node.top != t {
            node.left = l;
            node.top = t;
        }
        vis.set_if_neq(if w > 0.0 { Visibility::Inherited } else { Visibility::Hidden });
    }
    for (e, ..) in &labels {
        if !used.contains(&e) {
            // Gone (or not shown now): its node goes; it is made again when needed.
            commands.entity(e).despawn();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(t: usize) -> Option<String> {
        (t == kind(group::SWORD, 1)).then(|| "Kris".into()).or_else(|| (t == kind(group::POTION, 15)).then(|| "Zen".into()))
    }

    fn kris_drop(id: u16, fresh: bool) -> DroppedItem {
        DroppedItem { id, fresh, x: 130, y: 125, item: Item { level: 3, ..Item::new(group::SWORD, 1) }.to_bytes() }
    }

    /// A flat terrain 40 high.
    fn flat_terrain() -> mu_data::terrain::Terrain {
        use mu_data::terrain::{SIZE, Terrain, TerrainMapping};
        let n = SIZE * SIZE;
        Terrain {
            world: 1,
            dir: PathBuf::new(),
            heights: vec![40.0; n],
            light: vec![[0.5; 3]; n],
            mapping: TerrainMapping { version: 0, map_number: 0, layer1: vec![0; n], layer2: vec![255; n], alpha: vec![0; n] },
            attributes: vec![0; n],
            textures: Vec::new(),
        }
    }

    /// `ReceiveCreateItemViewport` / `ReceiveDeleteItemViewport` and the fresh drop's sound.
    #[test]
    fn spawn_and_remove() {
        let mut g = GroundItems::default();
        assert_eq!(g.create(&kris_drop(7, true), &names).0, Some(("SOUND_DROP_ITEM01", true)));
        assert_eq!(g.create(&kris_drop(8, false), &names).0, None);
        let zen = DroppedItem::money(9, true, 10, 10, 5000);
        assert_eq!(g.create(&zen, &names).0, Some(("SOUND_DROP_GOLD01", false)));
        assert_eq!(g.items.len(), 3);
        assert_eq!(g.items[&9].zen, Some(5000));
        assert_eq!(g.items[&7].name.text, "Kris +3");
        assert_eq!(g.items[&9].name.text, "Zen 5000");
        g.items.remove(&7);
        assert!(!g.items.contains_key(&7));
    }

    /// `CreateItem`'s heights and `MoveItems`: a fresh drop falls from 180 to 70 (a weapon) and lies
    /// down; one already there lands on the next tick.
    #[test]
    fn falling() {
        let t = flat_terrain();
        let mut g = GroundItems::default();
        g.create(&kris_drop(1, true), &names);
        g.create(&kris_drop(2, false), &names);
        for it in g.items.values_mut() {
            it.place(&t);
        }
        let h = t.height_at(130.5, 125.5);
        assert_eq!(g.items[&1].position().unwrap().z, h + 180.0);
        assert_eq!(g.items[&2].position().unwrap().z, h);
        // First tick: 180 + 20; it turns `-Gravity · 10` around x.
        let fresh = g.items.get_mut(&1).unwrap();
        fresh.tick(&t);
        assert_eq!(fresh.position().unwrap().z, h + 200.0);
        assert_eq!(fresh.angle.1[0], -(20.0 - 6.0) * 10.0);
        for _ in 0..40 {
            fresh.tick(&t);
        }
        assert_eq!(fresh.position().unwrap().z, h + 70.0);
        assert_eq!(fresh.angle.1, model::ground_pose(&fresh.item).angle);
        let lying = g.items.get_mut(&2).unwrap();
        lying.tick(&t);
        assert_eq!(lying.position().unwrap().z, h + 70.0);
    }

    /// `SelectItem`: lights and the item under the ray.
    #[test]
    fn select_lights() {
        let t = flat_terrain();
        let mut g = GroundItems::default();
        g.create(&kris_drop(1, false), &names);
        g.create(&DroppedItem { x: 140, ..kris_drop(2, false) }, &names);
        for it in g.items.values_mut() {
            it.place(&t);
        }
        assert_eq!(g.items[&1].light, LIGHT_START);
        let p = g.items[&2].position().unwrap();
        // Straight down onto item 2.
        assert_eq!(g.select((p + Vec3::Z * 500.0, -Vec3::Z)), Some(2));
        assert_eq!((g.items[&1].light, g.items[&2].light), (LIGHT_SELECT_ALL, LIGHT_HOVERED));
        assert_eq!(g.select((p + Vec3::new(0.0, 500.0, 500.0), -Vec3::Z)), None);
        assert_eq!(g.items[&2].light, LIGHT_SELECT_ALL);
    }

    /// `SendRequestGetItem` only while `SendGetItem == -1`.
    #[test]
    fn one_pick_up_at_a_time() {
        let mut g = GroundItems::default();
        g.create(&kris_drop(1, false), &names);
        g.create(&kris_drop(2, false), &names);
        let mut net = crate::net::Net::offline();
        // Without a connection nothing is sent.
        assert!(!g.request(1, &mut net));
        assert_eq!(g.pending_get, None);
        g.pending_get = Some(1);
        assert!(!g.request(2, &mut net));
        assert_eq!(g.pending_get, Some(1));
    }

    #[test]
    fn sounds() {
        assert_eq!(drop_sound(&Item::new(group::POTION, 13)), ("SOUND_JEWEL01", true));
        assert_eq!(drop_sound(&Item::new(group::POTION, 41)), ("SOUND_JEWEL02", true));
        assert_eq!(get_sound(Some(&Item::new(group::WING, 30))), "SOUND_JEWEL01");
        assert_eq!(get_sound(None), "SOUND_GET_ITEM01");
    }
}
