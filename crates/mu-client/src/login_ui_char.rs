//! Ventanas de la selección de personaje en el gestor de login_ui.rs (`CUIMng::CreateCharacterScene`,
//! UIMng.cpp:255): crear personaje (`CCharMakeWin`), los mensajes de crear y borrar (`CMsgWin`), el
//! menú de sistema con "Select Server" y las opciones. La barra de abajo, los globos y el 3D son de
//! charselect.rs; se cuentan lo necesario por `CharSceneLink`.
//!
//! Sin servidor: crear y borrar van a `account.rs` y la respuesta llega enseguida
//! (`ReceiveCreateCharacter` / `ReceiveDeleteCharacter`). Para borrar vale cualquier contraseña no
//! vacía (no se guardan contraseñas).

use bevy::ui::UiScale;

use super::*;
use crate::account::{self, Class, CreateError};
use crate::charselect::{CharSceneLink, SceneEvent, UiRequest};

/// `CCharMakeWin`: `m_winBack` de 454×406, centrado (UIMng.cpp).
const CMW_W: i32 = 454;
const CMW_H: i32 = 406;
/// La vista previa: `BeginOpengl(m_winBack, 410, 335)`.
const PREVIEW_W: i32 = 410;
const PREVIEW_H: i32 = 335;
const JOB_W: i32 = 108;
const JOB_H: i32 = 26;
/// `CLRDW_GRAY`: texto de los botones de clase deshabilitados.
#[allow(dead_code)]
const GRAY: [u8; 3] = [128, 128, 128];

fn cmw_origin(s: Screen) -> IVec2 {
    IVec2::new((s.w - CMW_W) / 2, (s.h - CMW_H) / 2)
}

/// `CCharMakeWin::SetPosition` (Main 5.2, sin el Rage Fighter): Dark Wizard, Dark Knight y Elf
/// desde y + 131, la Summoner debajo; Magic Gladiator y Dark Lord desde y + 246.
fn job_rect(s: Screen, class: Class) -> IRect {
    let o = cmw_origin(s);
    let x = o.x + 346;
    let y = match class {
        Class::Wizard | Class::Knight | Class::Elf => o.y + 131 + class.index() as i32 * JOB_H,
        Class::Summoner => o.y + 131 + 3 * JOB_H,
        Class::Gladiator => o.y + 246,
        Class::Lord => o.y + 246 + JOB_H,
    };
    IRect::new(x, y, JOB_W, JOB_H)
}

/// `GlobalText` 1705-1709 y 1690.
fn description(class: Class) -> &'static str {
    match class {
        Class::Wizard => "Kingdom of wizards, descendant of Arka. He has a inferior physical condition but has a enormous power and can command attacking spells freely.",
        Class::Knight => "Kingdom of knights, descendant of Lorencia.With a powerful strength and swordsmanship he can handle most of the close-range weapons.",
        Class::Elf => "Kingdom of elves, descendants of Noria. A master of arrows and bows and commands various spells.",
        Class::Gladiator => "Complex character that has a characteristics of the Dark knight and Dark wizard. Master in a close-range combat and can command spells freely.",
        Class::Lord => "Charismatic character that can command the troops and handle the Dark spirit and Dark horse.",
        Class::Summoner => "Strong mentality and excellent insight creates the most powerful curse spell. Also this item pulls out another world summoners and use the detrimental sorcery against them.",
    }
}

/// `SeparateTextIntoLines` con `max_lines` líneas de `row` − 1 caracteres: corta por espacios y
/// lo que no cabe se pierde.
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

/// `CheckName` (ZzzScene.cpp:267) sin los filtros de palabras: espacios, puntos y los nombres de
/// la casa (`GlobalText` 457 y 458).
fn forbidden_name(name: &str) -> bool {
    ["Webzen", "WebZen", "webzen", "WEBZEN", "Operation", " ", "."].iter().any(|w| name.contains(w))
}

/// `CheckSpecialText` (Local.cpp:7): solo letras y números ASCII.
fn special_text(name: &str) -> bool {
    name.chars().any(|c| !c.is_ascii_alphanumeric())
}

impl LoginUi {
    /// El gestor en la selección de personaje: sin la barra del login ni la lista de servidores.
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
        // `CCharSelMainWin::UpdateDisplay`: sin personajes, directamente a crear uno.
        ui.char_display();
        ui
    }

    /// `ShowWin(&m_CharMakeWin)`: la caja del nombre vacía y con el foco (`CCharMakeWin::Show`).
    pub(super) fn show_char_make(&mut self) {
        self.show_win(Win::CharMake);
        self.char_name = TextBox::new("", account::MAX_NAME, false);
        self.give_focus(Field::CharName, false);
    }

    /// `CCharSelMainWin::UpdateDisplay`: si no queda ningún personaje se abre la ventana de crear.
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

    /// `RequestDeleteCharacter` y su respuesta.
    pub(super) fn request_delete(&mut self) {
        let Some((_, name, _)) = self.char_selected.clone() else { return };
        if self.del_pass.text.is_empty() {
            self.popup(MsgCode::ResidentWrong);
            return;
        }
        account::delete(&self.account, &name);
        self.scene_events.push(SceneEvent::Deleted(name));
        self.char_count = self.char_count.saturating_sub(1);
        self.char_selected = None;
        self.focus = None;
        // `MESSAGE_DELETE_CHARACTER_SUCCESS`: `SelectedHero = -1` y `UpdateDisplay`.
        self.char_display();
        self.popup(MsgCode::DeleteSuccess);
    }

    /// `CCharMakeWin::RequestCreateCharacter` y `ReceiveCreateCharacter`.
    fn request_create(&mut self) {
        let name = self.char_name.text.clone();
        if name.len() < 4 {
            self.popup(MsgCode::MinLength);
        } else if forbidden_name(&name) {
            self.popup(MsgCode::IdSpaceError);
        } else if special_text(&name) {
            self.popup(MsgCode::SpecialName);
        } else {
            self.hide_win(Win::CharMake);
            self.focus = None;
            match account::create(&self.account, &name, self.job) {
                Ok(c) => {
                    self.scene_events.push(SceneEvent::Created(c));
                    self.char_count += 1;
                }
                Err(CreateError::NameTaken) => self.popup(MsgCode::CreateFail),
                Err(CreateError::Full) => self.popup(MsgCode::CreateFail2),
            }
        }
    }

    /// Botones de `CCharMakeWin`: las clases (`cha_bt`, 4 fotogramas: normal 0, encima 1, pulsado
    /// 2 y elegido 3) y OK / Cancel.
    pub(super) fn char_make_buttons(&self, s: Screen, out: &mut Vec<ButtonDef>) {
        for class in Class::ALL {
            out.push(ButtonDef {
                id: Btn::Job(class),
                rect: job_rect(s, class),
                tex: Tex::GroupBtn,
                frames: [0, 2, 1],
                check_frame: Some(3),
                checked: self.job == class,
                text: Some((class.name().to_string(), [BR_GRAY, BR_GRAY, WHITE])),
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
                if self.job != class {
                    self.job = class;
                    // `UpdateDisplay` → `SelectCreateCharacter`: la cara nueva, desde la acción 1.
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

    /// `CCharMakeWin::RenderControls`: la cara, el fondo del nombre, las cajas negras de las
    /// estadísticas y la descripción, los botones y los textos.
    pub(super) fn push_char_make(&self, s: Screen, out: &mut Vec<Item>) {
        // `CWin` de toda la pantalla: negro al 50 %.
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
        // `GlobalText` 1701-1704 y 1738, con el valor en naranja 54 más a la derecha.
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

/// `CUIMng::CreateCharacterScene`: el gestor y su capa de ventanas, por encima de la barra y los
/// globos de charselect.rs.
pub(super) fn spawn_character_ui(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let account = account::current_account();
    let count = account::characters(&account).len();
    commands.insert_resource(LoginUi::new_character_scene(account, count));
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

/// Antes de la pasada del gestor: lo que cuenta charselect.rs y los botones de su barra.
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

/// Después: si alguna ventana tapa la pantalla, lo creado o borrado, y la vista previa.
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
    // Píxeles reales de la ventana para la imagen de la vista previa.
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
