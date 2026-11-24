//! Character-selection windows in the login_ui.rs manager (`CUIMng::CreateCharacterScene`,
//! UIMng.cpp:255): character creation (`CCharMakeWin`), the create and delete messages (`CMsgWin`), the
//! system menu with "Select Server" and the options. The bottom bar, the balloons and the 3D belong to
//! charselect.rs; the two share what they need through `CharSceneLink`.
//!
//! Create and delete go to the server; while it answers "Please wait" is shown (`MESSAGE_WAIT`). For
//! deleting, the code is the account password.

use bevy::ui::UiScale;

use super::*;
use crate::account::{self, Class};
use crate::charselect::{CharSceneLink, SceneEvent, UiRequest};

/// `CCharMakeWin`: 454×406 `m_winBack`, centered (UIMng.cpp).
const CMW_W: i32 = 454;
const CMW_H: i32 = 406;
/// The preview: `BeginOpengl(m_winBack, 410, 335)`.
const PREVIEW_W: i32 = 410;
const PREVIEW_H: i32 = 335;
const JOB_W: i32 = 108;
const JOB_H: i32 = 26;
/// `CLRDW_GRAY`: text of disabled class buttons.
#[allow(dead_code)]
const GRAY: [u8; 3] = [128, 128, 128];

fn cmw_origin(s: Screen) -> IVec2 {
    IVec2::new((s.w - CMW_W) / 2, (s.h - CMW_H) / 2)
}

/// `CCharMakeWin::SetPosition` with `PBG_ADD_NEWCHAR_MONK`: Dark Wizard, Dark Knight and Elf from
/// y + 131 with the Summoner below; the Rage Fighter at y + 246 with Magic Gladiator and Dark Lord below.
fn job_rect(s: Screen, class: Class) -> IRect {
    let o = cmw_origin(s);
    let x = o.x + 346;
    let y = match class {
        Class::Wizard | Class::Knight | Class::Elf => o.y + 131 + class.index() as i32 * JOB_H,
        Class::Summoner => o.y + 131 + 3 * JOB_H,
        Class::Fighter => o.y + 246,
        Class::Gladiator => o.y + 246 + JOB_H,
        Class::Lord => o.y + 246 + 2 * JOB_H,
    };
    IRect::new(x, y, JOB_W, JOB_H)
}

/// `GlobalText` 1705-1709, 1690 and 3152.
fn description(class: Class) -> &'static str {
    match class {
        Class::Wizard => "Kingdom of wizards, descendant of Arka. He has a inferior physical condition but has a enormous power and can command attacking spells freely.",
        Class::Knight => "Kingdom of knights, descendant of Lorencia.With a powerful strength and swordsmanship he can handle most of the close-range weapons.",
        Class::Elf => "Kingdom of elves, descendants of Noria. A master of arrows and bows and commands various spells.",
        Class::Gladiator => "Complex character that has a characteristics of the Dark knight and Dark wizard. Master in a close-range combat and can command spells freely.",
        Class::Lord => "Charismatic character that can command the troops and handle the Dark spirit and Dark horse.",
        Class::Summoner => "Strong mentality and excellent insight creates the most powerful curse spell. Also this item pulls out another world summoners and use the detrimental sorcery against them.",
        Class::Fighter => "Legitimate bearers of the Karutan Royal Knights and martial artists of great physical strength. They also help other party memgers by casting subsidiary buffs.",
    }
}

/// `SeparateTextIntoLines` with `max_lines` lines of `row` − 1 characters: breaks at spaces and
/// whatever does not fit is lost.
fn wrap(text: &str, row: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = vec![String::new()];
    for word in text.split(' ') {
        let last = lines.last_mut().unwrap();
        let need = if last.is_empty() { word.chars().count() } else { last.chars().count() + 1 + word.chars().count() };
        if need <= row - 1 {
            if !last.is_empty() {
                last.push(' ');
            }
            last.push_str(word);
        } else if lines.len() < max_lines {
            lines.push(word.to_string());
        } else {
            break;
        }
    }
    lines
}

/// `CheckName` (ZzzScene.cpp:267) without the word filters: spaces, dots and the in-house
/// names (`GlobalText` 457 and 458).
fn forbidden_name(name: &str) -> bool {
    ["Webzen", "WebZen", "webzen", "WEBZEN", "Operation", " ", "."].iter().any(|w| name.contains(w))
}

/// `CheckSpecialText` (Local.cpp:7): ASCII letters and digits only.
fn special_text(name: &str) -> bool {
    name.chars().any(|c| !c.is_ascii_alphanumeric())
}

impl LoginUi {
    /// The manager in character selection: without the login bar or the server list.
    fn new_character_scene(account: String, count: usize) -> Self {
        let mut ui = LoginUi::new("", Vec::new());
        ui.scene = UiScene::Character;
        ui.order = vec![Win::CharMake, Win::Option, Win::SysMenu, Win::Msg];
        ui.shown.clear();
        ui.active = false;
        ui.pending_active = false;
        ui.focus = None;
        ui.account = account;
        ui.char_count = count;
        // Until the server list arrives (`ReceiveCharacterList` → `UpdateDisplay`).
        ui.net_out.clear();
        ui
    }

    /// What arrives from the server on this screen.
    pub(super) fn char_net_message(&mut self, msg: &mu_protocol::game::ServerMessage) {
        use mu_protocol::game::{self, ServerMessage};
        match msg {
            ServerMessage::CharacterList { unlock_flags, characters, .. } => {
                self.class_unlock = *unlock_flags;
                self.char_count = characters.len();
                // `CCharSelMainWin::UpdateDisplay`: no characters, go straight to creating one.
                self.char_display();
            }
            ServerMessage::CharacterClassCreationUnlock { flags } => self.class_unlock = *flags,
            ServerMessage::CharacterCreated { name, slot, level, class } => {
                // Successful `ReceiveCreateCharacter`: `CloseMsgWin` and the character in its slot.
                self.close_wait();
                self.scene_events.push(SceneEvent::Created(account::StoredCharacter {
                    slot: *slot as usize,
                    name: name.clone(),
                    class: Class::from_index(game::base_class_index(*class)),
                    level: *level as u32,
                    guild_status: 255,
                }));
                self.char_count += 1;
            }
            ServerMessage::CharacterCreationFailed => {
                self.close_wait();
                self.popup(MsgCode::CreateFail);
            }
            ServerMessage::CharacterDeleteResponse { result } => {
                self.close_wait();
                match result {
                    1 => {
                        let Some((_, name, _)) = self.char_selected.take() else { return };
                        self.scene_events.push(SceneEvent::Deleted(name));
                        self.char_count = self.char_count.saturating_sub(1);
                        // `MESSAGE_DELETE_CHARACTER_SUCCESS`: `SelectedHero = -1` and `UpdateDisplay`.
                        self.char_display();
                        self.popup(MsgCode::DeleteSuccess);
                    }
                    0 => self.popup(MsgCode::DeleteGuildWarning),
                    _ => self.popup(MsgCode::ResidentWrong),
                }
            }
            _ => {}
        }
    }

    /// `CloseMsgWin` of the "Please wait".
    fn close_wait(&mut self) {
        if self.is_shown(Win::Msg) && self.msg.code == Some(MsgCode::Wait) {
            self.hide_win(Win::Msg);
        }
    }

    /// Can this class be created? (`g_CharCardEnable` with the server flags: Magic
    /// Gladiator, Dark Lord and Summoner need their card; the Rage Fighter, its own.)
    fn class_enabled(&self, class: Class) -> bool {
        use mu_protocol::game::unlock;
        let bit = match class {
            Class::Gladiator => unlock::MAGIC_GLADIATOR,
            Class::Lord => unlock::DARK_LORD,
            Class::Summoner => unlock::SUMMONER,
            Class::Fighter => unlock::RAGE_FIGHTER,
            _ => return true,
        };
        self.class_unlock & bit != 0
    }

    /// `ShowWin(&m_CharMakeWin)`: the name box empty and focused (`CCharMakeWin::Show`).
    pub(super) fn show_char_make(&mut self) {
        self.show_win(Win::CharMake);
        self.char_name = TextBox::new("", account::MAX_NAME, false);
        self.give_focus(Field::CharName, false);
    }

    /// `CCharSelMainWin::UpdateDisplay`: if no character is left the create window opens.
    pub(super) fn char_display(&mut self) {
        if self.char_count == 0 {
            self.show_char_make();
        }
    }

    /// `CCharSelMainWin::DeleteCharacter`.
    fn delete_character(&mut self) {
        let Some((_, _, guild)) = &self.char_selected else { return };
        if *guild != 255 {
            self.popup(MsgCode::DeleteGuildWarning);
        } else {
            self.popup(MsgCode::DeleteConfirm);
        }
    }

    /// `RequestDeleteCharacter` and its response.
    pub(super) fn request_delete(&mut self) {
        let Some((_, name, _)) = self.char_selected.clone() else { return };
        self.focus = None;
        // `SendRequestDeleteCharacter(ID, code)` and `PopUp(MESSAGE_WAIT)`; the response arrives in
        // `char_net_message`.
        let code = self.del_pass.text.clone();
        self.net_out.push(NetOut::Game(mu_protocol::game::ClientMessage::DeleteCharacter { name, security_code: code }));
        self.popup(MsgCode::Wait);
    }

    /// `CCharMakeWin::RequestCreateCharacter` and `ReceiveCreateCharacter`.
    fn request_create(&mut self) {
        let name = self.char_name.text.clone();
        if name.len() < 4 {
            self.popup(MsgCode::MinLength);
        } else if forbidden_name(&name) {
            self.popup(MsgCode::IdSpaceError);
        } else if special_text(&name) {
            self.popup(MsgCode::SpecialName);
        } else {
            // `SendRequestCreateCharacter`, `HideWin` and `PopUpMsgWin(MESSAGE_WAIT)`.
            let class = mu_protocol::game::class_number(self.job.index() as u8);
            self.net_out.push(NetOut::Game(mu_protocol::game::ClientMessage::CreateCharacter { name, class }));
            self.hide_win(Win::CharMake);
            self.focus = None;
            self.popup(MsgCode::Wait);
        }
    }

    /// `CCharMakeWin` buttons: the classes (`cha_bt`, 4 frames: normal 0, hover 1, pressed
    /// 2 and selected 3) and OK / Cancel.
    pub(super) fn char_make_buttons(&self, s: Screen, out: &mut Vec<ButtonDef>) {
        for class in Class::ALL {
            // Disabled: frame 0 and gray text (`adwJobBtnClr[3]`).
            let enabled = self.class_enabled(class);
            out.push(ButtonDef {
                id: Btn::Job(class),
                rect: job_rect(s, class),
                tex: Tex::GroupBtn,
                frames: if enabled { [0, 2, 1] } else { [0, 0, 0] },
                check_frame: Some(3),
                checked: self.job == class,
                text: Some((class.name().to_string(), if enabled { [BR_GRAY, BR_GRAY, WHITE] } else { [GRAY; 3] })),
            });
        }
        let o = cmw_origin(s);
        out.push(ButtonDef::image(Btn::MakeOk, IRect::new(o.x + 346, o.y + 325, 54, 30), Tex::OkBtn));
        out.push(ButtonDef::image(Btn::MakeCancel, IRect::new(o.x + 400, o.y + 325, 54, 30), Tex::CancelBtn));
    }

    /// `CCharMakeWin::UpdateWhileActive`.
    pub(super) fn char_make_while_active(&mut self, clicks: &[Btn], input: &FrameInput) {
        for &c in clicks {
            if let Btn::Job(class) = c {
                if self.job != class && self.class_enabled(class) {
                    self.job = class;
                    // `UpdateDisplay` → `SelectCreateCharacter`: the new face, from action 1.
                    self.job_picks += 1;
                }
                return;
            }
        }
        if clicks.contains(&Btn::MakeOk) {
            self.request_create();
        } else if clicks.contains(&Btn::MakeCancel) {
            self.hide_win(Win::CharMake);
            self.focus = None;
        } else if input.enter {
            self.sounds += 1;
            self.request_create();
        } else if input.esc {
            self.sounds += 1;
            self.hide_win(Win::CharMake);
            self.focus = None;
            self.sys_flag = false;
        }
    }

    /// `CCharMakeWin::RenderControls`: the face, the name background, the black boxes of the
    /// stats and the description, the buttons and the texts.
    pub(super) fn push_char_make(&self, s: Screen, out: &mut Vec<Item>) {
        // Full-screen `CWin`: 50 % black.
        out.push(Item::Fill { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, color: [0, 0, 0, 128] });
        let o = cmw_origin(s);
        out.push(image(Tex::Preview, IRect::new(o.x, o.y, PREVIEW_W, PREVIEW_H)));
        let input = IRect::new(o.x, o.y + 317, 346, 38);
        out.push(image(Tex::CharId, input));
        let (stats, command) = self.job.base_stats();
        let stat = IRect::new(o.x + 346, o.y + 24, 108, if command.is_some() { 96 } else { 80 });
        out.push(Item::Fill { x: stat.x as f32, y: stat.y as f32, w: stat.w as f32, h: stat.h as f32, color: [0, 0, 0, 143] });
        let desc = IRect::new(o.x, o.y + 355, CMW_W, 51);
        out.push(Item::Fill { x: desc.x as f32, y: desc.y as f32, w: desc.w as f32, h: desc.h as f32, color: [0, 0, 0, 143] });
        self.push_buttons(s, Win::CharMake, out);

        let text = |x: i32, y: i32, text: &str, color| Item::Text {
            x: s.snap_x(x as f32),
            y: s.snap_y(y as f32),
            center: None,
            text: text.to_string(),
            color,
        };
        // `GlobalText` 1701-1704 and 1738, with the value in orange 54 further right.
        let labels = ["Strength", "Agility", "Vitality", "Energy", "Command"];
        let values: Vec<&str> = stats.iter().copied().chain(command).collect();
        for (i, value) in values.iter().enumerate() {
            let y = stat.y + 10 + i as i32 * 17;
            out.push(text(stat.x + 22 + 54, y, value, ORANGE));
            out.push(text(stat.x + 22, y, labels[i], WHITE));
        }
        for (i, line) in wrap(description(self.job), 75, 2).iter().enumerate() {
            out.push(text(desc.x + 10, desc.y + 12 + i as i32 * 19, line, WHITE));
        }
        out.push(self.input_item(s, Field::CharName, Vec2::new(s.snap_x((input.x + 78) as f32), s.snap_y((input.y + 21) as f32))));
    }
}

/// `CUIMng::CreateCharacterScene`: the manager and its window layer, above the bar and the
/// balloons of charselect.rs.
pub(super) fn spawn_character_ui(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    commands.insert_resource(LoginUi::new_character_scene(account::current_account(), 0));
    let textures: HashMap<Tex, Handle<Image>> = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|(h, _)| (t, h)))
        .collect();
    commands.insert_resource(LoginTextures(textures));
    commands
        .spawn((
            Name::new("Ventanas de la selección de personaje"),
            LoginUiRoot,
            GlobalZIndex(10),
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
        ))
        .with_children(|root| {
            root.spawn((
                WindowsLayer,
                Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            ));
        });
}

/// Before the manager pass: what charselect.rs shares and its bar buttons.
pub(super) fn before_update(
    ui: Option<ResMut<LoginUi>>,
    link: Option<ResMut<CharSceneLink>>,
    textures: Option<ResMut<LoginTextures>>,
) {
    let (Some(mut ui), Some(mut link)) = (ui, link) else { return };
    ui.account = link.account.clone();
    ui.char_count = link.count;
    ui.char_selected = link.selected.clone();
    for req in std::mem::take(&mut link.to_ui) {
        match req {
            // `CCharSelMainWin::UpdateWhileActive`.
            UiRequest::SysMenu => {
                ui.show_win(Win::SysMenu);
                ui.sys_flag = true;
            }
            UiRequest::Create => ui.show_char_make(),
            UiRequest::Delete => ui.delete_character(),
        }
    }
    if let (Some(mut textures), Some(img)) = (textures, link.preview_image.clone()) {
        if textures.0.get(&Tex::Preview) != Some(&img) {
            textures.0.insert(Tex::Preview, img);
        }
    }
}

/// After: whether any window covers the screen, what was created or deleted, and the preview.
pub(super) fn after_update(
    ui: Option<ResMut<LoginUi>>,
    link: Option<ResMut<CharSceneLink>>,
    windows: Query<&Window>,
    scale: Res<UiScale>,
) {
    let (Some(mut ui), Some(mut link)) = (ui, link) else { return };
    let blocked = [Win::CharMake, Win::Msg, Win::SysMenu, Win::Option].iter().any(|&w| ui.is_shown(w));
    if link.blocked != blocked {
        link.blocked = blocked;
    }
    link.to_scene.append(&mut ui.scene_events);
    link.preview = ui.is_shown(Win::CharMake).then_some((ui.job, ui.job_picks));
    // Real window pixels for the preview image.
    let px_per_ref = K * scale.0 * windows.single().map_or(1.0, |w| w.resolution.scale_factor());
    let size = UVec2::new((PREVIEW_W as f32 * px_per_ref).round() as u32, (PREVIEW_H as f32 * px_per_ref).round() as u32);
    if link.preview_size != size {
        link.preview_size = size;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres() {
        assert!(special_text("ab_c"));
        assert!(!special_text("Abc123"));
        assert!(forbidden_name("xWEBZENx"));
        assert!(!forbidden_name("crixpx"));
    }

    #[test]
    fn descripcion_en_dos_lineas() {
        let l = wrap(description(Class::Summoner), 75, 2);
        assert_eq!(l.len(), 2);
        assert!(l.iter().all(|l| l.chars().count() <= 74));
    }
}
