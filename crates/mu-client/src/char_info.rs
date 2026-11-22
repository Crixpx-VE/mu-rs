//! Ventana del personaje (`C`): `SEASON3B::CNewUICharacterInfoWindow` (NewUICharacterInfoWindow.cpp
//! del Main 5.2).
//!
//! - **Abrir y cerrar**: `C` (`CNewUIHotKey::UpdateKeyEvent`, NewUIHotKey.cpp:207-212, con
//!   `SOUND_CLICK01`; con la caja del menú de sistema o el minimapa abiertos no llega), el botón del
//!   personaje de la barra (`m_BtnChaInfo`, NewUIMainFrameWindow.cpp:782-792, en mainframe.rs) y Esc
//!   (`UpdateKeyEvent`, 185-199, en la cadena de Esc de options.rs). La cierran la X de arriba
//!   (`BtnProcess`, 123-129: al apretar) y el botón de abajo a la izquierda (al soltar), sin sonido.
//!   Al cambiar de mapa se cierra (`HideAll`, WSclient.cpp:946 y 1830).
//! - **Sitio**: (640 − 190, 0) (`Create`, NewUISystem.cpp:225-226), 190 × 429. Con el inventario
//!   abierto, el inventario se corre a 640 − 190·2 (`Show`/`Hide`, NewUISystem.cpp:601-616 y
//!   965-984): eso lo hace el inventario mirando [`CharInfoWindow::is_open`].
//! - **Ratón**: dentro de la ventana un clic no hace caminar (`UpdateMouseEvent`, 106-119). Las
//!   ventanas de más arriba (menú de ventanas, opciones, minimapa, caja de mensajes: `GameUi::blocks`)
//!   se quedan antes el ratón.
//! - **Dibujo**: `RenderFrame`, `RenderSubjectTexts`, `RenderTableTexts`, `RenderAttribute` y
//!   `RenderButtons` tal cual (206-1626), con los textos de Text_eng.bmd. Las cuentas son las de
//!   `CHARACTER_MACHINE` (char_stats.rs), sin equipo ni buffs todavía.
//! - **Botones "+"**: solo con puntos por repartir (`LevelUpPoint > 0`), 4 (5 el Dark Lord). El
//!   original manda `SendRequestAddPoint(i)` (C1 F3 06, wsclientinline.h:1154-1161); aquí se manda
//!   el mensaje [`PedirSubirStat`] y no se toca nada en local: el servidor contesta con los puntos.
//!
//! **No está:** la ventana de misiones (T), la de la mascota y la del árbol de maestría (no hay;
//! sus botones se ven y no hacen nada, el de maestría bloqueado y gris como sin 3.ª clase), los
//! colores de los buffs (pociones secretas, pergaminos...) y el parpadeo del botón de la barra
//! por las misiones de época.

use bevy::prelude::*;
use bevy::text::{FontSize, FontSmoothing, FontWeight, LineHeight};
use mu_protocol::game::ServerMessage;

use crate::Args;
use crate::account::Class;
use crate::char_stats::{self, EquipBonus, Points};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct CharInfoPlugin;

impl Plugin for CharInfoPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CharInfoWindow>()
            .init_resource::<CharEquipment>()
            .add_message::<PedirSubirStat>()
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(
                OnExit(GameScene::Game),
                (crate::scene::despawn::<CharInfoRoot>, |mut w: ResMut<CharInfoWindow>| w.close()),
            )
            .add_systems(PreUpdate, close_on_map_change.after(crate::net::NetSystems))
            .add_systems(
                Update,
                (update, draw, blink, place_tooltips, subir_stat_pendiente).chain().run_if(in_state(GameScene::Game)),
            );
    }
}

// --- Estado ------------------------------------------------------------------------------------

/// `CHAINFO_WINDOW_WIDTH/HEIGHT` y `m_Pos` (640 − 190, 0).
const W: f32 = 190.0;
const H: f32 = 429.0;
const POS: Vec2 = Vec2::new(640.0 - W, 0.0);
/// `HEIGHT_STRENGTH`, `HEIGHT_DEXTERITY`, `HEIGHT_VITALITY`, `HEIGHT_ENERGY` y `HEIGHT_CHARISMA`
/// (NewUICharacterInfoWindow.h:47-51).
const ROWS: [f32; 5] = [120.0, 175.0, 240.0, 295.0, 350.0];

/// Una estadística que se puede subir (`STAT_STRENGTH`...: el número que va en el paquete).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stat {
    Strength = 0,
    Agility = 1,
    Vitality = 2,
    Energy = 3,
    Command = 4,
}

impl Stat {
    const ALL: [Stat; 5] = [Stat::Strength, Stat::Agility, Stat::Vitality, Stat::Energy, Stat::Command];
}

/// Se ha pulsado un "+": el original manda `SendRequestAddPoint(tipo)` (C1 F3 06 tipo).
#[derive(Message, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PedirSubirStat(pub Stat);

/// Lo que el equipo suma a las cuentas de la ventana (sin objetos todavía: nada). Lo rellenará el
/// inventario cuando haya equipo (`CharacterMachine->Equipment`).
#[derive(Resource, Clone, Default, Debug)]
pub struct CharEquipment(pub EquipBonus);

/// El servidor elegido en el login (`g_ServerListManager->GetSelectServerName/Index` y
/// `GetNonPVPInfo`), que la ventana enseña bajo el nombre. Lo deja login_ui.rs al salir.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SelectedServer {
    pub name: String,
    pub index: i32,
    pub non_pvp: u8,
}

/// Botones de la ventana.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    /// Un "+" (`m_BtnStat[i]`).
    Stat(usize),
    /// `m_BtnExit`, `m_BtnQuest`, `m_BtnPet` y `m_BtnMasterLevel`.
    Exit,
    Quest,
    Pet,
    Master,
}

impl Btn {
    /// (x, y, ancho, alto) dentro de la ventana (`SetButtonInfo`, 53-87).
    fn rect(self) -> (f32, f32, f32, f32) {
        match self {
            Btn::Stat(i) => (160.0, ROWS[i] + 2.0, 16.0, 15.0),
            Btn::Exit => (13.0, 392.0, 36.0, 29.0),
            Btn::Quest => (50.0, 392.0, 36.0, 29.0),
            Btn::Pet => (87.0, 392.0, 36.0, 29.0),
            Btn::Master => (124.0, 392.0, 36.0, 29.0),
        }
    }
}

/// `INTERFACE_CHARACTER`.
#[derive(Resource, Default)]
pub struct CharInfoWindow {
    open: bool,
    /// El ratón dentro de la ventana (coordenadas de la ventana) si la ventana lo recibe, y el botón
    /// izquierdo apretado: para el fotograma de los botones y las ayudas.
    cursor: Option<Vec2>,
    held: bool,
}

impl CharInfoWindow {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// `g_pNewUISystem->Toggle(INTERFACE_CHARACTER)`.
    pub fn toggle(&mut self) {
        if self.open { self.close() } else { self.open = true }
    }

    /// `Hide(INTERFACE_CHARACTER)`.
    pub fn close(&mut self) {
        self.open = false;
        self.cursor = None;
    }

    /// El ratón (pantalla de 640×480) cae en la ventana: un clic no hace caminar.
    pub fn blocks(&self, c: Vec2) -> bool {
        self.open && inside(c - POS, 0.0, 0.0, W, H)
    }
}

/// `CheckMouseIn` (ratón en enteros).
fn inside(c: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    let (mx, my) = (c.x.floor(), c.y.floor());
    mx >= x && mx < x + w && my >= y && my < y + h
}

// --- Lo que se enseña --------------------------------------------------------------------------

/// Lo que la ventana lee del personaje (`CharacterAttribute`, `Hero` y el equipo).
#[derive(Clone, PartialEq, Debug)]
struct View {
    name: String,
    class: Class,
    /// `Hero->PK` (`CharacterHeroState`).
    pk: u8,
    points: Points,
    level_up_points: u16,
    experience: u64,
    next_experience: u64,
    life: u16,
    life_max: u16,
    mana: u16,
    mana_max: u16,
    /// `AddPoint`, `MaxAddPoint`, `wMinusPoint` y `wMaxMinusPoint` (los frutos).
    add_point: i16,
    max_add_point: i16,
    minus_point: u16,
    max_minus_point: u16,
    server: Option<SelectedServer>,
    eq: EquipBonus,
    /// `SkillAttribute[Skill[Hero->CurrentSkill]].Damage` y si es una invocación de la Summoner
    /// (`AT_SKILL_SUMMON_EXPLOSION`..`REQUIEM`). Sin la lista de habilidades del servidor todavía
    /// no hay ninguna: la 0, sin daño.
    skill_damage: u16,
    skill_is_summon: bool,
}

/// `GlobalText` de Text_eng.bmd.
mod txt {
    pub const STR: &str = "STR"; // 166
    pub const AGI: &str = "Agility"; // 1702
    pub const VIT: &str = "STA"; // 169
    pub const ENE: &str = "ENG"; // 168
    pub const CMD: &str = "Command"; // 1900
    pub const CREATE: &str = "Create"; // 1412
    pub const DECREASE: &str = "Decrease"; // 1903
    pub const CLOSE_TIP: &str = "Close (C)"; // 927 con "C"
    pub const QUEST_TIP: &str = "Quest(T)"; // "%s(%s)" con 1140 y "T"
    pub const PET_TIP: &str = "Pet"; // 1217
    pub const MASTER_TIP: &str = "Master skill tree (A)"; // 1749
}

/// Colores de `SetTextColor`.
const WHITE: [u8; 3] = [255, 255, 255];
const YELLOW: [u8; 3] = [230, 230, 0];
const ORANGE: [u8; 3] = [255, 138, 0];
const BLUE: [u8; 3] = [100, 150, 255];
const SKY: [u8; 3] = [76, 197, 254];

/// Una pieza de la ventana (coordenadas dentro de ella).
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderImage(tex, x, y, ancho, alto, 0, sv)`: el trozo de ancho × alto de la textura desde
    /// (0, sv); con `stretch`, la textura entera estirada (las líneas de 1 píxel). `gray`: el color
    /// (100, 100, 100) del botón bloqueado.
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32, stretch: bool, gray: bool },
    /// `RenderColor` (y el fondo de `SetBgColor`).
    Fill { x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4] },
    /// `RenderText(x, y, texto, ancho)`: con `center`, centrado en ese ancho (`RT3_SORT_CENTER`).
    /// `blink`: 0 la clase y 1 el servidor, que se turnan (`RenderSubjectTexts`, 286-290).
    Text { x: f32, y: f32, center: Option<f32>, text: String, color: [u8; 3], bold: bool, blink: Option<u8> },
    /// La ayuda de un botón (`CNewUIButton::Render`, NewUIButton.cpp:439-460).
    Tooltip { cx: f32, y: f32, text: String },
}

fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, sv: 0.0, stretch: false, gray: false }
}

fn line(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, sv: 0.0, stretch: true, gray: false }
}

fn text(x: f32, y: f32, s: String, color: [u8; 3], bold: bool) -> Piece {
    Piece::Text { x, y, center: None, text: s, color, bold, blink: None }
}

fn centered(x: f32, y: f32, w: f32, s: String, color: [u8; 3], bold: bool) -> Piece {
    Piece::Text { x, y, center: Some(w), text: s, color, bold, blink: None }
}

/// `GlobalText[461, 460, 3130, 3131]` según `GetNonPVPInfo`.
fn server_text(s: &SelectedServer) -> String {
    let (name, index) = (&s.name, s.index);
    match s.non_pvp {
        1 => format!("[{name}-{index}(Non-PvP) Server]"),
        2 => format!("[{name}-{index}(Gold PvP) Server]"),
        3 => format!("[{name}-{index}(Gold) Server]"),
        _ => format!("[{name}-{index} Server]"),
    }
}

/// El tanto por ciento de los frutos (`iAddPoint`/`iMinusPoint`, 358-417).
fn fruit_rate(used: i32, max: i32) -> i32 {
    if used <= 10 {
        return 100;
    }
    let t = if max != 0 { used * 100 / max } else { 0 };
    match t {
        ..=10 => 70,
        11..=30 => 60,
        31..=50 => 50,
        _ => 40,
    }
}

/// `GlobalText[203]`/`[204]`: daño (con el mínimo nunca por encima del máximo) y ataque.
fn damage_text(min: i32, max: i32, rating: i32) -> String {
    let min = if min >= max { max } else { min };
    if rating > 0 { format!("Dmg(rate): {min}~{max} ({rating})") } else { format!("Dmg: {min}~{max}") }
}

impl View {
    fn base_class(&self) -> Class {
        self.class
    }

    /// Fuerza... de una fila (`Strength + AddStrength`) y si lleva algo del equipo (azul).
    fn stat(&self, d: &char_stats::Derived, s: Stat) -> (u16, bool) {
        let eq = &self.eq;
        match s {
            Stat::Strength => (d.strength, eq.add_strength != 0),
            Stat::Agility => (d.agility, eq.add_agility != 0),
            Stat::Vitality => (d.vitality, eq.add_vitality != 0),
            Stat::Energy => (d.energy, eq.add_energy != 0),
            Stat::Command => (d.command, eq.add_command != 0),
        }
    }

    /// Los textos de `RenderSubjectTexts`, `RenderTableTexts` y `RenderAttribute` (267-1599).
    fn texts(&self) -> Vec<Piece> {
        let mut out = Vec::new();
        let class = self.base_class();
        let eq = &self.eq;
        let d = char_stats::calculate(class, &self.points, eq);

        // `RenderSubjectTexts`: el nombre en negrita sobre (20, 20, 20, 20) y, debajo, la clase y
        // el servidor turnándose (todo en negrita: no se cambia de letra).
        out.push(Piece::Fill { x: 0.0, y: 12.0, w: W, h: 0.0, rgba: [20, 20, 20, 20] });
        let [r, g, b, _] = crate::names::player_color(self.pk);
        out.push(centered(0.0, 12.0, W, self.name.clone(), [r, g, b], true));
        out.push(Piece::Text { x: 0.0, y: 27.0, center: Some(W), text: format!("({})", class.name()), color: WHITE, bold: true, blink: Some(0) });
        if let Some(s) = &self.server {
            out.push(Piece::Text { x: 0.0, y: 27.0, center: Some(W), text: server_text(s), color: WHITE, bold: true, blink: Some(1) });
        }

        // `RenderTableTexts` (sin maestría: `IsMasterLevel` necesita la 3.ª clase).
        out.push(text(18.0, 58.0, format!("Level: {}", self.points.level), YELLOW, true)); // 200
        if self.level_up_points > 0 {
            out.push(text(110.0, 58.0, format!("Point: {}", self.level_up_points), ORANGE, true)); // 217
        }
        out.push(text(18.0, 75.0, format!("Exp : {}/{}", self.experience, self.next_experience), WHITE, false)); // 201
        let (add, max_add) = (self.add_point as i32, self.max_add_point as i32);
        let rates = (fruit_rate(add, max_add), fruit_rate(self.minus_point as i32, self.max_minus_point as i32));
        out.push(text(18.0, 88.0, format!("[+]:{}%|[-]:{}%", rates.0, rates.1), SKY, false)); // 1907
        let fruits = if self.points.level > 9 {
            let minus = -(self.minus_point as i32);
            let max_minus = -(self.max_minus_point as i32);
            format!("{} {add}/{max_add} | {} {minus}/{max_minus}", txt::CREATE, txt::DECREASE)
        } else {
            format!("{} 0/0 | {} 0/0", txt::CREATE, txt::DECREASE)
        };
        out.push(text(18.0, 101.0, fruits, SKY, false));

        // `RenderAttribute`: cada fila con su nombre y su valor en negrita, del mismo color.
        let rows = if class == Class::Lord { 5 } else { 4 };
        let labels = [txt::STR, txt::AGI, txt::VIT, txt::ENE, txt::CMD];
        for (i, s) in Stat::ALL.into_iter().enumerate().take(rows) {
            let (value, from_items) = self.stat(&d, s);
            let color = if from_items { BLUE } else { YELLOW };
            out.push(centered(12.0, ROWS[i] + 6.0, 74.0, labels[i].to_string(), color, true));
            out.push(centered(86.0, ROWS[i] + 6.0, 86.0, value.to_string(), color, true));
        }

        // Fuerza: daño y ataque (461-836). Sin habilidades que los suban (`Add_Rat`...: 0).
        let (mut dmin, mut dmax, alpha) = char_stats::attack_damage(class, &d, eq.hand);
        dmin += eq.window_damage.0;
        dmax += eq.window_damage.1;
        let rating = d.attack_rating as i32;
        let rating_pk = d.attack_rating_pk as i32;
        let mut y = ROWS[0] + 25.0;
        out.push(text(20.0, y, damage_text(dmin, dmax, rating), if alpha { BLUE } else { WHITE }, false));
        if rating_pk > 0 {
            let s = if eq.pvp_attack_bonus != 0 {
                format!("Attack rate: {rating_pk} (+{})", eq.pvp_attack_bonus) // 2109
            } else {
                format!("Attack rate: {rating_pk}") // 2044
            };
            y += 13.0;
            out.push(text(20.0, y, s, WHITE, false));
        }

        // Agilidad: defensa, velocidad y defensa contra jugadores (942-1139).
        // `t_adjdef + maxdefense + iChangeRingAddDefense`.
        let def = d.defense as i32 + eq.harmony_defense + eq.ring_defense;
        let blocking = d.blocking as i32;
        let s = match (eq.full_set, blocking > 0) {
            // 206: "Defense (rate):%d (%d +%d)"; el +10 % del conjunto.
            (true, true) => format!("Defense (rate):{def} ({blocking} +{})", blocking / 10),
            // 207: `(t_adjdef + iChangeRingAddDefense) / 10`, sin la armonía.
            (true, false) => format!("Defense: {def} (+{})", (d.defense as i32 + eq.ring_defense) / 10),
            (false, true) => format!("Defense (rate):{def} ({blocking})"), // 208
            (false, false) => format!("Defense: {def}"),                    // 209
        };
        let mut y = ROWS[1] + 24.0;
        out.push(text(20.0, y, s, WHITE, false));
        // `MagicSpeed` el Dark Wizard y la Summoner, `AttackSpeed` el resto.
        let speed = if matches!(class, Class::Wizard | Class::Summoner) { d.magic_speed } else { d.attack_speed };
        y += 13.0;
        out.push(text(20.0, y, format!("Attack speed: {speed}"), WHITE, false)); // 64
        let s = if eq.pvp_defense_bonus != 0 {
            format!("Defense rate: {} (+{})", d.blocking_pk, eq.pvp_defense_bonus) // 2110
        } else {
            format!("Defense rate: {}", d.blocking_pk) // 2045
        };
        y += 13.0;
        out.push(text(20.0, y, s, WHITE, false));

        // Vitalidad: vida (y el daño cuerpo a cuerpo del Rage Fighter), 1184-1233.
        let mut y = ROWS[2] + 24.0;
        out.push(text(20.0, y, format!("HP: {} / {}", self.life, self.life_max), WHITE, false)); // 211
        if class == Class::Fighter {
            y += 13.0;
            out.push(text(20.0, y, format!("Melee Damage: {}%", 50 + d.vitality / 10), WHITE, false)); // 3155
        }

        // Energía: maná, daño mágico, maldición y daño de las habilidades (1260-1568).
        let mut y = ROWS[3] + 24.0;
        out.push(text(18.0, y, format!("Mana: {} / {}", self.mana, self.mana_max), WHITE, false)); // 213
        if matches!(class, Class::Wizard | Class::Gladiator | Class::Summoner) {
            // `GetMagicSkillDamage` (CharacterManager.cpp:355-386).
            let (smin, smax) = if self.skill_is_summon {
                (d.magic_min as i32, d.magic_max as i32)
            } else {
                let k = self.skill_damage as i32;
                (d.magic_min as i32 + k, d.magic_max as i32 + k + k / 2)
            };
            let mg = eq.harmony_magic;
            let (min, max) = (smin + eq.window_magic.0 + mg, smax + eq.window_magic.1 + mg);
            let s = match eq.staff_magic {
                Some(p) => format!("Wizardry Dmg: {min}~{max} (+{})", ((smax + mg) as f32 * p) as i32), // 215
                None => format!("Wizardry Dmg: {min}~{max}"),                                            // 216
            };
            y += 13.0;
            out.push(text(18.0, y, s, WHITE, false));
        }
        if class == Class::Summoner {
            // `GetCurseSkillDamage` (CharacterManager.cpp:388-404).
            let (mut min, mut max) = (d.curse_min as i32, d.curse_max as i32);
            if self.skill_is_summon {
                let k = self.skill_damage as i32;
                (min, max) = (min + k, max + k + k / 2);
            }
            (min, max) = (min + eq.window_curse.0, max + eq.window_curse.1);
            let s = match eq.book_curse {
                Some(p) => format!(" Curse Spell: {min} ~ {max}(+{})", (max as f32 * p) as i32), // 1693
                None => format!(" Curse Spell: {min} ~ {max}"),                                  // 1694
            };
            y += 13.0;
            out.push(text(18.0, y, s, WHITE, false));
        }
        y += 13.0;
        let e = d.energy as i32;
        match class {
            Class::Knight => out.push(text(20.0, y, format!("Skill Damage: {}%", 200 + e / 10), WHITE, false)), // 582
            Class::Gladiator => out.push(text(20.0, y, "Skill Damage: 200%".into(), WHITE, false)),
            Class::Lord => out.push(text(20.0, y, format!("Skill Damage: {}%", 200 + e / 20), WHITE, false)),
            Class::Fighter => {
                out.push(text(20.0, y, format!("Divine Damage (Roar, Slasher): {}%", 50 + e / 10), WHITE, false)); // 3156
                y += 13.0;
                let a = d.agility as i32;
                out.push(text(20.0, y, format!("AOE Damage (Dark Side): {}%", 100 + (a / 8 + e / 10)), WHITE, false)); // 3157
            }
            _ => {}
        }
        out
    }
}

/// Lo que se dibuja (`Render`: `RenderFrame`, `RenderTexts` y `RenderButtons`).
fn pieces(v: &View, cursor: Option<Vec2>, held: bool) -> Vec<Piece> {
    let mut out = Vec::new();
    let lord = v.base_class() == Class::Lord;

    // `RenderFrame` (206-247).
    out.push(img(Tex::Back, 0.0, 0.0, W, H));
    out.push(img(Tex::Top, 0.0, 0.0, W, 64.0));
    out.push(img(Tex::Left, 0.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Right, W - 21.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Bottom, 0.0, H - 45.0, W, 45.0));
    out.push(Piece::Fill { x: 12.0, y: 48.0, w: 160.0, h: 66.0, rgba: [0, 0, 0, 77] });
    out.push(img(Tex::TableTopLeft, 12.0, 48.0, 14.0, 14.0));
    out.push(img(Tex::TableTopRight, 12.0 + 165.0 - 14.0, 48.0, 14.0, 14.0));
    out.push(img(Tex::TableBottomLeft, 12.0, 119.0 - 14.0, 14.0, 14.0));
    out.push(img(Tex::TableBottomRight, 12.0 + 165.0 - 14.0, 119.0 - 14.0, 14.0, 14.0));
    // Las tiras de 1 píxel, de una vez.
    out.push(line(Tex::TableTopPixel, 26.0, 48.0, 165.0 - 28.0, 14.0));
    out.push(line(Tex::TableBottomPixel, 26.0, 119.0 - 14.0, 165.0 - 28.0, 14.0));
    out.push(line(Tex::TableBottomPixel, 14.0, 48.0 + 12.0, 12.0 + 165.0 - 4.0 - 14.0, 14.0));
    out.push(line(Tex::TableLeftPixel, 12.0, 62.0, 14.0, 119.0 - 14.0 - 62.0));
    out.push(line(Tex::TableRightPixel, 12.0 + 165.0 - 14.0, 62.0, 14.0, 119.0 - 14.0 - 62.0));
    for &y in &ROWS[..if lord { 5 } else { 4 }] {
        out.push(img(Tex::TextBox, 11.0, y, 170.0, 21.0));
    }

    out.extend(v.texts());

    // `RenderButtons` (1601-1626): `ChangeButtonImgState(true, img, false)`: fotograma 0 y, apretado
    // encima, el 1 (sin fotograma de ratón encima).
    let over = |b: Btn| {
        let (x, y, w, h) = b.rect();
        cursor.is_some_and(|c| inside(c, x, y, w, h))
    };
    let frame = |b: Btn| if held && over(b) { 1.0 } else { 0.0 };
    if v.level_up_points > 0 {
        for i in 0..if lord { 5 } else { 4 } {
            let b = Btn::Stat(i);
            let (x, y, w, h) = b.rect();
            out.push(Piece::Image { tex: Tex::BtnStat, x, y, w, h, sv: frame(b) * h, stretch: false, gray: false });
        }
    }
    let bottom = [
        (Btn::Exit, Tex::BtnExit, txt::CLOSE_TIP),
        (Btn::Quest, Tex::BtnQuest, txt::QUEST_TIP),
        (Btn::Pet, Tex::BtnPet, txt::PET_TIP),
        (Btn::Master, Tex::BtnMaster, txt::MASTER_TIP),
    ];
    for (b, tex, _) in bottom {
        let (x, y, w, h) = b.rect();
        // El de maestría, bloqueado (sin 3.ª clase): no cambia de fotograma y va gris
        // (`OpenningProcess`, 1689-1704).
        let locked = b == Btn::Master;
        let sv = if locked { 0.0 } else { frame(b) * h };
        out.push(Piece::Image { tex, x, y, w, h, sv, stretch: false, gray: locked });
    }
    for (b, _, tip) in bottom {
        if over(b) {
            let (x, y, w, h) = b.rect();
            out.push(Piece::Tooltip { cx: x + w / 2.0, y: y + h + 2.0, text: tip.to_string() });
        }
    }
    out
}

// --- Lógica ------------------------------------------------------------------------------------

/// Lo que hay del personaje: `HeroInfo` del servidor o, sin él (`--world`), los puntos de la clase
/// al crearla.
fn view(
    hero: Option<&crate::account::Hero>,
    info: Option<&crate::account::HeroInfo>,
    stats: &crate::mainframe::CharacterStats,
    server: Option<&SelectedServer>,
    eq: &CharEquipment,
) -> View {
    let class = hero.map_or(Class::Knight, |h| h.class);
    let level = hero.map_or(1, |h| h.level.min(u16::MAX as u32) as u16);
    let mut v = View {
        name: hero.map_or_else(String::new, |h| h.name.clone()),
        class,
        pk: 3,
        points: Points { level, ..Default::default() },
        level_up_points: 0,
        experience: 0,
        next_experience: 0,
        life: stats.hp as u16,
        life_max: stats.hp_max as u16,
        mana: stats.mp as u16,
        mana_max: stats.mp_max as u16,
        add_point: 0,
        max_add_point: 0,
        minus_point: 0,
        max_minus_point: 0,
        server: server.cloned(),
        eq: eq.0.clone(),
        skill_damage: 0,
        skill_is_summon: false,
    };
    match info {
        Some(crate::account::HeroInfo(i)) => {
            v.pk = i.hero_state;
            v.points = Points {
                level,
                strength: i.strength,
                agility: i.agility,
                vitality: i.vitality,
                energy: i.energy,
                command: i.leadership,
            };
            v.level_up_points = i.level_up_points;
            (v.experience, v.next_experience) = (i.experience, i.next_experience);
            (v.life, v.life_max, v.mana, v.mana_max) = (i.health, i.max_health, i.mana, i.max_mana);
            // `short AddPoint, MaxAddPoint` (_struct.h:380-383).
            (v.add_point, v.max_add_point) = (i.used_fruit_points as i16, i.max_fruit_points as i16);
            (v.minus_point, v.max_minus_point) = (i.used_negative_fruit_points, i.max_negative_fruit_points);
        }
        None => {
            let (base, command) = class.base_stats();
            let n = |s: &str| s.parse().unwrap_or(0);
            v.points.strength = n(base[0]);
            v.points.agility = n(base[1]);
            v.points.vitality = n(base[2]);
            v.points.energy = n(base[3]);
            v.points.command = command.map_or(0, n);
        }
    }
    v
}

#[allow(clippy::too_many_arguments)]
fn update(
    mut w: ResMut<CharInfoWindow>,
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    game_ui: Res<crate::options::GameUi>,
    hero: Option<Res<crate::account::Hero>>,
    info: Option<Res<crate::account::HeroInfo>>,
    mut sound: ResMut<SoundQueue>,
    mut ask: MessageWriter<PedirSubirStat>,
) {
    // `C` (con la caja del menú de sistema o el minimapa abiertos las teclas rápidas no llegan,
    // NewUIHotKey.cpp:134-137).
    if keys.just_pressed(KeyCode::KeyC) && !game_ui.modal() && !game_ui.minimap {
        w.toggle();
        sound.play("SOUND_CLICK01");
    }
    if !w.open {
        return;
    }
    // Las ventanas de capa más alta se quedan antes el ratón.
    let cursor = windows
        .single()
        .ok()
        .and_then(crate::mainframe::cursor_virtual)
        .filter(|&c| !game_ui.blocks(c))
        .map(|c| c - POS)
        .filter(|&c| inside(c, 0.0, 0.0, W, H));
    w.cursor = cursor;
    w.held = mouse.pressed(MouseButton::Left);
    let Some(c) = cursor else { return };
    let at = |b: Btn| {
        let (x, y, bw, bh) = b.rect();
        inside(c, x, y, bw, bh)
    };
    let release = mouse.just_released(MouseButton::Left);

    // `BtnProcess` (121-183), por orden. La X de arriba cierra al apretar.
    if mouse.just_pressed(MouseButton::Left) && inside(c, 169.0, 7.0, 13.0, 12.0) {
        w.close();
        return;
    }
    let points = info.as_ref().map_or(0, |i| i.0.level_up_points);
    if points > 0 && release {
        let lord = hero.as_ref().is_some_and(|h| h.class == Class::Lord);
        let count = if lord { 5 } else { 4 };
        if let Some(i) = (0..count).find(|&i| at(Btn::Stat(i))) {
            // `SendRequestAddPoint(i)`, que suena `SOUND_CLICK01`.
            ask.write(PedirSubirStat(Stat::ALL[i]));
            sound.play("SOUND_CLICK01");
            return;
        }
    }
    if release && at(Btn::Exit) {
        w.close();
    } else if release && at(Btn::Quest) {
        // `Toggle(INTERFACE_MYQUEST)`.
        info!("ventana del personaje: no hay ventana de misiones todavía");
    } else if release && at(Btn::Pet) {
        // `Toggle(INTERFACE_PET)`.
        info!("ventana del personaje: no hay ventana de la mascota todavía");
    }
    // `m_BtnMasterLevel`: bloqueado sin 3.ª clase (`UpdateMouseEvent` de un botón bloqueado no hace nada).
}

/// `PedirSubirStat` todavía no llega al servidor.
fn subir_stat_pendiente(mut asks: MessageReader<PedirSubirStat>) {
    for PedirSubirStat(stat) in asks.read() {
        // TODO(servidor): mandar `IncreaseCharacterStatPoint` (C1 F3 06 <tipo>, OpenMU
        // `IncreaseCharacterStatPoint`) y aplicar la respuesta F3 06 (`ReceiveAddPoint`) en
        // `HeroInfo`. No se cambia nada aquí: lo decide el servidor.
        info!("ventana del personaje: pedir subir {stat:?} (tipo {}): sin paquete todavía", *stat as u8);
    }
}

/// `HideAll` al cambiar de mapa (`ReceiveTeleport`, WSclient.cpp:1830).
fn close_on_map_change(mut from: MessageReader<crate::net::FromServer>, mut w: ResMut<CharInfoWindow>) {
    for m in from.read() {
        if let ServerMessage::MapChanged { is_map_change: true, .. } = m.0 {
            w.close();
        }
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
    TableTopLeft,
    TableTopRight,
    TableBottomLeft,
    TableBottomRight,
    TableTopPixel,
    TableBottomPixel,
    TableLeftPixel,
    TableRightPixel,
    BtnExit,
    TextBox,
    BtnStat,
    BtnQuest,
    BtnPet,
    BtnMaster,
}

impl Tex {
    const ALL: [Tex; 19] = [
        Tex::Back,
        Tex::Top,
        Tex::Left,
        Tex::Right,
        Tex::Bottom,
        Tex::TableTopLeft,
        Tex::TableTopRight,
        Tex::TableBottomLeft,
        Tex::TableBottomRight,
        Tex::TableTopPixel,
        Tex::TableBottomPixel,
        Tex::TableLeftPixel,
        Tex::TableRightPixel,
        Tex::BtnExit,
        Tex::TextBox,
        Tex::BtnStat,
        Tex::BtnQuest,
        Tex::BtnPet,
        Tex::BtnMaster,
    ];

    /// `LoadImages` (1633-1657).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_msgbox_back.jpg",
            Tex::Top => "newui_item_back04.tga",
            Tex::Left => "newui_item_back02-L.tga",
            Tex::Right => "newui_item_back02-R.tga",
            Tex::Bottom => "newui_item_back03.tga",
            Tex::TableTopLeft => "newui_item_table01(L).tga",
            Tex::TableTopRight => "newui_item_table01(R).tga",
            Tex::TableBottomLeft => "newui_item_table02(L).tga",
            Tex::TableBottomRight => "newui_item_table02(R).tga",
            Tex::TableTopPixel => "newui_item_table03(Up).tga",
            Tex::TableBottomPixel => "newui_item_table03(Dw).tga",
            Tex::TableLeftPixel => "newui_item_table03(L).tga",
            Tex::TableRightPixel => "newui_item_table03(R).tga",
            Tex::BtnExit => "newui_exit_00.tga",
            Tex::TextBox => "newui_cha_textbox02.tga",
            Tex::BtnStat => "newui_chainfo_btn_level.tga",
            Tex::BtnQuest => "newui_chainfo_btn_quest.tga",
            Tex::BtnPet => "newui_chainfo_btn_pet.tga",
            Tex::BtnMaster => "newui_chainfo_btn_master.tga",
        }
    }
}

/// Las texturas y su tamaño.
#[derive(Resource)]
struct CharInfoTextures(std::collections::HashMap<Tex, (Handle<Image>, Vec2)>);

#[derive(Component)]
struct CharInfoRoot;

/// El texto de la clase (0) o del servidor (1), que se turnan.
#[derive(Component)]
struct Blink(u8);

/// Una ayuda: su centro (la caja se coloca cuando se sabe lo que mide).
#[derive(Component)]
struct TooltipBox {
    cx: f32,
}

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|h| (t, h)))
        .collect();
    commands.insert_resource(CharInfoTextures(textures));
    // La pantalla de 640×480 centrada y pegada abajo, como la barra; la ventana en (450, 0).
    commands
        .spawn((
            Name::new("Ventana del personaje"),
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-320.0)),
                width: Val::Px(640.0),
                height: Val::Px(480.0),
                ..default()
            },
            // Por debajo del menú de ventanas y las opciones (capa 5.1 contra 10.4 y 10.5).
            GlobalZIndex(0),
        ))
        .with_children(|p| {
            p.spawn((CharInfoRoot, absolute(POS.x, POS.y, W, H), Visibility::Hidden));
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

/// `g_hFont`/`g_hFontBold` (`CreateFont(FontHeight - 1)`, Winmain.cpp:1456-1472) en unidades de la
/// pantalla de 640, con la resolución más cercana de las del `switch (WindowWidth)`.
fn font_size(window_width: u32) -> f32 {
    let ww = [800, 1024, 1280].into_iter().min_by_key(|&c| (c - window_width as i32).abs()).unwrap_or(1024);
    let font_height = match ww {
        800 => 13,
        1024 => 14,
        _ => 15,
    };
    (font_height - 1) as f32 * 640.0 / ww as f32
}

fn label(t: &str, color: [u8; 3], bold: bool, font: f32) -> impl Bundle {
    let mut text_font = TextFont { font_size: FontSize::Px(font), font_smoothing: FontSmoothing::AntiAliased, ..default() };
    if bold {
        text_font.weight = FontWeight::BOLD;
    }
    (
        Text::new(t.to_string()),
        text_font,
        LineHeight::Px(font),
        TextColor(Color::srgb_u8(color[0], color[1], color[2])),
        TextLayout::no_wrap(),
    )
}

/// Rehace la ventana cuando cambia algo de lo que se dibuja.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    w: Res<CharInfoWindow>,
    (hero, info, stats, server, eq): (
        Option<Res<crate::account::Hero>>,
        Option<Res<crate::account::HeroInfo>>,
        Res<crate::mainframe::CharacterStats>,
        Option<Res<SelectedServer>>,
        Res<CharEquipment>,
    ),
    windows: Query<&Window>,
    textures: Option<Res<CharInfoTextures>>,
    mut root: Query<(Entity, &mut Visibility), With<CharInfoRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>, f32)>,
) {
    let (Ok((root, mut visibility)), Some(textures)) = (root.single_mut(), textures) else { return };
    let shown = if w.open { Visibility::Inherited } else { Visibility::Hidden };
    if *visibility != shown {
        *visibility = shown;
    }
    if !w.open {
        return;
    }
    let v = view(hero.as_deref(), info.as_deref(), &stats, server.as_deref(), &eq);
    let pieces = pieces(&v, w.cursor, w.held);
    let font = font_size(windows.single().map_or(1024, |w| w.physical_width()));
    if last.0 == Some(root) && last.1 == pieces && last.2 == font {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Image { tex, x, y, w, h, sv, stretch, gray } => {
                    let Some((handle, size)) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(if *stretch { Rect::new(0.0, 0.0, size.x, size.y) } else { Rect::new(0.0, *sv, *w, sv + h) });
                    if *gray {
                        image.color = Color::srgb_u8(100, 100, 100);
                    }
                    p.spawn((absolute(*x, *y, *w, *h), image));
                }
                Piece::Fill { x, y, w, h, rgba: [r, g, b, a] } => {
                    // Alto 0: el de una línea de texto.
                    let h = if *h > 0.0 { *h } else { font };
                    p.spawn((absolute(*x, *y, *w, h), BackgroundColor(Color::srgba_u8(*r, *g, *b, *a))));
                }
                Piece::Text { x, y, center, text, color, bold, blink } => {
                    let mut e = match center {
                        Some(cw) => {
                            let mut node = absolute(*x, *y, *cw, font);
                            node.justify_content = JustifyContent::Center;
                            let mut e = p.spawn(node);
                            e.with_children(|c| {
                                let mut t = c.spawn(label(text, *color, *bold, font));
                                if let Some(k) = blink {
                                    t.insert(Blink(*k));
                                }
                            });
                            e
                        }
                        None => p.spawn((
                            Node { position_type: PositionType::Absolute, left: Val::Px(*x), top: Val::Px(*y), ..default() },
                            label(text, *color, *bold, font),
                        )),
                    };
                    if center.is_none()
                        && let Some(k) = blink
                    {
                        e.insert(Blink(*k));
                    }
                }
                Piece::Tooltip { cx, y, text } => {
                    // Caja negra de alfa 180, 6 más ancha que el texto (blanco, `g_hFont`).
                    let guess = text.chars().count() as f32 * font * 0.5 + 6.0;
                    p.spawn((
                        TooltipBox { cx: *cx },
                        Node {
                            position_type: PositionType::Absolute,
                            left: Val::Px(cx - guess / 2.0),
                            top: Val::Px(*y),
                            padding: UiRect::horizontal(Val::Px(3.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba_u8(0, 0, 0, 180)),
                    ))
                    .with_children(|c| {
                        c.spawn(label(text, WHITE, false, font));
                    });
                }
            }
        }
    });
    *last = (Some(root), pieces, font);
}

/// La clase y el servidor se turnan: `fAlpha = sin(WorldTime · 0.001) + 1`, la clase con alfa
/// 127·(2 − fAlpha) y el servidor con 127·fAlpha (`RenderSubjectTexts`, 286-290).
fn blink(time: Res<Time>, w: Res<CharInfoWindow>, mut texts: Query<(&Blink, &mut TextColor)>) {
    if !w.open {
        return;
    }
    let f = time.elapsed_secs().sin() + 1.0;
    for (Blink(k), mut color) in &mut texts {
        let a = if *k == 0 { 127.0 * (2.0 - f) } else { 127.0 * f } / 255.0;
        if (color.0.alpha() - a).abs() > 1e-3 {
            color.0.set_alpha(a);
        }
    }
}

/// La ayuda va centrada bajo el botón y, si se sale por la derecha de la pantalla de 640, se
/// mete (`_iTempWidth > 640`, NewUIButton.cpp:450-454). Se coloca cuando se sabe lo que mide.
fn place_tooltips(mut boxes: Query<(&TooltipBox, &ComputedNode, &mut Node)>) {
    for (t, computed, mut node) in &mut boxes {
        let width = computed.size().x * computed.inverse_scale_factor();
        if width <= 0.0 {
            continue;
        }
        // La caja empieza en el centro menos medio texto y mide el texto + 6.
        let mut left = t.cx - (width - 6.0) / 2.0;
        if POS.x + left + width > 640.0 {
            left = 640.0 - POS.x - width;
        }
        if node.left != Val::Px(left) {
            node.left = Val::Px(left);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view_of(class: Class, level: u16, s: [u16; 5], points: u16) -> View {
        View {
            name: "Prueba".into(),
            class,
            pk: 3,
            points: Points { level, strength: s[0], agility: s[1], vitality: s[2], energy: s[3], command: s[4] },
            level_up_points: points,
            experience: 0,
            next_experience: 100,
            life: 110,
            life_max: 110,
            mana: 20,
            mana_max: 20,
            add_point: 0,
            max_add_point: 0,
            minus_point: 0,
            max_minus_point: 0,
            server: Some(SelectedServer { name: "Mu".into(), index: 1, non_pvp: 0 }),
            eq: EquipBonus::default(),
            skill_damage: 0,
            skill_is_summon: false,
        }
    }

    fn strings(v: &View) -> Vec<String> {
        v.texts()
            .into_iter()
            .filter_map(|p| match p {
                Piece::Text { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn dark_knight_nivel_1() {
        let t = strings(&view_of(Class::Knight, 1, [28, 20, 25, 10, 0], 0));
        for s in [
            "Prueba",
            "(Dark Knight)",
            "[Mu-1 Server]",
            "Level: 1",
            "Exp : 0/100",
            "[+]:100%|[-]:100%",
            "Create 0/0 | Decrease 0/0",
            "STR",
            "28",
            "Dmg(rate): 4~7 (42)",
            "Attack rate: 93",
            "Defense (rate):6 (6)",
            "Attack speed: 1",
            "Defense rate: 12",
            "HP: 110 / 110",
            "Mana: 20 / 20",
            "Skill Damage: 201%",
        ] {
            assert!(t.iter().any(|x| x == s), "falta {s:?} en {t:?}");
        }
        assert!(!t.iter().any(|x| x.starts_with("Point")), "sin puntos no sale");
        assert!(!t.iter().any(|x| x == "Command"), "solo el Dark Lord");
        assert!(!t.iter().any(|x| x.starts_with("Wizardry")));
    }

    #[test]
    fn magos_y_lord() {
        let t = strings(&view_of(Class::Wizard, 1, [18, 18, 15, 30, 0], 5));
        assert!(t.contains(&"Point: 5".to_string()));
        assert!(t.contains(&"Wizardry Dmg: 3~7".to_string()));
        assert!(t.contains(&"Attack speed: 1".to_string()), "el Dark Wizard enseña MagicSpeed");
        assert!(!t.iter().any(|x| x.starts_with("Skill Damage")));

        let t = strings(&view_of(Class::Summoner, 1, [21, 21, 18, 23, 0], 0));
        assert!(t.contains(&"Wizardry Dmg: 2~5".to_string()));
        assert!(t.contains(&" Curse Spell: 2 ~ 5".to_string()));

        let t = strings(&view_of(Class::Lord, 1, [26, 20, 20, 15, 25], 0));
        assert!(t.contains(&"Command".to_string()) && t.contains(&"25".to_string()));
        assert!(t.contains(&"Dmg(rate): 4~6 (61)".to_string()));
        assert!(t.contains(&"Skill Damage: 200%".to_string()));

        let t = strings(&view_of(Class::Fighter, 1, [32, 27, 25, 20, 0], 0));
        assert!(t.contains(&"Melee Damage: 52%".to_string()));
        assert!(t.contains(&"Divine Damage (Roar, Slasher): 52%".to_string()));
        assert!(t.contains(&"AOE Damage (Dark Side): 105%".to_string()));
    }

    #[test]
    fn frutos_y_equipo() {
        let mut v = view_of(Class::Knight, 150, [400, 200, 200, 50, 0], 0);
        (v.add_point, v.max_add_point, v.minus_point, v.max_minus_point) = (40, 100, 5, 60);
        let t = strings(&v);
        assert!(t.contains(&"Create 40/100 | Decrease -5/-60".to_string()));
        assert!(t.contains(&"[+]:50%|[-]:100%".to_string()), "40 de 100 = 40 %: 50; 5 ≤ 10: 100");
        assert_eq!(fruit_rate(15, 200), 70, "7 %");
        assert_eq!(fruit_rate(11, 100), 60, "11 %");
        assert_eq!(fruit_rate(80, 100), 40);
        assert_eq!(fruit_rate(20, 0), 70, "sin máximo, 0 %");

        // Con equipo: el valor va en azul y la defensa con el conjunto enseña el +10 %.
        v.eq = EquipBonus { add_strength: 10, full_set: true, pvp_attack_bonus: 7, ..Default::default() };
        let texts = v.texts();
        let blue_str = texts.iter().any(|p| matches!(p, Piece::Text { text, color, .. } if text == "410" && *color == BLUE));
        assert!(blue_str);
        let t = strings(&v);
        assert!(t.iter().any(|x| x.starts_with("Attack rate: ") && x.ends_with(" (+7)")));
        assert!(t.contains(&"Defense (rate):66 (66 +6)".to_string()));
    }

    #[test]
    fn botones() {
        let v = view_of(Class::Knight, 1, [28, 20, 25, 10, 0], 3);
        let count = |p: &[Piece]| p.iter().filter(|p| matches!(p, Piece::Image { tex: Tex::BtnStat, .. })).count();
        assert_eq!(count(&pieces(&v, None, false)), 4);
        assert_eq!(count(&pieces(&view_of(Class::Lord, 1, [26, 20, 20, 15, 25], 3), None, false)), 5);
        assert_eq!(count(&pieces(&view_of(Class::Knight, 1, [28, 20, 25, 10, 0], 0), None, false)), 0, "sin puntos, sin +");
        // Apretado encima de un "+": fotograma 1 (15 más abajo en la textura).
        let at = Vec2::new(165.0, ROWS[1] + 5.0);
        let p = pieces(&v, Some(at), true);
        assert!(p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::BtnStat, y, sv, .. } if *y == ROWS[1] + 2.0 && *sv == 15.0)));
        // Ayuda del botón de cerrar.
        let p = pieces(&v, Some(Vec2::new(20.0, 400.0)), false);
        assert!(p.iter().any(|p| matches!(p, Piece::Tooltip { text, .. } if text == "Close (C)")));
    }

    #[test]
    fn letra() {
        assert_eq!(font_size(1024), 13.0 * 640.0 / 1024.0);
        assert_eq!(font_size(1920), 14.0 * 640.0 / 1280.0);
    }
}
