//! Inventario (`V` o `I`): `SEASON3B::CNewUIMyInventory` (NewUIMyInventory.cpp) con su rejilla de 8×8
//! `CNewUIInventoryCtrl` (NewUIInventoryCtrl.cpp).
//!
//! - **Abrir y cerrar**: `I` o `V` (`CNewUIHotKey::UpdateKeyEvent`, NewUIHotKey.cpp:155-163 y 198-204, con
//!   `SOUND_CLICK01`), el botón de inventario de la barra (NewUIMainFrameWindow.cpp:769-775), el botón de
//!   cerrar de abajo y Esc (`UpdateKeyEvent`, NewUIMyInventory.cpp:560-570). Al cerrar, lo que va en el
//!   ratón vuelve a su sitio (`ClosingProcess` → `BackupPickedItem`).
//! - **Dibujo**: `Create(..., 640 - 190, 0)` (NewUISystem.cpp:172), `RenderFrame`, `RenderInventoryDetails`,
//!   `RenderSetOption`/`RenderSocketOption`, `RenderButtons`, la rejilla (`CNewUIInventoryCtrl::Render`) y
//!   los huecos del equipo (`SetEquipmentSlotInfo`, `RenderEquippedItem`). Los objetos, en 3D
//!   (item_view.rs), encima; los cuadros de información, encima de todo.
//! - **Ratón**: coger un objeto al soltar el botón encima (`CNewUIInventoryCtrl::UpdateMouseEvent`) y
//!   dejarlo al soltar en una casilla donde cabe (`InventoryProcess`) o en un hueco del equipo libre donde
//!   se puede llevar (`EquipmentWindowProcess` + `IsEquipable`). El original pide el cambio al servidor
//!   (`SendRequestEquipmentItem`) y lo hace al contestar; sin servidor todavía, se hace aquí a la vez. Los
//!   objetos no se tiran al suelo (eso también lo hace el servidor): con uno en el ratón, un clic en el
//!   juego no hace nada.
//! - **Contenido**: sin servidor no hay inventario; `--inventario-prueba` pone uno de prueba (con +0 a +15,
//!   excelentes y un antiguo). [`Inventory::set_items`] es la entrada del paquete del inventario del S6
//!   (`F3 10`: hueco y los 12 bytes de cada objeto) para cuando llegue del servidor.
//!
//! No están: reparar, la tienda personal, los sets antiguos y los zócalos de la ventana, las teclas
//! rápidas de pociones, juntar objetos (joyas sobre objetos, `IsOverlayItem`) ni los colores de
//! durabilidad de la rejilla.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_data::items::info::{HeroView, TipLine, item_info};
use mu_data::items::{Item, group, slot};

use crate::Args;
use crate::item_view::{ItemData, ItemView, ItemViewSet, ItemViews};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Inventory>()
            .add_systems(OnEnter(GameScene::Game), (spawn_root, fill_test_inventory))
            .add_systems(
                OnExit(GameScene::Game),
                (crate::scene::despawn::<InventoryRoot>, |mut inv: ResMut<Inventory>| inv.close()),
            )
            .add_systems(
                PreUpdate,
                keys.after(bevy::input::InputSystems).after(crate::chat::ChatKeys).run_if(in_state(GameScene::Game)),
            )
            .add_systems(Update, (update_mouse, request_views, draw).chain().before(ItemViewSet).run_if(in_state(GameScene::Game)));
    }
}

// --- Medidas (pantalla de 640×480) -------------------------------------------------------------

/// `g_hFont` (como options.rs).
const FONT: f32 = crate::chat::FONT;
/// `m_Pos` (`Create(m_pNewUIMng, m_pNewUI3DRenderMng, 640-190, 0)`).
const POS: (f32, f32) = (640.0 - 190.0, 0.0);
/// `INVENTORY_WIDTH` × `INVENTORY_HEIGHT`.
const W: f32 = 190.0;
const H: f32 = 429.0;
/// La rejilla: `Create(..., x+15, y+200, 8, 8)` con casillas de `INVENTORY_SQUARE_WIDTH/HEIGHT` = 20.
pub const COLUMNS: usize = 8;
pub const ROWS: usize = 8;
const GRID: (f32, f32) = (POS.0 + 15.0, POS.1 + 200.0);
const SQUARE: f32 = 20.0;
/// `WND_TOP_EDGE`, `WND_LEFT_EDGE`, `WND_BOTTOM_EDGE`, `WND_RIGHT_EDGE` (NewUIInventoryCtrl.h:109-112).
const EDGE_TOP: f32 = 3.0;
const EDGE_LEFT: f32 = 4.0;
const EDGE_BOTTOM: f32 = 8.0;
const EDGE_RIGHT: f32 = 9.0;
/// Botones (`SetButtonInfo`): cerrar, reparar y tienda personal, de 36×29.
const BTN_EXIT: (f32, f32) = (POS.0 + 13.0, POS.1 + 391.0);
const BTN_REPAIR: (f32, f32) = (POS.0 + 50.0, POS.1 + 391.0);
const BTN_SHOP: (f32, f32) = (POS.0 + 87.0, POS.1 + 391.0);
const BTN_W: f32 = 36.0;
const BTN_H: f32 = 29.0;
/// `GlobalText[223, 225, 233, 1125, 989, 2651]`.
const TITLE: &str = "Inventory";
const TIP_EXIT: &str = "Close (I,V)";
const TIP_REPAIR: &str = "Repair (R)";
const TIP_SHOP: &str = "Open personal store(S)";
const SET_OPTION: &str = "[Set option]";
const SOCKET_OPTION: &str = "[Socket option]";

/// `SetEquipmentSlotInfo` (NewUIMyInventory.cpp:1062-1135): (x, y, ancho, alto) respecto a `m_Pos` y
/// su imagen de fondo, por hueco (`EQUIPMENT_*`).
const SLOTS: [(f32, f32, f32, f32, Tex); slot::COUNT] = [
    (15.0, 87.0, 46.0, 66.0, Tex::SlotWeaponR),
    (135.0, 87.0, 46.0, 66.0, Tex::SlotWeaponL),
    (75.0, 44.0, 46.0, 46.0, Tex::SlotHelm),
    (75.0, 87.0, 46.0, 66.0, Tex::SlotArmor),
    (75.0, 150.0, 46.0, 46.0, Tex::SlotPants),
    (15.0, 150.0, 46.0, 46.0, Tex::SlotGloves),
    (135.0, 150.0, 46.0, 46.0, Tex::SlotBoots),
    (120.0, 44.0, 61.0, 46.0, Tex::SlotWing),
    (15.0, 44.0, 46.0, 46.0, Tex::SlotFairy),
    (54.0, 87.0, 28.0, 28.0, Tex::SlotNecklace),
    (54.0, 150.0, 28.0, 28.0, Tex::SlotRing),
    (114.0, 150.0, 28.0, 28.0, Tex::SlotRing),
];

fn slot_rect(i: usize) -> (f32, f32, f32, f32) {
    let (x, y, w, h, _) = SLOTS[i];
    (POS.0 + x, POS.1 + y, w, h)
}

/// `CheckMouseIn(x, y, ancho, alto)`.
fn inside(m: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    m.x >= x && m.x < x + w && m.y >= y && m.y < y + h
}

// --- Estado ------------------------------------------------------------------------------------

/// Un objeto de la rejilla, con su casilla de arriba a la izquierda.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridItem {
    pub x: u8,
    pub y: u8,
    pub item: Item,
}

/// De dónde salió lo que va en el ratón (`GetOwnerInventory` o `ex_src_type == ITEM_EX_SRC_EQUIPMENT`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    Grid { x: u8, y: u8 },
    Equipment(u8),
}

/// `CNewUIPickedItem`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Picked {
    pub item: Item,
    pub from: Source,
}

/// `m_EventState` de la rejilla.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum CtrlEvent {
    #[default]
    None,
    Hover,
    Picking,
}

/// El inventario del personaje y la ventana.
#[derive(Resource, Default)]
pub struct Inventory {
    pub open: bool,
    pub grid: Vec<GridItem>,
    pub equipment: [Option<Item>; slot::COUNT],
    /// `CharacterMachine->Gold` (sin servidor; con él, el de `CharacterInformation`).
    pub zen: u32,
    pub picked: Option<Picked>,
    /// Cambia cada vez que cambia el equipo (para vestir al personaje).
    pub equipment_rev: u32,
    event: CtrlEvent,
    /// `m_iPointedSquareIndex` y `m_iPointedSlot`.
    pointed_square: Option<usize>,
    pointed_slot: Option<usize>,
    /// `m_pToolTipItem`: índice en `grid`.
    tooltip: Option<usize>,
    /// El botón de cerrar apretado encima (`BUTTON_STATE_DOWN`).
    exit_down: bool,
}

impl Inventory {
    /// `ItemAttribute.Width/Height` (1×1 si no está en la tabla).
    fn size(data: &ItemData, item: &Item) -> (u8, u8) {
        data.attr(item).map_or((1, 1), |a| (a.width.max(1), a.height.max(1)))
    }

    /// El objeto que ocupa la casilla (`m_pdwItemCheckBox`).
    fn owner(&self, data: &ItemData, x: usize, y: usize) -> Option<usize> {
        self.grid.iter().position(|g| {
            let (w, h) = Self::size(data, &g.item);
            (g.x as usize..g.x as usize + w as usize).contains(&x) && (g.y as usize..g.y as usize + h as usize).contains(&y)
        })
    }

    /// `CheckSlot`: cabe un objeto de `w`×`h` en (x, y). (El original cuenta casillas seguidas y, si se
    /// sale por la derecha, mira las de la fila siguiente; el servidor lo rechaza. Aquí, que no se salga.)
    fn fits(&self, data: &ItemData, x: i32, y: i32, w: u8, h: u8) -> bool {
        if x < 0 || y < 0 || x as usize + w as usize > COLUMNS || y as usize + h as usize > ROWS {
            return false;
        }
        (0..h as usize).all(|dy| (0..w as usize).all(|dx| self.owner(data, x as usize + dx, y as usize + dy).is_none()))
    }

    /// `FindEmptySlot`: la primera casilla (por filas) donde cabe.
    pub fn find_empty(&self, data: &ItemData, item: &Item) -> Option<(u8, u8)> {
        let (w, h) = Self::size(data, item);
        (0..ROWS as i32)
            .flat_map(|y| (0..COLUMNS as i32).map(move |x| (x, y)))
            .find(|&(x, y)| self.fits(data, x, y, w, h))
            .map(|(x, y)| (x as u8, y as u8))
    }

    /// Mete el objeto en la primera casilla libre (para el inventario de prueba).
    pub fn add(&mut self, data: &ItemData, item: Item) -> bool {
        match self.find_empty(data, &item) {
            Some((x, y)) => {
                self.grid.push(GridItem { x, y, item });
                true
            }
            None => false,
        }
    }

    /// El inventario que manda el servidor (`F3 10`): hueco (0-11 equipo, 12-75 rejilla) y los 12 bytes
    /// del objeto (`Item::from_bytes`).
    #[allow(dead_code)]
    pub fn set_items(&mut self, list: &[(u8, [u8; 12])]) {
        self.grid.clear();
        self.equipment = [None; slot::COUNT];
        self.picked = None;
        for (s, bytes) in list {
            let Some(item) = Item::from_bytes(bytes) else { continue };
            let s = *s as usize;
            if s < slot::COUNT {
                self.equipment[s] = Some(item);
            } else if s - slot::COUNT < COLUMNS * ROWS {
                let i = s - slot::COUNT;
                self.grid.push(GridItem { x: (i % COLUMNS) as u8, y: (i / COLUMNS) as u8, item });
            }
        }
        self.equipment_rev += 1;
    }

    /// `BackupPickedItem`: lo que va en el ratón vuelve a su sitio.
    fn backup_picked(&mut self) {
        let Some(p) = self.picked.take() else { return };
        match p.from {
            Source::Grid { x, y } => self.grid.push(GridItem { x, y, item: p.item }),
            Source::Equipment(s) => {
                self.equipment[s as usize] = Some(p.item);
                self.equipment_rev += 1;
            }
        }
        self.event = CtrlEvent::None;
    }

    fn close(&mut self) {
        self.backup_picked();
        self.open = false;
        self.tooltip = None;
        self.exit_down = false;
    }

    /// `g_pNewUISystem->Toggle(INTERFACE_INVENTORY)`.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.event = CtrlEvent::None;
        }
    }

    /// El ratón (pantalla de 640×480) cae en la ventana (`CheckMouseIn(m_Pos, 190, 429)`), o hay un
    /// objeto en el ratón (un clic en el juego lo tiraría al suelo): no se camina.
    pub fn blocks(&self, c: Vec2) -> bool {
        self.open && (inside(c, POS.0, POS.1, W, H) || (self.picked.is_some() && c.y < 429.0))
    }
}

/// `IsEquipable` (NewUIMyInventory.cpp:199-362) sin las mascotas ni los mapas especiales. Sin los
/// datos del personaje (sin servidor) no se miran fuerza, agilidad...
fn is_equipable(inv: &Inventory, data: &ItemData, hero: &HeroView, slot_index: usize, item: &Item) -> bool {
    let Some(attr) = data.attr(item) else { return false };
    let class = hero.class as usize;
    let rc = attr.require_class;
    // El Magic Gladiator lleva lo de Dark Wizard y Dark Knight a la vez.
    let mut ok = rc[class] != 0 || (class == 3 && rc[0] != 0 && rc[1] != 0);
    if rc[class] > hero.step.max(1) {
        return false;
    }
    if !ok {
        return false;
    }
    let s = slot_index as u8;
    ok = attr.slot == Some(s);
    if attr.slot == Some(slot::WEAPON_RIGHT) && s == slot::WEAPON_LEFT {
        // Dark Knight, Magic Gladiator y Rage Fighter: armas de una mano en la izquierda; la Summoner,
        // lo que no sea báculo.
        if matches!(class, 1 | 3 | 6) {
            ok = !attr.two_hand;
        } else if class == 5 && item.group != group::STAFF {
            ok = true;
        }
    } else if attr.slot == Some(slot::RING_RIGHT) && s == slot::RING_LEFT {
        ok = true;
    }
    // La Elfa con arco en la izquierda solo lleva flechas de ballesta (`ITEM_BOW+15`) en la derecha.
    if class == 2 && s == slot::WEAPON_RIGHT {
        if let Some(l) = inv.equipment[slot::WEAPON_LEFT as usize] {
            if l.group == group::BOW && l.index != 7 && item.kind() != mu_data::items::kind(group::BOW, 15) {
                ok = false;
            }
        }
    }
    // El Rage Fighter no lleva guantes.
    if class == 6 && s == slot::GLOVES {
        ok = false;
    }
    if !ok {
        return false;
    }
    let stats = mu_data::items::convert(attr, item, hero.level);
    if stats.require_level > hero.level {
        return false;
    }
    if let Some([str_, agi, vit, ene, cha]) = hero.stats {
        if stats.require_strength > str_
            || stats.require_dexterity > agi
            || stats.require_vitality > vit
            || stats.require_energy > ene
            || stats.require_charisma > cha
        {
            return false;
        }
    }
    true
}

/// El personaje que mira el inventario (`Hero->Class` y `CharacterAttribute`).
fn hero_view(hero: Option<&crate::account::Hero>, info: Option<&crate::account::HeroInfo>) -> HeroView {
    HeroView {
        class: hero.map_or(1, |h| h.class.index() as u8),
        // La clase aún no se sabe subida: siempre la primera.
        step: 1,
        level: hero.map_or(1, |h| h.level),
        stats: info.map(|i| {
            let i = &i.0;
            [i.strength as u32, i.agility as u32, i.vitality as u32, i.energy as u32, i.leadership as u32]
        }),
    }
}

/// `--inventario-prueba`: un inventario para mirar los objetos y su brillo.
fn fill_test_inventory(args: Res<Args>, data: Res<ItemData>, mut inv: ResMut<Inventory>) {
    if !args.test_inventory || !inv.grid.is_empty() || inv.equipment.iter().any(|e| e.is_some()) {
        return;
    }
    let it = |g: u8, i: u16, level: u8| Item { level, durability: 255, ..Item::new(g, i) };
    // Equipo de Dark Knight: espada +9 con suerte y habilidad, escudo +7, set de dragón +11 a +15
    // (la armadura, excelente), alas de Satán, anillo y colgante.
    let mut equip = [None; slot::COUNT];
    equip[slot::WEAPON_RIGHT as usize] = Some(Item { skill: true, luck: true, option: 3, ..it(group::SWORD, 5, 9) });
    equip[slot::WEAPON_LEFT as usize] = Some(it(group::SHIELD, 4, 7));
    equip[slot::HELM as usize] = Some(it(group::HELM, 1, 11));
    equip[slot::ARMOR as usize] = Some(Item { excellent: 0b10_0001, ..it(group::ARMOR, 1, 13) });
    equip[slot::PANTS as usize] = Some(it(group::PANTS, 1, 15));
    equip[slot::GLOVES as usize] = Some(it(group::GLOVES, 1, 9));
    equip[slot::BOOTS as usize] = Some(it(group::BOOTS, 1, 7));
    equip[slot::WING as usize] = Some(it(group::WING, 2, 0));
    equip[slot::AMULET as usize] = Some(it(group::HELPER, 12, 0));
    equip[slot::RING_RIGHT as usize] = Some(Item { option: 2, ..it(group::HELPER, 8, 0) });
    inv.equipment = equip;
    inv.equipment_rev += 1;
    let grid = [
        it(group::SWORD, 0, 0),
        it(group::SWORD, 1, 3),
        Item { excellent: 0b01_0100, ..it(group::SWORD, 3, 5) },
        it(group::AXE, 3, 7),
        it(group::MACE, 2, 9),
        it(group::SPEAR, 4, 11),
        Item { excellent: 0b11_0000, luck: true, ..it(group::BOW, 10, 13) },
        Item { excellent: 0b00_1001, ..it(group::STAFF, 2, 15) },
        // Antiguo (`ExtOption % 4` = 1).
        Item { ancient: 5, ..it(group::ARMOR, 5, 0) },
        it(group::HELM, 0, 13),
        it(group::SHIELD, 2, 15),
        it(group::POTION, 13, 0),
        it(group::POTION, 14, 0),
        it(group::WING, 15, 0),
        Item { durability: 3, ..it(group::POTION, 3, 0) },
        Item { excellent: 0b00_0010, ..it(group::HELPER, 9, 0) },
    ];
    for item in grid {
        if !inv.add(&data, item) {
            warn!("inventario de prueba: no cabe {},{}", item.group, item.index);
        }
    }
    inv.zen = 2_500_000;
    // Se abre solo, para verlo enseguida.
    inv.open = true;
    info!("inventario de prueba: {} objetos en la rejilla", inv.grid.len());
}

// --- Teclado y ratón ---------------------------------------------------------------------------

/// `I`/`V` abren o cierran (`CNewUIHotKey`, con las cajas de mensajes no llegan); Esc cierra (antes de
/// que el menú de sistema lo vea: la ventana está por encima en el orden de teclas del original).
fn keys(
    mut keys: ResMut<ButtonInput<KeyCode>>,
    game_ui: Res<crate::options::GameUi>,
    move_window: Res<crate::move_window::MoveWindow>,
    mut inv: ResMut<Inventory>,
    mut sound: ResMut<SoundQueue>,
) {
    if game_ui.modal() {
        return;
    }
    if keys.just_pressed(KeyCode::KeyI) || keys.just_pressed(KeyCode::KeyV) {
        inv.toggle();
        sound.play("SOUND_CLICK01");
    } else if inv.open
        && keys.just_pressed(KeyCode::Escape)
        && !game_ui.option_open()
        && !game_ui.window_menu_open()
        && !move_window.is_open()
    {
        inv.close();
        sound.play("SOUND_CLICK01");
        keys.clear_just_pressed(KeyCode::Escape);
    }
}

/// `CNewUIMyInventory::Update` + `UpdateMouseEvent` (NewUIMyInventory.cpp:447-558 y 653-675).
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    data: Res<ItemData>,
    game_ui: Res<crate::options::GameUi>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut inv: ResMut<Inventory>,
    mut sound: ResMut<SoundQueue>,
) {
    if !inv.open {
        return;
    }
    let Some(m) = windows.single().ok().and_then(crate::mainframe::cursor_virtual).filter(|_| !game_ui.modal()) else {
        return;
    };
    // `MouseX`, `MouseY`: enteros de la pantalla de 640×480.
    let m = m.floor();
    let hero = hero_view(hero.as_deref(), info.as_deref());
    let (pressed, released) = (mouse.pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    let inv = &mut *inv;

    // `Update`: el hueco del equipo bajo el ratón (`CheckMouseIn(x + 1, y, w - 4, h - 4)`).
    inv.pointed_slot = (0..slot::COUNT).find(|&i| {
        let (x, y, w, h) = slot_rect(i);
        inside(m, x + 1.0, y, w - 4.0, h - 4.0)
    });
    // `CNewUIInventoryCtrl::UpdateProcess`.
    let square = square_at(m);
    if square != inv.pointed_square {
        inv.pointed_square = square;
    }
    let pointed_item = inv.pointed_square.and_then(|s| inv.owner(&data, s % COLUMNS, s / COLUMNS));
    if pointed_item.is_none() {
        inv.event = CtrlEvent::None;
        inv.tooltip = None;
    }

    // `CNewUIInventoryCtrl::UpdateMouseEvent`.
    if inv.event == CtrlEvent::None && !pressed && pointed_item.is_some() {
        inv.event = CtrlEvent::Hover;
    } else if inv.event == CtrlEvent::Hover && released && inv.picked.is_none() {
        if let Some(i) = pointed_item {
            inv.event = CtrlEvent::Picking;
            let g = inv.grid.remove(i);
            inv.picked = Some(Picked { item: g.item, from: Source::Grid { x: g.x, y: g.y } });
            inv.tooltip = None;
            return;
        }
    } else if inv.event == CtrlEvent::Hover && !pressed && inv.picked.is_none() {
        if pointed_item != inv.tooltip {
            inv.tooltip = pointed_item;
        }
    }

    // `EquipmentWindowProcess`.
    if let (Some(s), true) = (inv.pointed_slot, released) {
        match inv.picked {
            Some(p) => {
                if inv.equipment[s].is_none() && is_equipable(inv, &data, &hero, s, &p.item) {
                    match p.from {
                        // Del mismo hueco: vuelve (`BackupPickedItem`).
                        Source::Equipment(from) if from as usize == s => inv.backup_picked(),
                        // `SendRequestEquipmentItem`: lo que haría el servidor al contestar.
                        _ => {
                            inv.equipment[s] = Some(p.item);
                            inv.picked = None;
                            inv.equipment_rev += 1;
                            inv.event = CtrlEvent::None;
                            sound.play("SOUND_GET_ITEM01");
                        }
                    }
                }
            }
            None => {
                if let Some(item) = inv.equipment[s].take() {
                    inv.picked = Some(Picked { item, from: Source::Equipment(s as u8) });
                    inv.equipment_rev += 1;
                }
            }
        }
        return;
    }

    // `InventoryProcess`: soltar en la rejilla.
    if inside(m, POS.0, POS.1, W, H)
        && released
        && let Some(p) = inv.picked
    {
        let (w, h) = Inventory::size(&data, &p.item);
        // `GetTargetPos`: el ratón menos media pieza (`(Width - 1) · 20 / 2`).
        let tx = m.x - ((w as i32 - 1) * SQUARE as i32 / 2) as f32;
        let ty = m.y - ((h as i32 - 1) * SQUARE as i32 / 2) as f32;
        if let Some(t) = square_at(Vec2::new(tx, ty)) {
            let (x, y) = ((t % COLUMNS) as u8, (t / COLUMNS) as u8);
            if inv.fits(&data, x as i32, y as i32, w, h) {
                if p.from == (Source::Grid { x, y }) {
                    inv.backup_picked();
                } else {
                    inv.grid.push(GridItem { x, y, item: p.item });
                    inv.picked = None;
                    inv.event = CtrlEvent::None;
                    sound.play("SOUND_GET_ITEM01");
                }
                return;
            }
        }
    }

    // `BtnProcess`: el botón de cerrar (se suelta encima) y la equis de arriba (`m_Pos + (169, 7)`, 13×12).
    let over_exit = inside(m, BTN_EXIT.0, BTN_EXIT.1, BTN_W, BTN_H);
    if mouse.just_pressed(MouseButton::Left) && inside(m, POS.0 + 169.0, POS.1 + 7.0, 13.0, 12.0) {
        inv.close();
        return;
    }
    if over_exit && mouse.just_pressed(MouseButton::Left) {
        inv.exit_down = true;
    }
    if !pressed {
        if inv.exit_down && over_exit && released {
            inv.close();
            sound.play("SOUND_CLICK01");
            return;
        }
        inv.exit_down = false;
    }
}

/// `GetSquareIndexAtPt`.
fn square_at(p: Vec2) -> Option<usize> {
    if !inside(p, GRID.0, GRID.1, COLUMNS as f32 * SQUARE, ROWS as f32 * SQUARE) {
        return None;
    }
    let (x, y) = (((p.x - GRID.0) / SQUARE) as usize, ((p.y - GRID.1) / SQUARE) as usize);
    Some(y * COLUMNS + x)
}

/// Lo que dibuja la cámara de los objetos: `CNewUIMyInventory::Render3D` (el equipo, con la armadura 10
/// más arriba), `CNewUIInventoryCtrl::Render3D` (la rejilla) y `CNewUIPickedItem::Render3D` (el del
/// ratón, centrado en él).
fn request_views(
    windows: Query<&Window>,
    data: Res<ItemData>,
    opts: Res<crate::options::GameOptions>,
    inv: Res<Inventory>,
    mut views: ResMut<ItemViews>,
) {
    let mut out = Vec::new();
    if inv.open {
        let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| c.floor());
        let hovered = |r: [f32; 4]| m.is_some_and(|m| inside(m, r[0], r[1], r[2], r[3]));
        let free = inv.picked.is_none();
        for (s, item) in inv.equipment.iter().enumerate() {
            let Some(item) = item else { continue };
            let (x, y, w, h) = slot_rect(s);
            let y = if s == slot::ARMOR as usize { y - 10.0 } else { y };
            let rect = [x + 1.0, y, w - 4.0, h - 4.0];
            out.push(ItemView { key: s as u32, item: *item, rect, picked: false, hovered: free && hovered(rect) });
        }
        for g in &inv.grid {
            let (w, h) = Inventory::size(&data, &g.item);
            let rect = [GRID.0 + g.x as f32 * SQUARE, GRID.1 + g.y as f32 * SQUARE, w as f32 * SQUARE, h as f32 * SQUARE];
            let key = 100 + g.y as u32 * COLUMNS as u32 + g.x as u32;
            out.push(ItemView { key, item: g.item, rect, picked: false, hovered: free && hovered(rect) });
        }
        if let (Some(p), Some(m)) = (inv.picked, m) {
            let (w, h) = Inventory::size(&data, &p.item);
            let (sw, sh) = (w as f32 * SQUARE, h as f32 * SQUARE);
            // `m_Pos = Mouse - m_Size / 2` (enteros).
            let rect = [m.x - (sw / 2.0).floor(), m.y - (sh / 2.0).floor(), sw, sh];
            out.push(ItemView { key: 999, item: p.item, rect, picked: true, hovered: hovered(rect) });
        }
    }
    if views.views != out {
        views.views = out;
    }
    if views.render_level != opts.render_level {
        views.render_level = opts.render_level;
    }
}

// --- Dibujo ------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    Back,
    Top,
    Left,
    Right,
    Bottom,
    SlotBoots,
    SlotHelm,
    SlotFairy,
    SlotWing,
    SlotWeaponR,
    SlotWeaponL,
    SlotArmor,
    SlotGloves,
    SlotPants,
    SlotRing,
    SlotNecklace,
    Money,
    Exit,
    Repair,
    Shop,
    Square,
    TableTopLeft,
    TableTopRight,
    TableBottomLeft,
    TableBottomRight,
    TableTop,
    TableBottom,
    TableLeft,
    TableRight,
}

impl Tex {
    const ALL: [Tex; 29] = [
        Tex::Back,
        Tex::Top,
        Tex::Left,
        Tex::Right,
        Tex::Bottom,
        Tex::SlotBoots,
        Tex::SlotHelm,
        Tex::SlotFairy,
        Tex::SlotWing,
        Tex::SlotWeaponR,
        Tex::SlotWeaponL,
        Tex::SlotArmor,
        Tex::SlotGloves,
        Tex::SlotPants,
        Tex::SlotRing,
        Tex::SlotNecklace,
        Tex::Money,
        Tex::Exit,
        Tex::Repair,
        Tex::Shop,
        Tex::Square,
        Tex::TableTopLeft,
        Tex::TableTopRight,
        Tex::TableBottomLeft,
        Tex::TableBottomRight,
        Tex::TableTop,
        Tex::TableBottom,
        Tex::TableLeft,
        Tex::TableRight,
    ];

    /// `CNewUIMyInventory::LoadImages` (NewUIMyInventory.cpp:1153-1177) y
    /// `CNewUIInventoryCtrl::LoadImages` (NewUIInventoryCtrl.cpp:192-209). El fondo de arriba que se
    /// dibuja es `newui_item_back04` (`IMAGE_INVENTORY_BACK_TOP2`).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_msgbox_back.jpg",
            Tex::Top => "newui_item_back04.tga",
            Tex::Left => "newui_item_back02-L.tga",
            Tex::Right => "newui_item_back02-R.tga",
            Tex::Bottom => "newui_item_back03.tga",
            Tex::SlotBoots => "newui_item_boots.tga",
            Tex::SlotHelm => "newui_item_cap.tga",
            Tex::SlotFairy => "newui_item_fairy.tga",
            Tex::SlotWing => "newui_item_wing.tga",
            // `IMAGE_INVENTORY_ITEM_RIGHT` es "weapon(L)" y `_LEFT` "weapon(R)".
            Tex::SlotWeaponR => "newui_item_weapon(L).tga",
            Tex::SlotWeaponL => "newui_item_weapon(R).tga",
            Tex::SlotArmor => "newui_item_upper.tga",
            Tex::SlotGloves => "newui_item_gloves.tga",
            Tex::SlotPants => "newui_item_lower.tga",
            Tex::SlotRing => "newui_item_ring.tga",
            Tex::SlotNecklace => "newui_item_necklace.tga",
            Tex::Money => "newui_item_money.tga",
            Tex::Exit => "newui_exit_00.tga",
            Tex::Repair => "newui_repair_00.tga",
            Tex::Shop => "newui_Bt_openshop.tga",
            Tex::Square => "newui_item_box.tga",
            Tex::TableTopLeft => "newui_item_table01(L).tga",
            Tex::TableTopRight => "newui_item_table01(R).tga",
            Tex::TableBottomLeft => "newui_item_table02(L).tga",
            Tex::TableBottomRight => "newui_item_table02(R).tga",
            Tex::TableTop => "newui_item_table03(Up).tga",
            Tex::TableBottom => "newui_item_table03(Dw).tga",
            Tex::TableLeft => "newui_item_table03(L).tga",
            Tex::TableRight => "newui_item_table03(R).tga",
        }
    }
}

#[derive(Resource)]
struct InventoryTextures(std::collections::HashMap<Tex, Handle<Image>>);

/// Una pieza en la pantalla de 640×480.
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderImage`: el trozo `src` de la textura en (x, y, ancho, alto) (estirado si no miden igual).
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, src: [f32; 4] },
    /// `RenderColor` con `glColor4f`.
    Color { x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4] },
    /// Texto con la esquina de arriba a la izquierda en (x, y); con ancho, centrado en él.
    Text { x: f32, y: f32, w: Option<f32>, text: String, color: [f32; 3], bold: bool, bg: Option<[f32; 4]> },
}

/// `RenderImage(tex, x, y, w, h, 0, sv)`: sin estirar desde (0, sv).
fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, src: [0.0, sv, w, h] }
}

fn color(x: f32, y: f32, w: f32, h: f32, rgba: [f32; 4]) -> Piece {
    Piece::Color { x, y, w, h, rgba }
}

fn text(x: f32, y: f32, w: Option<f32>, s: &str, color: [f32; 3], bold: bool) -> Piece {
    Piece::Text { x, y, w, text: s.to_string(), color, bold, bg: None }
}

/// `ConvertGold`: miles con comas.
pub fn convert_gold(zen: u32) -> String {
    let s = zen.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `getGoldColor` (ZzzInventory.cpp:1041-1057; ABGR): rojo desde 10 000 000, naranja desde 1 000 000,
/// verde desde 100 000 y amarillo claro el resto.
fn gold_color(zen: u32) -> [f32; 3] {
    let c = |r: u8, g: u8, b: u8| [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0];
    if zen >= 10_000_000 {
        c(255, 0, 0)
    } else if zen >= 1_000_000 {
        c(255, 150, 0)
    } else if zen >= 100_000 {
        c(0, 201, 24)
    } else {
        c(255, 220, 150)
    }
}

/// Alto de una línea del cuadro (`TextSize.cy` de `g_hFont` en la pantalla de 480).
const TIP_LINE: f32 = FONT;

/// `RenderTipTextList(sx, sy, ...)` (ZzzInventory.cpp:285-453) con la colocación de `RenderItemInfo`
/// (`sy += INVENTORY_SCALE` y que no pase de 420, ZzzInventory.cpp ≈5600-5620).
fn push_tooltip(out: &mut Vec<Piece>, lines: &[TipLine], sx: f32, sy: f32, metrics: &crate::chat::TextMetrics) {
    if lines.is_empty() {
        return;
    }
    let (texts, gaps) = lines.iter().fold((0, 0), |(t, g), l| if l.is_gap() { (t, g + 1) } else { (t + 1, g) });
    let height = (TIP_LINE * texts as f32 + TIP_LINE / 2.0 * gaps as f32) * 1.1;
    let mut sy = sy + 20.0;
    if sy + height > 420.0 {
        sy = 420.0 - height;
    }
    // La negrita mide algo más que la normal con la que se mide.
    let width = lines
        .iter()
        .filter(|l| !l.is_gap())
        .map(|l| metrics.width(&l.text) * if l.bold { 1.06 } else { 1.0 })
        .fold(0.0f32, f32::max)
        .ceil()
        + 4.0;
    let mut x = (sx - width / 2.0).floor().max(0.0);
    if x + width > 640.0 {
        x = 640.0 - width - 1.0;
    }
    let black = [0.0, 0.0, 0.0, 1.0];
    out.push(color(x - 1.0, sy - 1.0, width + 1.0, 1.0, black));
    out.push(color(x - 1.0, sy - 1.0, 1.0, height + 1.0, black));
    out.push(color(x + width, sy - 1.0, 1.0, height + 1.0, black));
    out.push(color(x - 1.0, sy + height, width + 2.0, 1.0, black));
    out.push(color(x, sy, width, height, [0.0, 0.0, 0.0, 0.8]));
    let mut y = sy;
    for l in lines {
        if l.is_gap() {
            y += TIP_LINE / 2.0 * 1.1;
            continue;
        }
        let bg = l.color.background().map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]);
        out.push(Piece::Text { x: x + 1.0, y, w: Some(width - 2.0), text: l.text.clone(), color: l.color.rgb(), bold: l.bold, bg });
        y += TIP_LINE * 1.1;
    }
}

/// `CNewUIButton::Render` del texto de ayuda: debajo del botón, centrado, en un fondo negro de 180.
fn push_button_tip(out: &mut Vec<Piece>, s: &str, bx: f32, by: f32, metrics: &crate::chat::TextMetrics) {
    let tw = metrics.width(s).ceil();
    let mut x = (bx + BTN_W / 2.0 - tw / 2.0).floor();
    let y = by + BTN_H + 2.0;
    if x + tw + 6.0 > 640.0 {
        x -= x + tw + 6.0 - 640.0;
    }
    out.push(Piece::Text { x, y, w: Some(tw + 6.0), text: s.to_string(), color: [1.0; 3], bold: false, bg: Some([0.0, 0.0, 0.0, 180.0 / 255.0]) });
}

impl Inventory {
    /// `CNewUIMyInventory::Render` (lo de detrás de los objetos).
    fn back_pieces(&self, data: &ItemData, hero: &HeroView, zen: u32, m: Option<Vec2>, metrics: &crate::chat::TextMetrics) -> Vec<Piece> {
        let mut out = Vec::new();
        let (x, y) = POS;
        // `RenderFrame`.
        out.push(img(Tex::Back, x, y, W, H, 0.0));
        out.push(img(Tex::Top, x, y, W, 64.0, 0.0));
        out.push(img(Tex::Left, x, y + 64.0, 21.0, 320.0, 0.0));
        out.push(img(Tex::Right, x + W - 21.0, y + 64.0, 21.0, 320.0, 0.0));
        out.push(img(Tex::Bottom, x, y + H - 45.0, W, 45.0, 0.0));
        // `RenderInventoryDetails`: el título y el zen en negrita.
        out.push(text(x, y + 12.0, Some(W), TITLE, [1.0; 3], true));
        out.push(img(Tex::Money, x + 11.0, y + 364.0, 170.0, 26.0, 0.0));
        out.push(text(x + 50.0, y + 371.0, None, &convert_gold(zen), gold_color(zen), true));
        // `RenderSetOption` y `RenderSocketOption`: en gris sin sets ni zócalos.
        let gray = [100.0 / 255.0; 3];
        out.push(text(x + W * 0.2, y + 25.0, Some(W * 0.3), SET_OPTION, gray, true));
        out.push(text(x + W * 0.5, y + 25.0, Some(W * 0.3), SOCKET_OPTION, gray, true));
        // `RenderButtons`: reparar desde el nivel 50 y la tienda desde el 6 (`OpenningProcess`).
        let over = |bx: f32, by: f32| m.is_some_and(|m| inside(m, bx, by, BTN_W, BTN_H));
        let mut tips = Vec::new();
        if hero.level >= 50 {
            out.push(img(Tex::Repair, BTN_REPAIR.0, BTN_REPAIR.1, BTN_W, BTN_H, 0.0));
            if over(BTN_REPAIR.0, BTN_REPAIR.1) {
                tips.push((TIP_REPAIR, BTN_REPAIR));
            }
        }
        if hero.level >= 6 {
            out.push(img(Tex::Shop, BTN_SHOP.0, BTN_SHOP.1, BTN_W, BTN_H, 0.0));
            if over(BTN_SHOP.0, BTN_SHOP.1) {
                tips.push((TIP_SHOP, BTN_SHOP));
            }
        }
        let exit_frame = if self.exit_down && over(BTN_EXIT.0, BTN_EXIT.1) { BTN_H } else { 0.0 };
        out.push(img(Tex::Exit, BTN_EXIT.0, BTN_EXIT.1, BTN_W, BTN_H, exit_frame));
        if over(BTN_EXIT.0, BTN_EXIT.1) {
            tips.push((TIP_EXIT, BTN_EXIT));
        }
        for (s, (bx, by)) in tips {
            push_button_tip(&mut out, s, bx, by, metrics);
        }
        self.push_grid(&mut out, data, m);
        self.push_equipment(&mut out, data, hero);
        out
    }

    /// `CNewUIInventoryCtrl::Render` (NewUIInventoryCtrl.cpp:862-1128).
    fn push_grid(&self, out: &mut Vec<Piece>, data: &ItemData, m: Option<Vec2>) {
        let (gx, gy) = GRID;
        for y in 0..ROWS {
            for x in 0..COLUMNS {
                let (px, py) = (gx + x as f32 * SQUARE, gy + y as f32 * SQUARE);
                if self.owner(data, x, y).is_some() {
                    // `ITEM_COLOR_NORMAL`.
                    out.push(color(px, py, SQUARE, SQUARE, [0.3, 0.5, 0.5, 0.6]));
                }
                out.push(img(Tex::Square, px, py, 21.0, 21.0, 0.0));
            }
        }
        let (sw, sh) = (COLUMNS as f32 * SQUARE, ROWS as f32 * SQUARE);
        out.push(img(Tex::TableTopLeft, gx - EDGE_LEFT, gy - EDGE_TOP, 14.0, 14.0, 0.0));
        out.push(img(Tex::TableTopRight, gx + sw - EDGE_RIGHT, gy - EDGE_TOP, 14.0, 14.0, 0.0));
        out.push(img(Tex::TableBottomLeft, gx - EDGE_LEFT, gy + sh - EDGE_BOTTOM, 14.0, 14.0, 0.0));
        out.push(img(Tex::TableBottomRight, gx + sw - EDGE_RIGHT, gy + sh - EDGE_BOTTOM, 14.0, 14.0, 0.0));
        // Los bordes de un píxel, uno al lado de otro: el trozo de 1 estirado.
        let (x0, x1) = (gx - EDGE_LEFT + 14.0, gx + sw - EDGE_RIGHT);
        let (y0, y1) = (gy - EDGE_TOP + 14.0, gy + sh - EDGE_BOTTOM);
        let strip = |tex, x, y, w, h, src: [f32; 4]| Piece::Image { tex, x, y, w, h, src };
        out.push(strip(Tex::TableTop, x0, gy - EDGE_TOP, x1 - x0, 14.0, [0.0, 0.0, 1.0, 14.0]));
        out.push(strip(Tex::TableBottom, x0, gy + sh - EDGE_BOTTOM, x1 - x0, 14.0, [0.0, 0.0, 1.0, 14.0]));
        out.push(strip(Tex::TableLeft, gx - EDGE_LEFT, y0, 14.0, y1 - y0, [0.0, 0.0, 14.0, 1.0]));
        out.push(strip(Tex::TableRight, gx + sw - EDGE_RIGHT, y0, 14.0, y1 - y0, [0.0, 0.0, 14.0, 1.0]));

        // Lo que va en el ratón sobre la rejilla: dónde caería, en azul, o en rojo si no cabe.
        let (Some(p), Some(m)) = (self.picked, m) else { return };
        let (w, h) = Self::size(data, &p.item);
        let (pw, ph) = (w as f32 * SQUARE, h as f32 * SQUARE);
        let (px, py) = (m.x - (pw / 2.0).floor(), m.y - (ph / 2.0).floor());
        let intersects = px < gx + sw && px + pw > gx && py < gy + sh && py + ph > gy;
        if !intersects {
            return;
        }
        let ix = m.x as i32 - (w as i32 - 1) * SQUARE as i32 / 2;
        let iy = m.y as i32 - (h as i32 - 1) * SQUARE as i32 / 2;
        // Fuera de la rejilla, la casilla (negativa o pasada) contando desde su esquina.
        let cell = |v: i32, origin: f32| {
            let d = v - origin as i32;
            if d < 0 { d / SQUARE as i32 - 1 } else { d / SQUARE as i32 }
        };
        let (mut cx, mut cy) = (cell(ix, gx), cell(iy, gy));
        let (mut nw, mut nh) = (w as i32, h as i32);
        let mut warning = false;
        let (cols, rows) = (COLUMNS as i32, ROWS as i32);
        if cx < 0 && cx >= -nw {
            nw += cx;
            cx = 0;
            warning = true;
        }
        if cx + nw > cols && cx < cols {
            nw = cols - cx;
            warning = true;
        }
        if cy < 0 && cy >= -nh {
            nh += cy;
            cy = 0;
            warning = true;
        }
        if cy + nh > rows && cy < rows {
            nh = rows - cy;
            warning = true;
        }
        if warning {
            out.push(color(gx + cx as f32 * SQUARE, gy + cy as f32 * SQUARE, nw as f32 * SQUARE, nh as f32 * SQUARE, [1.0, 0.2, 0.2, 0.4]));
        } else if (0..cols).contains(&cx) && (0..rows).contains(&cy) {
            for dy in 0..nh {
                for dx in 0..nw {
                    let (sx, sy) = (cx + dx, cy + dy);
                    let busy = self.owner(data, sx as usize, sy as usize).is_some();
                    // `m_afColorStateWarning` (1, 0.2, 0.2) / `m_afColorStateNormal` (0.1, 0.4, 0.8), a 0.4.
                    let c = if busy { [1.0, 0.2, 0.2, 0.4] } else { [0.1, 0.4, 0.8, 0.4] };
                    out.push(color(gx + sx as f32 * SQUARE, gy + sy as f32 * SQUARE, SQUARE, SQUARE, c));
                }
            }
        }
    }

    /// `RenderEquippedItem` (NewUIMyInventory.cpp:1214-1299).
    fn push_equipment(&self, out: &mut Vec<Piece>, data: &ItemData, hero: &HeroView) {
        for i in 0..slot::COUNT {
            // El Magic Gladiator no lleva casco y el Rage Fighter no lleva guantes.
            if (i == slot::HELM as usize && hero.class == 3) || (i == slot::GLOVES as usize && hero.class == 6) {
                continue;
            }
            let (x, y, w, h) = slot_rect(i);
            out.push(img(SLOTS[i].4, x, y, w, h, 0.0));
            let Some(item) = self.equipment[i] else { continue };
            let Some(attr) = data.attr(&item) else { continue };
            let max = mu_data::items::max_durability(attr, &item) as f32;
            let d = item.durability as f32;
            let c = if item.durability == 0 {
                Some([1.0, 0.0, 0.0, 0.25])
            } else if d <= max * 0.2 {
                Some([1.0, 0.15, 0.0, 0.25])
            } else if d <= max * 0.3 {
                Some([1.0, 0.5, 0.0, 0.25])
            } else if d <= max * 0.5 {
                Some([1.0, 1.0, 0.0, 0.25])
            } else if !is_equipable(self, data, hero, i, &item) {
                Some([1.0, 0.0, 0.0, 0.25])
            } else {
                None
            };
            if let Some(c) = c {
                out.push(color(x + 1.0, y, w - 4.0, h - 4.0, c));
            }
        }
        // Con un objeto en el ratón sobre un hueco ocupado o que no le va, en rojo.
        if let (Some(p), Some(s)) = (self.picked, self.pointed_slot) {
            if self.equipment[s].is_some() || !is_equipable(self, data, hero, s, &p.item) {
                let (x, y, w, h) = slot_rect(s);
                out.push(color(x + 1.0, y, w - 4.0, h - 4.0, [0.9, 0.1, 0.1, 0.4]));
            }
        }
    }

    /// Los cuadros de información (`RenderItemToolTip` de la rejilla y del equipo).
    fn front_pieces(&self, data: &ItemData, hero: &HeroView, metrics: &crate::chat::TextMetrics) -> Vec<Piece> {
        let mut out = Vec::new();
        let mana = |s: u16| data.skill_mana.as_ref().and_then(|m| m.get(s as usize)).map(|&m| m as u32);
        // `CNewUIInventoryCtrl::RenderItemToolTip`: centrado sobre el objeto, desde su fila de arriba
        // (media casilla más abajo los de una de alto).
        if let (Some(i), None) = (self.tooltip, self.picked)
            && let Some(g) = self.grid.get(i)
            && let Some(attr) = data.attr(&g.item)
        {
            let sx = GRID.0 + g.x as f32 * SQUARE + attr.width as f32 * SQUARE / 2.0;
            let mut sy = GRID.1 + g.y as f32 * SQUARE;
            if attr.height == 1 {
                sy += SQUARE / 2.0;
            }
            push_tooltip(&mut out, &item_info(attr, &g.item, hero, &mana), sx.floor(), sy, metrics);
        }
        // `CNewUIMyInventory::RenderItemToolTip`: el del hueco, desde su centro.
        if let Some(s) = self.pointed_slot
            && let Some(item) = self.equipment[s]
            && let Some(attr) = data.attr(&item)
        {
            let (x, y, w, h) = slot_rect(s);
            push_tooltip(&mut out, &item_info(attr, &item, hero, &mana), (x + w / 2.0).floor(), (y + h / 2.0).floor(), metrics);
        }
        out
    }
}

#[derive(Component)]
struct InventoryRoot;

/// Las tres capas de la ventana: lo de detrás, los objetos en 3D y los cuadros de información.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum Layer {
    Back,
    Items,
    Front,
}

fn full_screen() -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(0.0),
        top: Val::Px(0.0),
        width: Val::Px(640.0),
        height: Val::Px(480.0),
        ..default()
    }
}

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(InventoryTextures(textures));
    // La pantalla de 640×480 centrada y pegada abajo, como la barra (options.rs).
    commands
        .spawn((
            Name::new("Inventario"),
            InventoryRoot,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-320.0)),
                width: Val::Px(640.0),
                height: Val::Px(480.0),
                ..default()
            },
            Visibility::Hidden,
            GlobalZIndex(1),
        ))
        .with_children(|p| {
            p.spawn((full_screen(), Layer::Back));
            // Transparente hasta que exista la imagen de la cámara de los objetos.
            p.spawn((full_screen(), Layer::Items, ImageNode { color: Color::NONE, ..default() }));
            p.spawn((full_screen(), Layer::Front));
        });
}

fn absolute(x: f32, y: f32, w: f32, h: f32) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px(x),
        top: Val::Px(y),
        width: Val::Px(w),
        height: Val::Px(h),
        ..default()
    }
}

fn spawn_pieces(p: &mut ChildSpawnerCommands, pieces: &[Piece], textures: &InventoryTextures) {
    let rgb = |c: [f32; 3]| Color::srgb(c[0], c[1], c[2]);
    for piece in pieces {
        match piece {
            Piece::Image { tex, x, y, w, h, src } => {
                let Some(handle) = textures.0.get(tex) else { continue };
                let mut image = ImageNode::new(handle.clone());
                image.rect = Some(Rect::new(src[0], src[1], src[0] + src[2], src[1] + src[3]));
                p.spawn((absolute(*x, *y, *w, *h), image));
            }
            Piece::Color { x, y, w, h, rgba } => {
                p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]))));
            }
            Piece::Text { x, y, w, text, color, bold, bg } => {
                let mut font = TextFont { font_size: FontSize::Px(FONT), font_smoothing: FontSmoothing::AntiAliased, ..default() };
                if *bold {
                    font.weight = FontWeight::BOLD;
                }
                let label = (Text::new(text.clone()), font, LineHeight::Px(FONT), TextColor(rgb(*color)), TextLayout::no_wrap());
                let node = match w {
                    Some(w) => {
                        let mut n = absolute(*x, *y, *w, FONT);
                        n.justify_content = JustifyContent::Center;
                        n
                    }
                    None => Node { position_type: PositionType::Absolute, left: Val::Px(*x), top: Val::Px(*y), ..default() },
                };
                let mut e = p.spawn(node);
                if let Some(c) = bg {
                    e.insert(BackgroundColor(Color::srgba(c[0], c[1], c[2], c[3])));
                }
                e.with_children(|c| {
                    c.spawn(label);
                });
            }
        }
    }
}

/// Rehace las capas cuando cambia lo que se dibuja.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    windows: Query<&Window>,
    inv: Res<Inventory>,
    data: Res<ItemData>,
    views: Res<ItemViews>,
    textures: Option<Res<InventoryTextures>>,
    metrics: Res<crate::chat::TextMetrics>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut roots: Query<&mut Visibility, With<InventoryRoot>>,
    mut layers: Query<(Entity, &Layer, Option<&mut ImageNode>)>,
    mut last: Local<(Vec<Piece>, Vec<Piece>, bool)>,
) {
    let Some(textures) = textures else { return };
    let Ok(mut vis) = roots.single_mut() else { return };
    let want = if inv.open { Visibility::Inherited } else { Visibility::Hidden };
    if *vis != want {
        *vis = want;
    }
    if !inv.open {
        return;
    }
    let m = windows.single().ok().and_then(crate::mainframe::cursor_virtual).map(|c| c.floor());
    let hero = hero_view(hero.as_deref(), info.as_deref());
    let zen = info.as_ref().map_or(inv.zen, |i| i.0.money);
    let back = inv.back_pieces(&data, &hero, zen, m, &metrics);
    let front = inv.front_pieces(&data, &hero, &metrics);
    // Las capas se crean una vez (`spawn_root`); la primera vez se dibujan aunque no cambie nada.
    let fresh = !last.2;
    for (e, layer, image) in &mut layers {
        match layer {
            Layer::Back if fresh || last.0 != back => {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| spawn_pieces(p, &back, &textures));
            }
            Layer::Front if fresh || last.1 != front => {
                commands.entity(e).despawn_children();
                commands.entity(e).with_children(|p| spawn_pieces(p, &front, &textures));
            }
            Layer::Items => {
                if let (Some(mut node), Some(h)) = (image, views.image.as_ref())
                    && node.image != *h
                {
                    node.image = h.clone();
                    node.color = Color::WHITE;
                }
            }
            _ => {}
        }
    }
    *last = (back, front, true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> ItemData {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../referencias/MuMain/src/bin/Data");
        ItemData { table: mu_data::items::load(&dir).ok(), ..Default::default() }
    }

    #[test]
    fn zen_con_comas() {
        assert_eq!(convert_gold(0), "0");
        assert_eq!(convert_gold(999), "999");
        assert_eq!(convert_gold(2_500_000), "2,500,000");
        assert_eq!(convert_gold(12_345), "12,345");
    }

    #[test]
    fn casillas_y_huecos() {
        let data = data();
        if data.table.is_none() {
            return;
        }
        let mut inv = Inventory::default();
        // Una armadura de 2×3 en la primera casilla, y una espada de 1×3 ya no cabe en (1, 0).
        assert!(inv.add(&data, Item::new(group::ARMOR, 1)));
        assert_eq!(inv.grid[0].x, 0);
        assert!(!inv.fits(&data, 1, 0, 1, 3));
        assert!(inv.fits(&data, 2, 0, 1, 3));
        // No se sale por la derecha ni por abajo.
        assert!(!inv.fits(&data, 7, 0, 2, 2));
        assert!(!inv.fits(&data, 0, 6, 1, 3));
        assert_eq!(inv.owner(&data, 1, 2), Some(0));
        assert_eq!(inv.owner(&data, 2, 2), None);
        // Equipar: la espada en la mano derecha del Dark Knight, no en el casco; el escudo a la izquierda.
        let knight = HeroView { class: 1, step: 1, level: 400, stats: None };
        let sword = Item::new(group::SWORD, 1);
        assert!(is_equipable(&inv, &data, &knight, slot::WEAPON_RIGHT as usize, &sword));
        assert!(is_equipable(&inv, &data, &knight, slot::WEAPON_LEFT as usize, &sword));
        assert!(!is_equipable(&inv, &data, &knight, slot::HELM as usize, &sword));
        // Un Dark Wizard no lleva el casco de bronce; un anillo va en los dos huecos de anillo.
        let wizard = HeroView { class: 0, ..knight };
        assert!(!is_equipable(&inv, &data, &wizard, slot::HELM as usize, &Item::new(group::HELM, 0)));
        let ring = Item::new(group::HELPER, 8);
        assert!(is_equipable(&inv, &data, &wizard, slot::RING_LEFT as usize, &ring));
        assert!(is_equipable(&inv, &data, &wizard, slot::RING_RIGHT as usize, &ring));
        // Fuerza que falta (con los datos del servidor).
        let weak = HeroView { stats: Some([10, 10, 10, 10, 0]), ..knight };
        assert!(!is_equipable(&inv, &data, &weak, slot::WEAPON_RIGHT as usize, &sword));
    }

    #[test]
    fn paquete_del_inventario() {
        let mut inv = Inventory::default();
        let sword = Item { level: 7, ..Item::new(group::SWORD, 1) };
        let jewel = Item::new(group::POTION, 13);
        inv.set_items(&[(0, sword.to_bytes()), (12 + 9, jewel.to_bytes())]);
        assert_eq!(inv.equipment[0], Some(sword));
        assert_eq!(inv.grid, vec![GridItem { x: 1, y: 1, item: jewel }]);
    }

    #[test]
    fn coger_y_devolver() {
        let mut inv = Inventory { open: true, ..Default::default() };
        let it = Item::new(group::SWORD, 0);
        inv.grid.push(GridItem { x: 3, y: 2, item: it });
        let g = inv.grid.remove(0);
        inv.picked = Some(Picked { item: g.item, from: Source::Grid { x: g.x, y: g.y } });
        inv.close();
        assert!(inv.picked.is_none());
        assert_eq!(inv.grid, vec![GridItem { x: 3, y: 2, item: it }]);
        assert!(!inv.open);
    }
}
