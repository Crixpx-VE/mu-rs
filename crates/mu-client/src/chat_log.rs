//! Lo que guarda y decide el chat, sin dibujar nada (lo dibuja y lo conecta chat.rs):
//!
//! - [`ChatLog`]: `SEASON3B::CNewUIChatLogWindow` (NewUIChatLogWindow.cpp): los mensajes por tipo,
//!   hasta 200 por lista, partidos en dos líneas si no caben (`SeparateText`), la línea del final que
//!   se ve, el desplazamiento automático, el tamaño (3 a 15 líneas, de 3 en 3), la transparencia
//!   del fondo, los filtros de `/filter` y el ratón (rueda, barra, cambiar de tamaño, clic derecho en
//!   un nombre).
//! - [`SystemLog`]: la ventana de mensajes del sistema de arriba a la izquierda del S6
//!   (`CSystemLogWindow` de MuMain, que el Main 5.2 aún no tiene: allí van al chat), 6 líneas.
//! - [`ChatInput`]: `SEASON3B::CNewUIChatInputBox` (NewUIChatInputBox.cpp): las dos cajas de texto,
//!   el historial, el tipo de mensaje y lo que pasa al pulsar Enter (`UpdateKeyEvent`, :464-552,
//!   con `CheckCommand`, `SendChat` y `SendChatWhisper`).
//!
//! **S6:** la barra del chat de los datos (`newui_chat_back`) mide 281 y trae el botón de Gens,
//! como el `CChatInputBox` de MuMain ("modernizado para el Season 6 Episodio 3"); el Main 5.2
//! tiene la de 254. Se siguen las medidas de MuMain (`CHATBOX_WIDTH` = 281, `CLIENT_WIDTH` =
//! 281 − 7·2 − 4·2) y su ventana de mensajes del sistema; el resto es el Main 5.2.
//!
//! Las medidas de texto son de la pantalla de 640×480 (la letra de `g_hFont` a 1024×768).

/// `MESSAGE_TYPE` (NewUIChatLogWindow.h:18-32, con `TYPE_GENS_MESSAGE` de MuMain).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum MsgType {
    All = 0,
    Chat,
    Whisper,
    System,
    Error,
    Party,
    Guild,
    Union,
    Gens,
    /// Sin GMs todavía (`ReceiveChat` los reconoce por `eBuff_GMEffect` o `CtlCode`).
    #[allow(dead_code)]
    Gm,
}

const NUMBER_OF_TYPES: usize = 10;

impl MsgType {
    /// Los que tienen un nombre al que se puede susurrar con el botón derecho
    /// (`UpdateMouseEvent`, NewUIChatLogWindow.cpp:707-713).
    fn has_sender(self) -> bool {
        matches!(self, MsgType::Whisper | MsgType::Chat | MsgType::Party | MsgType::Guild | MsgType::Union | MsgType::Gens | MsgType::Gm)
    }
}

/// Colores de una línea: fondo RGBA, letra RGBA y si va en negrita (`RenderMessages`,
/// NewUIChatLogWindow.cpp:115-164; Gens de chat_log.rcss de MuMain). `None` = no se dibuja.
pub fn line_colors(kind: MsgType, frame: bool, pointed: bool) -> Option<([u8; 4], [u8; 4], bool)> {
    let (bg, text, bold) = match kind {
        MsgType::Whisper => ([255, 200, 50, 150], [0, 0, 0, 255], false),
        MsgType::System => ([0, 0, 0, 150], [100, 150, 255, 255], false),
        MsgType::Error => ([0, 0, 0, 150], [255, 30, 0, 255], false),
        // Con el fondo puesto, el negro de las líneas baja de 150 a 100.
        MsgType::Chat => ([0, 0, 0, if frame { 100 } else { 150 }], [205, 220, 239, 255], false),
        MsgType::Party => ([0, 200, 255, 150], [0, 0, 0, 255], false),
        MsgType::Guild => ([0, 255, 150, 200], [0, 0, 0, 255], false),
        MsgType::Union => ([200, 200, 0, 200], [0, 0, 0, 255], false),
        MsgType::Gens => ([150, 200, 100, 200], [0, 0, 0, 255], false),
        MsgType::Gm => ([30, 30, 30, 200], [250, 200, 50, 255], true),
        MsgType::All => return None,
    };
    // La línea bajo el ratón (`m_bPointedMessage`), solo si tiene nombre (lo mira quien llama).
    if pointed {
        return Some(([30, 30, 30, 180], [255, 128, 255, 255], bold));
    }
    Some((bg, text, bold))
}

/// Una línea del chat (`CMessageText`).
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Line {
    pub id: String,
    pub text: String,
    pub kind: MsgType,
}

impl Line {
    /// Lo que se escribe: `ID : texto` o solo el texto.
    pub fn shown(&self) -> String {
        if self.id.is_empty() { self.text.clone() } else { format!("{} : {}", self.id, self.text) }
    }
}

// --- Medidas de la ventana del chat (pantalla de 640×480) --------------------------------------

/// `CChatInputBox::CHATBOX_WIDTH` de MuMain (la barra del S6).
pub const WND_WIDTH: i32 = 281;
pub const FONT_LEADING: i32 = 4;
pub const WND_TOP_BOTTOM_EDGE: i32 = 2;
pub const WND_LEFT_RIGHT_EDGE: i32 = 4;
pub const RESIZING_BTN_HEIGHT: i32 = 10;
pub const SCROLL_BAR_WIDTH: i32 = 7;
pub const SCROLL_TOP_BOTTOM_PART_HEIGHT: i32 = 3;
pub const SCROLL_MIDDLE_PART_HEIGHT: i32 = 15;
pub const SCROLL_BTN_WIDTH: i32 = 15;
pub const SCROLL_BTN_HEIGHT: i32 = 30;
/// Ancho del texto (`CLIENT_WIDTH` de MuMain).
pub const CLIENT_WIDTH: f32 = (WND_WIDTH - SCROLL_BAR_WIDTH * 2 - WND_LEFT_RIGHT_EDGE * 2) as f32;
const MAX_NUMBER_OF_LINES: usize = 200;
/// `Create(m_pNewUIMng, 0, 480-50-47, 6)` (NewUISystem.cpp:104): `m_WndPos` es la esquina de abajo.
pub const LOG_X: i32 = 0;
pub const LOG_BOTTOM: i32 = 480 - 50 - 47;

/// `CheckMouseIn`.
fn inside(m: (i32, i32), x: i32, y: i32, w: i32, h: i32) -> bool {
    m.0 >= x && m.0 < x + w && m.1 >= y && m.1 < y + h
}

/// `SeparateText` (NewUIChatLogWindow.cpp:910-938): la primera línea, que cabe en `CLIENT_WIDTH`
/// menos `ID : `, y el resto. Se corta en el último espacio que deje caber la primera (el espacio
/// pasa a la segunda); sin espacios, letra a letra. Si con espacios ninguno sirve, el original lee
/// fuera de la cadena: aquí se corta letra a letra.
pub fn separate_text(id: &str, text: &str, measure: &dyn Fn(&str) -> f32) -> (String, String) {
    let max_first = CLIENT_WIDTH - measure(&format!("{id} : "));
    let chars: Vec<char> = text.chars().collect();
    let width = |n: usize| measure(&chars[..n].iter().collect::<String>());
    if width(chars.len()) <= max_first {
        return (text.to_string(), String::new());
    }
    let split = |n: usize| (chars[..n].iter().collect::<String>(), chars[n..].iter().collect::<String>());
    if chars.contains(&' ') {
        let mut at = chars.len();
        while let Some(p) = chars[..at].iter().rposition(|&c| c == ' ') {
            at = p;
            if width(at) <= max_first {
                return split(at);
            }
            if at == 0 {
                break;
            }
        }
    }
    let mut at = chars.len();
    while at > 0 && width(at) > max_first {
        at -= 1;
    }
    split(at)
}

/// `m_EventState` de la ventana del chat.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum LogEvent {
    #[default]
    None,
    ClientHover,
    ScrollBtnDown,
    ResizeHover,
    ResizeDown,
}

/// El ratón en un fotograma (pantalla de 640×480, en enteros como `MouseX`/`MouseY`).
#[derive(Clone, Copy, Default, Debug)]
pub struct MouseIn {
    pub pos: Option<(i32, i32)>,
    /// `MouseLButtonPush`: el botón izquierdo está abajo.
    pub held: bool,
    /// `IsRelease(VK_LBUTTON)`.
    pub release: bool,
    /// `IsPress(VK_RBUTTON)`.
    pub right_press: bool,
    /// `MouseWheel`: > 0 hacia arriba.
    pub wheel: f32,
}

/// Lo que ha hecho el ratón en la ventana del chat.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct MouseOut {
    /// `UpdateMouseEvent` devolvió falso: el clic no llega al juego.
    pub consumed: bool,
    /// La rueda era suya (`MouseWheel = 0`).
    pub wheel_used: bool,
    /// Clic derecho en una línea con nombre: susurrarle.
    pub whisper_to: Option<String>,
}

/// `CNewUIChatLogWindow`.
#[derive(Clone, Debug)]
pub struct ChatLog {
    msgs: Vec<Vec<Line>>,
    current: MsgType,
    /// `m_iCurrentRenderEndLine`.
    end_line: i32,
    /// `m_nShowingLines`.
    showing: i32,
    back_alpha: f32,
    show_frame: bool,
    show_log: bool,
    filters: Vec<String>,
    pub event: LogEvent,
    /// `m_ScrollBtnPos.y` y `m_iGrapRelativePosY`.
    scroll_btn_y: i32,
    grab_y: i32,
    /// `m_bPointedMessage` / `m_iPointedMessageIndex`.
    pointed: Option<usize>,
}

impl Default for ChatLog {
    fn default() -> Self {
        let mut log = Self {
            msgs: vec![Vec::new(); NUMBER_OF_TYPES],
            current: MsgType::All,
            end_line: -1,
            showing: 6,
            back_alpha: 0.6,
            show_frame: false,
            show_log: true,
            filters: Vec::new(),
            event: LogEvent::None,
            scroll_btn_y: 0,
            grab_y: 0,
            pointed: None,
        };
        log.update_scroll_pos();
        log
    }
}

impl ChatLog {
    fn msgs(&self, t: MsgType) -> &Vec<Line> {
        &self.msgs[t as usize]
    }

    pub fn lines(&self, t: MsgType) -> usize {
        self.msgs(t).len()
    }

    pub fn end_line(&self) -> i32 {
        self.end_line
    }

    pub fn showing(&self) -> i32 {
        self.showing
    }

    pub fn back_alpha(&self) -> f32 {
        self.back_alpha
    }

    pub fn show_frame(&self) -> bool {
        self.show_frame
    }

    pub fn set_frame(&mut self, on: bool) {
        self.show_frame = on;
    }

    pub fn show_log(&self) -> bool {
        self.show_log
    }

    pub fn pointed(&self) -> Option<usize> {
        self.pointed
    }

    /// `AddText`: `err` es la otra lista a la que va también un error (`TYPE_WHISPER_MESSAGE` para
    /// "No users"). Devuelve si suena el aviso de susurro porque pasó el filtro de `/filter` (con
    /// la opción puesta, lo decide quien llama).
    pub fn add_text(&mut self, id: &str, text: &str, kind: MsgType, err: MsgType, measure: &dyn Fn(&str) -> f32) -> bool {
        if id.is_empty() && text.is_empty() {
            return false;
        }
        if self.lines(kind) >= MAX_NUMBER_OF_LINES {
            self.remove_front_line(kind);
        }
        if self.lines(MsgType::All) >= MAX_NUMBER_OF_LINES {
            self.remove_front_line(MsgType::All);
        }
        if self.filters.is_empty() || kind != MsgType::Chat {
            self.process_add_text(id, text, kind, err, measure);
            false
        } else if self.check_filter(id) || self.check_filter(text) {
            self.process_add_text(id, text, kind, err, measure);
            true
        } else {
            false
        }
    }

    /// `ProcessAddText` (NewUIChatLogWindow.cpp:313-465).
    fn process_add_text(&mut self, id: &str, text: &str, kind: MsgType, err: MsgType, measure: &dyn Fn(&str) -> f32) {
        let pieces: Vec<(String, String)> = if text.len() >= 20 {
            let (t1, t2) = separate_text(id, text, measure);
            [(id.to_string(), t1), (String::new(), t2)].into_iter().filter(|(_, t)| !t.is_empty()).collect()
        } else {
            vec![(id.to_string(), text.to_string())]
        };
        let mut scroll_lines = 0;
        for (id, text) in pieces {
            let line = Line { id, text, kind };
            self.msgs[kind as usize].push(line.clone());
            self.msgs[MsgType::All as usize].push(line.clone());
            if kind == MsgType::Error && err != MsgType::Error && err != MsgType::All {
                self.msgs[err as usize].push(line);
            }
            if self.current == MsgType::All || self.current == kind {
                scroll_lines += 1;
            }
        }
        // Desplazamiento automático (con la resta en `size_t` del original: si sale negativa, no).
        let size = self.lines(self.current) as i64;
        let diff = size - (self.end_line as i64 + 1) - scroll_lines;
        if scroll_lines > 0 && (0..3).contains(&diff) || !self.show_frame {
            self.end_line = size as i32 - 1;
        }
    }

    /// `RemoveFrontLine`.
    fn remove_front_line(&mut self, kind: MsgType) {
        let v = &mut self.msgs[kind as usize];
        if !v.is_empty() {
            v.remove(0);
        }
        if kind == self.current {
            self.scrolling(self.end_line);
        }
    }

    /// `Scrolling`: la línea del final que se ve, sin pasarse.
    pub fn scrolling(&mut self, end: i32) {
        let size = self.lines(self.current) as i32;
        self.end_line = if size <= self.showing {
            size - 1
        } else if end < self.showing {
            self.showing - 1
        } else if end >= size {
            size - 1
        } else {
            end
        };
    }

    /// `SetNumberOfShowingLines`: de 3 en 3, de 3 a 15 (y al pasarse de 15, otra vez 3).
    pub fn set_showing_lines(&mut self, n: i32) {
        self.showing = (n / 3) * 3;
        if self.showing < 3 || self.showing > 15 {
            self.showing = 3;
        }
        if self.showing > self.end_line {
            self.scrolling(self.showing - 1);
        }
        self.update_scroll_pos();
    }

    /// `SetSizeAuto` (F4 y el botón de tamaño).
    pub fn size_auto(&mut self) {
        self.set_showing_lines(self.showing + 3);
    }

    /// `SetBackAlphaAuto`: 0.2, 0.4 ... hasta pasar de 0.9.
    pub fn back_alpha_auto(&mut self) {
        self.back_alpha += 0.2;
        if self.back_alpha > 0.9 {
            self.back_alpha = 0.2;
        }
    }

    /// `ChangeMessage` (F2 y el botón del sistema del Main 5.2).
    pub fn change_message(&mut self, t: MsgType) {
        self.current = t;
        self.end_line = self.lines(t) as i32 - 1;
    }

    pub fn show_chat_log(&mut self) {
        self.show_log = true;
        self.end_line = self.lines(self.current) as i32 - 1;
    }

    pub fn hide_chat_log(&mut self) {
        self.show_log = false;
    }

    /// `m_WndSize.cy` (`UpdateWndSize`).
    pub fn wnd_height(&self) -> i32 {
        SCROLL_MIDDLE_PART_HEIGHT * self.showing + SCROLL_TOP_BOTTOM_PART_HEIGHT * 2 + WND_TOP_BOTTOM_EDGE * 2
    }

    /// Arriba de la ventana.
    pub fn wnd_top(&self) -> i32 {
        LOG_BOTTOM - self.wnd_height()
    }

    /// Las líneas que se ven: (índice en la lista actual, la línea, y de arriba)
    /// (`RenderMessages`, NewUIChatLogWindow.cpp:80-106; también lo que mira el ratón).
    pub fn visible(&self) -> Vec<(usize, &Line, i32)> {
        let mut y0 = self.wnd_top() + SCROLL_TOP_BOTTOM_PART_HEIGHT;
        let mut start = 0;
        if self.end_line >= self.showing {
            start = self.end_line - self.showing + 1;
        } else {
            y0 += FONT_LEADING + SCROLL_MIDDLE_PART_HEIGHT * (self.showing - self.end_line - 1);
        }
        let msgs = self.msgs(self.current);
        (start.max(0)..=self.end_line)
            .enumerate()
            .filter_map(|(s, i)| {
                let line = msgs.get(i as usize)?;
                Some((i as usize, line, y0 + FONT_LEADING + SCROLL_MIDDLE_PART_HEIGHT * s as i32))
            })
            .collect()
    }

    /// `m_ScrollBtnPos` (`UpdateScrollPos`, NewUIChatLogWindow.cpp:959-979).
    pub fn scroll_btn(&self) -> (i32, i32) {
        (LOG_X + WND_WIDTH - SCROLL_BAR_WIDTH - WND_LEFT_RIGHT_EDGE - 4, self.scroll_btn_y)
    }

    /// `UpdateScrollPos` (cada fotograma en `Update`, salvo mientras se arrastra el botón).
    pub fn update_scroll_pos(&mut self) {
        let total = self.lines(self.current) as i32;
        let mut rate = 1.0;
        if total > self.showing {
            rate = if self.showing > self.end_line {
                0.0
            } else {
                (self.end_line + 1 - self.showing) as f32 / (total - self.showing) as f32
            };
        }
        if self.event != LogEvent::ScrollBtnDown {
            let cy = self.wnd_height();
            self.scroll_btn_y = LOG_BOTTOM - cy + WND_TOP_BOTTOM_EDGE + ((cy - SCROLL_BTN_HEIGHT - WND_TOP_BOTTOM_EDGE * 2) as f32 * rate) as i32;
        }
    }

    /// La barra para cambiar de tamaño (`ptResizingBtn`), encima de la ventana.
    pub fn resize_bar_y(&self) -> i32 {
        self.wnd_top() - RESIZING_BTN_HEIGHT
    }

    /// El ratón está sobre algo de la ventana que se queda el clic (con el fondo puesto: la barra
    /// de tamaño y el botón de la barra de desplazamiento), o los está arrastrando.
    pub fn blocks(&self, m: (i32, i32)) -> bool {
        if !self.show_frame {
            return false;
        }
        let (bx, by) = self.scroll_btn();
        matches!(self.event, LogEvent::ScrollBtnDown | LogEvent::ResizeDown)
            || inside(m, LOG_X, self.resize_bar_y(), WND_WIDTH, RESIZING_BTN_HEIGHT)
            || inside(m, bx, by, SCROLL_BTN_WIDTH, SCROLL_BTN_HEIGHT)
    }

    /// `UpdateMouseEvent` (NewUIChatLogWindow.cpp:663-848).
    pub fn update_mouse(&mut self, m: &MouseIn) -> MouseOut {
        let mut out = MouseOut::default();
        let Some(pos) = m.pos else { return out };
        let cy = self.wnd_height();
        let top = LOG_BOTTOM - cy;
        let in_wnd = inside(pos, LOG_X, top, WND_WIDTH, cy);
        if self.event == LogEvent::None && !m.held && in_wnd {
            self.event = LogEvent::ClientHover;
            out.consumed = true;
            return out;
        }
        if !m.held && self.event == LogEvent::ClientHover && !in_wnd {
            self.event = LogEvent::None;
            return out;
        }
        if self.event == LogEvent::ClientHover && m.wheel != 0.0 {
            if m.wheel > 0.0 {
                self.scrolling(self.end_line - 1);
            } else {
                self.scrolling(self.end_line + 1);
            }
            out.wheel_used = true;
        }
        self.pointed = None;
        if in_wnd {
            let mut hit = None;
            for (i, line, y) in self.visible() {
                if line.kind.has_sender() && inside(pos, LOG_X + WND_LEFT_RIGHT_EDGE, y, WND_WIDTH, SCROLL_MIDDLE_PART_HEIGHT) {
                    hit = Some((i, line.id.clone()));
                }
            }
            if let Some((i, id)) = hit {
                self.pointed = Some(i);
                if m.right_press && !id.is_empty() {
                    out.whisper_to = Some(id);
                }
            }
        }
        if self.show_frame {
            let (bx, by) = self.scroll_btn();
            if self.event == LogEvent::ClientHover && m.held && inside(pos, bx, by, SCROLL_BTN_WIDTH, SCROLL_BTN_HEIGHT) {
                self.event = LogEvent::ScrollBtnDown;
                self.grab_y = pos.1 - by;
                out.consumed = true;
                return out;
            }
            if self.event == LogEvent::ScrollBtnDown {
                if m.held {
                    let total = self.lines(self.current) as i32;
                    if total > self.showing {
                        let y = pos.1 - self.grab_y;
                        let first = top + WND_TOP_BOTTOM_EDGE;
                        let last = LOG_BOTTOM - SCROLL_BTN_HEIGHT - WND_TOP_BOTTOM_EDGE;
                        if y < first {
                            self.scrolling(self.showing - 1);
                            self.scroll_btn_y = first;
                        } else if y > last {
                            self.scrolling(total - 1);
                            self.scroll_btn_y = last;
                        } else {
                            let rate = (y - first) as f32 / (cy - WND_TOP_BOTTOM_EDGE * 2 - SCROLL_BTN_HEIGHT) as f32;
                            self.scrolling(self.showing + ((total - self.showing) as f32 * rate) as i32);
                            self.scroll_btn_y = y;
                        }
                    }
                    out.consumed = true;
                    return out;
                }
                if m.release {
                    self.event = LogEvent::None;
                    return out;
                }
            }
            let ry = top - RESIZING_BTN_HEIGHT;
            let on_resize = inside(pos, LOG_X, ry, WND_WIDTH, RESIZING_BTN_HEIGHT);
            if self.event == LogEvent::None && !m.held && on_resize {
                self.event = LogEvent::ResizeHover;
                out.consumed = true;
                return out;
            }
            if !m.held && self.event == LogEvent::ResizeHover && !on_resize {
                self.event = LogEvent::None;
                return out;
            }
            if self.event == LogEvent::ResizeHover && m.held && on_resize {
                self.event = LogEvent::ResizeDown;
                out.consumed = true;
                return out;
            }
            if self.event == LogEvent::ResizeDown {
                if m.held {
                    self.resize_to(pos.1, ry);
                    out.consumed = true;
                    return out;
                }
                self.event = LogEvent::None;
                return out;
            }
        }
        out
    }

    /// Las franjas de 3 líneas por encima y por debajo de la barra de tamaño mientras se arrastra
    /// (NewUIChatLogWindow.cpp:807-838).
    fn resize_to(&mut self, my: i32, ry: i32) {
        let band = |y: i32, h: i32| my >= y && my < y + h;
        let mid = SCROLL_MIDDLE_PART_HEIGHT;
        let top_sections = (15 - self.showing) / 3;
        let bottom_sections = (self.showing - 3) / 3;
        for i in 0..top_sections {
            if band(ry - RESIZING_BTN_HEIGHT - (i + 1) * mid * 3 * 2, mid * 3 + RESIZING_BTN_HEIGHT) {
                self.set_showing_lines(self.showing + (i + 1) * 3);
                return;
            }
        }
        for i in 0..bottom_sections {
            if band(ry + RESIZING_BTN_HEIGHT + (i + 1) * mid * 3, RESIZING_BTN_HEIGHT + mid * 3) {
                self.set_showing_lines(self.showing - (i + 1) * 3);
                return;
            }
        }
        if band(0, LOG_BOTTOM - (mid * 15 + RESIZING_BTN_HEIGHT + SCROLL_TOP_BOTTOM_PART_HEIGHT * 2)) {
            self.set_showing_lines(15);
        }
        let h = mid * 3 + SCROLL_TOP_BOTTOM_PART_HEIGHT * 2;
        if band(LOG_BOTTOM - h, h) {
            self.set_showing_lines(3);
        }
    }

    /// `SetFilterText` (`/filter palabra...`, NewUIChatLogWindow.cpp:551-586): hasta 5 palabras
    /// tras el comando; sin ninguna, se quita el filtro. Devuelve el aviso (`GlobalText[755/756]`),
    /// que va al chat como mensaje del sistema.
    pub fn set_filter_text(&mut self, text: &str, measure: &dyn Fn(&str) -> f32) {
        self.filters.clear();
        // `MAX_CHAT_BUFFER_SIZE`: los 60 primeros.
        let text: String = text.chars().take(60).collect();
        let mut tokens = text.split(' ').filter(|t| !t.is_empty()).skip(1).peekable();
        let message = if tokens.peek().is_none() {
            TEXT_FILTER_OFF
        } else {
            for t in tokens.take(5) {
                // `AddFilterWord`: sin repetir y no más de 6.
                if self.filters.len() <= 5 && !self.filters.iter().any(|f| f == t) {
                    self.filters.push(t.to_string());
                }
            }
            TEXT_FILTER_ON
        };
        self.add_text("", message, MsgType::System, MsgType::All, measure);
    }

    /// `CheckFilterText` con `FindText` (está dentro).
    fn check_filter(&self, s: &str) -> bool {
        self.filters.iter().any(|f| s.contains(f.as_str()))
    }
}

/// `GlobalText[755]` y `[756]`.
const TEXT_FILTER_ON: &str = "Filtering has been activated";
const TEXT_FILTER_OFF: &str = "Filtering has been canceled";

/// `CSystemLogWindow` de MuMain: los últimos 6 mensajes del sistema y de error.
#[derive(Clone, Debug, Default)]
pub struct SystemLog {
    lines: Vec<Line>,
}

impl SystemLog {
    pub const MAX_LINES: usize = 6;

    pub fn add(&mut self, text: &str, kind: MsgType) {
        if text.is_empty() {
            return;
        }
        if self.lines.len() >= Self::MAX_LINES {
            self.lines.remove(0);
        }
        self.lines.push(Line { id: String::new(), text: text.to_string(), kind });
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// Azul los del sistema y rojo todo lo demás (`RebuildLineModel` de MuMain).
    pub fn color(kind: MsgType) -> [u8; 3] {
        if kind == MsgType::System { [100, 150, 255] } else { [255, 30, 0] }
    }
}

// --- Cajas de texto ---------------------------------------------------------------------------

/// Una caja de texto (`CUITextInputBox`, un `edit` de Windows de una línea).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextBox {
    pub text: String,
    /// Posición del cursor, en letras.
    pub caret: usize,
    /// Todo seleccionado (`GiveFocus(TRUE)`): lo siguiente que se escriba lo sustituye.
    pub select_all: bool,
    pub max: usize,
    /// Desplazamiento del texto (`ES_AUTOHSCROLL`), en unidades de la pantalla de 640.
    pub scroll: f32,
}

impl TextBox {
    pub fn new(max: usize) -> Self {
        Self { max, ..Default::default() }
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    fn byte_at(&self, i: usize) -> usize {
        self.text.char_indices().nth(i).map_or(self.text.len(), |(b, _)| b)
    }

    /// `SetText`: el cursor al final.
    pub fn set(&mut self, text: &str) {
        self.text = text.chars().take(self.max).collect();
        self.caret = self.len();
        self.select_all = false;
    }

    fn take_selection(&mut self) -> bool {
        if !self.select_all {
            return false;
        }
        self.select_all = false;
        let had = !self.text.is_empty();
        self.text.clear();
        self.caret = 0;
        had
    }

    pub fn insert(&mut self, c: char) {
        self.take_selection();
        if self.len() >= self.max {
            return;
        }
        let b = self.byte_at(self.caret);
        self.text.insert(b, c);
        self.caret += 1;
    }

    pub fn backspace(&mut self) {
        if self.take_selection() || self.caret == 0 {
            return;
        }
        let b = self.byte_at(self.caret - 1);
        self.text.remove(b);
        self.caret -= 1;
    }

    pub fn delete(&mut self) {
        if self.take_selection() || self.caret >= self.len() {
            return;
        }
        let b = self.byte_at(self.caret);
        self.text.remove(b);
    }

    pub fn move_to(&mut self, caret: usize) {
        self.select_all = false;
        self.caret = caret.min(self.len());
    }

    pub fn left(&mut self) {
        let c = if self.select_all { 0 } else { self.caret.saturating_sub(1) };
        self.move_to(c);
    }

    pub fn right(&mut self) {
        let c = if self.select_all { self.len() } else { self.caret + 1 };
        self.move_to(c);
    }

    /// Lo que hay antes y después del cursor.
    pub fn split(&self) -> (&str, &str) {
        self.text.split_at(self.byte_at(self.caret))
    }

    /// `ES_AUTOHSCROLL`: mueve el texto para que se vea el cursor en una caja de `width`.
    pub fn follow_caret(&mut self, width: f32, measure: &dyn Fn(&str) -> f32) {
        let x = measure(self.split().0);
        if x - self.scroll > width - 2.0 {
            self.scroll = x - width + 2.0;
        }
        if x < self.scroll {
            self.scroll = x;
        }
        let total = measure(&self.text);
        if total - self.scroll < width - 2.0 {
            self.scroll = (total - width + 2.0).max(0.0);
        }
    }
}

/// `AddChatHistory` / `AddWhsprIDHistory`: sin repetir (el repetido pasa al final) y, si no
/// estaba, quitando el más viejo cuando ya hay más de `max`.
pub fn add_history(v: &mut Vec<String>, s: &str, max: usize) {
    if let Some(i) = v.iter().position(|h| h == s) {
        v.remove(i);
    } else if v.len() > max {
        v.remove(0);
    }
    v.push(s.to_string());
}

/// Flechas del historial (NewUIChatInputBox.cpp:555-599): `up` resta, `down` suma, dando la vuelta.
pub fn step_history(cur: &mut i32, len: usize, up: bool) {
    *cur += if up { -1 } else { 1 };
    if *cur < 0 {
        *cur = len as i32 - 1;
    }
    if *cur >= len as i32 {
        *cur = 0;
    }
}

// --- Enviar -------------------------------------------------------------------------------------

/// `INPUT_MESSAGE_TYPE`: los botones de tipo (con el de Gens del S6).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum InputType {
    #[default]
    Normal = 0,
    Party,
    Guild,
    Gens,
}

impl InputType {
    pub const ALL: [InputType; 4] = [InputType::Normal, InputType::Party, InputType::Guild, InputType::Gens];

    /// El prefijo del canal (NewUIChatInputBox.cpp:480-489; `$` de MuMain).
    fn prefix(self) -> &'static str {
        match self {
            InputType::Normal => "",
            InputType::Party => "~",
            InputType::Guild => "@",
            InputType::Gens => "$",
        }
    }
}

/// `GlobalText[260]`, `[264]`, `[265]`, `[267]`, `[268]` y `[263]`.
pub const TEXT_WARP: &str = "/warp";
const TEXT_WHISPER_OFF: &str = "/whisper off";
const TEXT_WHISPER_ON: &str = "/whisper on";
const TEXT_WHISPER_IS_OFF: &str = "Whisper function is now off";
const TEXT_WHISPER_IS_ON: &str = "Whisper function is now on";
pub const TEXT_ICARUS_NEEDS_WINGS: &str = "You can enter Icarus only with wings, dinorant, fenrirr";
/// `GlobalText[482]`: el susurro no llegó.
pub const TEXT_NO_USERS: &str = "No users";
/// `GlobalText[55]`: Icarus.
const ICARUS: &str = "Icarus";

/// Un destino de la ventana de mover (`MOVEREQINFO`): índice, nombre y subnombre.
#[derive(Clone, Debug)]
pub struct WarpName {
    pub index: i32,
    pub name: String,
    pub sub_name: String,
}

/// `GetMapIndexFromMovereq`: el índice por nombre o subnombre (`stricmp`); −1 si no está.
pub fn map_index_from_movereq(list: &[WarpName], name: &str) -> i32 {
    list.iter()
        .find(|w| w.name.eq_ignore_ascii_case(name) || w.sub_name.eq_ignore_ascii_case(name))
        .map_or(-1, |w| w.index)
}

/// `IsIllegalMovementByUsingMsg` (ZzzInterface.cpp:9226-9307) sin equipo: si el texto pide
/// moverse ("/MOVE" en mayúsculas; el `/warp` del `GlobalText[260]` se busca en el texto ya en
/// mayúsculas, así que nunca se encuentra) a Icarus, no se puede volar sin alas. Devuelve el aviso.
fn illegal_movement(text: &str, list: &[WarpName]) -> Option<&'static str> {
    let upper = text.to_uppercase();
    if !upper.contains("/MOVE") && !upper.contains(TEXT_WARP) {
        return None;
    }
    let w = list.iter().find(|w| text.contains(w.name.as_str()) || upper.contains(&w.sub_name.to_uppercase()))?;
    // Atlans solo se niega montando una Uniria o Dinorant (sin equipo, nunca).
    (w.name.eq_ignore_ascii_case(ICARUS)).then_some(TEXT_ICARUS_NEEDS_WINGS)
}

/// `CheckChatText` (ZzzInterface.cpp:4434-4547): el gesto (`AT_*`) del primer grupo de palabras que
/// sale en el texto (`FindText`: en cualquier sitio, mirando mayúsculas), con las de Text_eng.bmd
/// (`GlobalText[270..=348]` y `[783]`) y las que van escritas tal cual. Las que el original escribe
/// en coreano no se pueden escribir con el inglés y no están. "the forehead" (`[2228]`) y `[2243]`
/// piden anillos de transformación: sin equipo no hacen nada.
pub fn chat_emote(text: &str) -> Option<u8> {
    use mu_protocol::game::action as at;
    const TABLE: &[(&[&str], u8)] = &[
        (&["Hello", "Hi", "Welcome", "Thanks"], at::GREETING1),
        (&["enjoy the game", "Bye", "bye"], at::GOODBYE1),
        (&["Good", "Wow", "Nice"], at::CLAP1),
        (&["Here", "Come"], at::GESTURE1),
        (&["There", "That"], at::DIRECTION1),
        (&["Not", "Never", "Do not", "do not"], at::UNKNOWN1),
        (&[";", "Sorry"], at::AWKWARD1),
        (&["T_T", "Sad", "Cry"], at::CRY1),
        (&["-.-", "-_-", "Huh", "Pooh"], at::SEE1),
        (&["^^", "^.^", "^_^", "Haha", "Hehe", "Hoho", "Hihi"], at::SMILE1),
        (&["Great", "Oh Yeah", "Oh yeah", "beat it"], at::CHEER1),
        (&["Win", "Victory"], at::WIN1),
        (&["Sleep", "Tired"], at::SLEEP1),
        (&["Cold", "hurt"], at::COLD1),
        (&["Again", "OK", "Great"], at::AGAIN1),
        (&["Respect", "Defeated"], at::RESPECT1),
        (&["Sir"], at::SALUTE1),
        (&["Rush", "Go go"], at::RUSH1),
        (&["Hustle", "hustle"], at::HUSTLE),
        (&["come on"], at::PROVOCATION),
        // `GlobalText[317]` es "Great", que ya ha cogido `AT_CHEER1`: como en el original, no sale nunca.
        (&["Great"], at::CHEERS),
        (&["Look around"], at::LOOK_AROUND),
    ];
    TABLE.iter().find(|(words, _)| words.iter().any(|w| text.contains(w))).map(|&(_, a)| a)
}

/// Lo que sale de pulsar Enter con texto.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outgoing {
    /// El gesto de `CheckChatText` (`AT_*`), antes de `SendChat` (y aunque `SendChat` no lo mande).
    Emote(u8),
    /// `SendChat`.
    Chat(String),
    /// `SendChatWhisper(destinatario, texto)`.
    Whisper(String, String),
    /// `SendRequestMoveMap(GetMoveCommandKey(), índice)` (−1 si el nombre no está, como el original).
    Warp(i32),
}

/// `SEASON3B::CNewUIChatInputBox`: lo que no es dibujo.
#[derive(Clone, Debug)]
pub struct ChatInput {
    pub open: bool,
    /// La caja del nombre tiene el foco (si no, la del texto).
    pub whisper_focus: bool,
    pub chat: TextBox,
    pub whisper: TextBox,
    pub chat_history: Vec<String>,
    pub whisper_history: Vec<String>,
    pub cur_chat: i32,
    pub cur_whisper: i32,
    pub msg_type: InputType,
    pub block_whisper: bool,
    /// Ventana de mensajes del sistema a la vista (el botón "System message pop-up" del S6).
    pub show_system: bool,
    pub show_chat_log: bool,
    /// `m_bWhisperSend`: la caja del nombre está puesta (F3).
    pub whisper_send: bool,
    /// `m_bShowMessageElseNormal` (F2): solo los susurros.
    pub whisper_only: bool,
    /// `ChatTime` y `ChatText` de `SendChat` (en ticks de 25 por segundo).
    pub chat_time: i32,
    pub last_chat: String,
    /// `ChatWhisperID`: a quién fue el último susurro.
    pub whisper_id: String,
}

/// `MAX_CHAT_SIZE_UTF16 − 1` (90 / 2 − 1 con el inglés, NewUIChatInputBox.cpp:14, :86).
pub const MAX_CHAT_CHARS: usize = 44;
/// `m_pWhsprIDInputBox->Init(g_hWnd, 50, 14, 10)`.
pub const MAX_ID_CHARS: usize = 10;

impl Default for ChatInput {
    fn default() -> Self {
        Self {
            open: false,
            whisper_focus: false,
            chat: TextBox::new(MAX_CHAT_CHARS),
            whisper: TextBox::new(MAX_ID_CHARS),
            chat_history: Vec::new(),
            whisper_history: Vec::new(),
            cur_chat: 0,
            cur_whisper: 0,
            msg_type: InputType::Normal,
            block_whisper: false,
            show_system: true,
            show_chat_log: true,
            whisper_send: true,
            whisper_only: false,
            chat_time: 0,
            last_chat: String::new(),
            whisper_id: String::new(),
        }
    }
}

impl ChatInput {
    /// `OpenningProcess`: el foco al texto, vacío.
    pub fn open(&mut self) {
        self.open = true;
        self.whisper_focus = false;
        self.chat.set("");
    }

    /// `ClosingProcess` (y `Hide`); quien llama quita el fondo del chat.
    pub fn close(&mut self) {
        self.open = false;
        self.whisper_focus = false;
    }

    /// La caja con el foco.
    pub fn focused(&mut self) -> &mut TextBox {
        if self.whisper_focus { &mut self.whisper } else { &mut self.chat }
    }

    /// Tab (`SetTabTarget`): a la otra caja si se ve, con todo seleccionado.
    pub fn tab(&mut self) {
        if self.whisper_focus {
            self.whisper_focus = false;
        } else if self.whisper_send {
            self.whisper_focus = true;
        } else {
            return;
        }
        let b = self.focused();
        b.select_all = true;
        b.caret = b.len();
    }

    /// Enter con la caja abierta (NewUIChatInputBox.cpp:464-552): lo que hay que mandar; los
    /// mensajes van a `log` (chat) y `system` (ventana del sistema). Cierra la caja.
    pub fn submit(
        &mut self,
        hero: &str,
        warps: &[WarpName],
        log: &mut ChatLog,
        system: &mut SystemLog,
        measure: &dyn Fn(&str) -> f32,
    ) -> Vec<Outgoing> {
        let mut out = Vec::new();
        let typed = self.chat.text.clone();
        let whisper_id = self.whisper.text.clone();
        // El prefijo del canal, salvo los comandos.
        let text = if typed.starts_with('/') { typed.clone() } else { format!("{}{typed}", self.msg_type.prefix()) };
        if !typed.is_empty() && !self.check_command(&text, warps, log, system, measure) {
            if self.whisper_send && !whisper_id.is_empty() {
                out.push(Outgoing::Whisper(whisper_id.clone(), text.clone()));
                self.whisper_id = whisper_id.clone();
                log.add_text(hero, &text, MsgType::Whisper, MsgType::All, measure);
                add_history(&mut self.whisper_history, &whisper_id, 5);
            } else if let Some(rest) = text.strip_prefix(TEXT_WARP) {
                // `szChatText + strlen(GlobalText[260]) + 1`: lo que va tras el espacio.
                let name: String = rest.chars().skip(1).collect();
                out.push(Outgoing::Warp(map_index_from_movereq(warps, &name)));
            } else {
                // `CheckChatText` (sin montura: siempre), antes de `SendChat`.
                if let Some(emote) = chat_emote(&text) {
                    out.push(Outgoing::Emote(emote));
                }
                if let Some(chat) = self.send_chat(&text, system) {
                    out.push(Outgoing::Chat(chat));
                }
                add_history(&mut self.chat_history, &text, 12);
            }
        }
        self.chat.set("");
        self.cur_chat = 0;
        self.cur_whisper = 0;
        self.close();
        out
    }

    /// `CheckCommand` (ZzzInterface.cpp:3853-4359): de lo que no es solo del servidor, `/filter`
    /// (que además se manda) y el aviso de Icarus. Devuelve si se ha quedado el texto.
    fn check_command(&self, text: &str, warps: &[WarpName], log: &mut ChatLog, system: &mut SystemLog, measure: &dyn Fn(&str) -> f32) -> bool {
        // `GlobalText[753]` y "/filter".
        if text.starts_with("/filter") {
            log.set_filter_text(text, measure);
        }
        match illegal_movement(text, warps) {
            Some(message) => {
                system.add(message, MsgType::System);
                true
            }
            None => false,
        }
    }

    /// `SendChat` (wsclientinline.h:328-387): no más de uno cada 0.8 s ni el mismo en 2.8 s, y
    /// `/whisper off|on` se queda aquí.
    fn send_chat(&mut self, text: &str, system: &mut SystemLog) -> Option<String> {
        if self.chat_time > 50 || (self.chat_time > 0 && self.last_chat == text) {
            return None;
        }
        self.last_chat = text.to_string();
        self.chat_time = 70;
        if text.starts_with('/') {
            if text.starts_with(TEXT_WHISPER_OFF) {
                self.block_whisper = true;
                system.add(TEXT_WHISPER_IS_OFF, MsgType::System);
                return None;
            }
            if text.starts_with(TEXT_WHISPER_ON) {
                self.block_whisper = false;
                system.add(TEXT_WHISPER_IS_ON, MsgType::System);
                return None;
            }
        }
        Some(text.to_string())
    }

    /// Un tick (`ChatTime--`, ZzzScene.cpp:2313).
    pub fn tick(&mut self) {
        if self.chat_time > 0 {
            self.chat_time -= 1;
        }
    }
}

// --- Bocadillos -----------------------------------------------------------------------------------

/// `CutText` (ZzzInterface.cpp:594-615) con `GetClosestBlankFromCenter` (MultiLanguage.cpp:159-189):
/// (lo de después del corte, lo de antes con el espacio). El bocadillo pone primero lo de antes.
pub fn cut_text(text: &str) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len() as i32;
    let half = len / 2;
    let last_first = chars[..half as usize].iter().rposition(|&c| c == ' ').map_or(-1, |p| p as i32);
    // `find_first_of(L" ", 1)` en la segunda mitad.
    let first_second = chars[half as usize..].iter().enumerate().skip(1).find(|(_, c)| **c == ' ').map_or(-1, |(p, _)| p as i32);
    let blank = if last_first < 0 && first_second < 0 {
        half
    } else if last_first < 0 {
        first_second + half
    } else if first_second < 0 {
        last_first
    } else if first_second <= half - last_first {
        first_second + half
    } else {
        last_first
    };
    let at = ((blank + 1) as usize).min(chars.len());
    (chars[at..].iter().collect(), chars[..at].iter().collect())
}

// --- Nombres de los mapas ---------------------------------------------------------------------------

/// `GetMapName` (MapManager.cpp:1745-1884) para `WorldActive`, con los textos de Text_eng.bmd.
/// Los que caen en `GlobalText[30 + mapa]` sin ser nombres de mapa salen igual que en el original.
pub fn map_name(map: u16) -> &'static str {
    match map {
        34 => "Crywolf Fortress",
        33 => "Aida",
        37 => "Kanturu",
        38 => "Kanturu3",
        39 => "Refinery Tower",
        40 => "GM summon zone",
        51 => "Elveland",
        57 => "La Cleon",
        58 => "Hatchery",
        30 => "Valley of Loren",
        31 => "Land of Trials",
        18..=23 | 53 => "Chaos Castle",
        36 => "Lost Kalima",
        24..=29 => "Kalima",
        11..=17 | 52 => "Blood Castle",
        10 => "Icarus",
        32 => "Devil Square",
        41 => "Balgass' Barrack",
        42 => "Balgass' Resting Place",
        45..=50 => "Illusion Temple",
        56 => "Swamp of Peace",
        62 => "Santa's village",
        64 => "Colosseum",
        63 => "Vulcanus",
        65..=68 => "Doppelganger",
        69..=72 => "Varka",
        79 => "Loren Market",
        80 | 81 => "Karutan",
        // `GlobalText[30 + iMap]`.
        0 => "Lorencia",
        1 => "Dungeon",
        2 => "Devias",
        3 => "Noria",
        4 => "Lost Tower",
        5 => "A place of exile",
        6 => "Arena",
        7 => "Atlans",
        8 => "Tarkan",
        9 => "Devil Square",
        35 => "Defense: %d",
        43 => "Strength Requirement: %d",
        44 => "(lacking %d)",
        54 => "Selling Price: %s",
        55 => "Attack speed: %d",
        59 => "Additional Wizardry Dmg +%d",
        60 => "Additional defense rate +%d",
        61 => "Additional defense +%d",
        _ => "",
    }
}

/// `GlobalText[484]` + el nombre (WSclient.cpp:984-991 al entrar y 1805-1812 al cambiar de mapa);
/// en los Doppelganger, nada.
pub fn welcome_text(map: u16) -> Option<String> {
    (!(65..=68).contains(&map)).then(|| format!("Welcome to {}", map_name(map)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Letra de ancho fijo: 5 por letra.
    fn fixed(s: &str) -> f32 {
        s.chars().count() as f32 * 5.0
    }

    #[test]
    fn partir_lineas_largas() {
        // 259 de ancho: con "Pepe : " (7 letras = 35) caben 224 / 5 = 44 letras en la primera.
        let text = "palabra ".repeat(8);
        let (a, b) = separate_text("Pepe", &text, &fixed);
        assert!(fixed(&a) <= CLIENT_WIDTH - 35.0);
        assert_eq!(format!("{a}{b}"), text, "no se pierde nada");
        assert!(b.starts_with(' '), "el espacio pasa a la segunda: {b:?}");
        // Sin espacios, letra a letra (sin nombre también se descuenta " : ", como el original).
        let (a, b) = separate_text("", &"x".repeat(60), &fixed);
        assert_eq!((a.len(), b.len()), (48, 12));
        // Si cabe, entera.
        assert_eq!(separate_text("Pepe", "hola", &fixed), ("hola".into(), String::new()));
    }

    #[test]
    fn mensajes_y_desplazamiento() {
        let mut log = ChatLog::default();
        assert_eq!(log.end_line(), -1);
        assert!(log.visible().is_empty());
        for i in 0..10 {
            log.add_text("Pepe", &format!("hola {i}"), MsgType::Chat, MsgType::All, &fixed);
        }
        assert_eq!(log.end_line(), 9, "sin el fondo, siempre al final");
        let v = log.visible();
        assert_eq!(v.len(), 6);
        assert_eq!(v[0].1.text, "hola 4");
        // La primera, en y = 383 − 100 + 3 + 4.
        assert_eq!(v[0].2, 290);
        assert_eq!(v[5].2, 290 + 75);
        // Con pocas, abajo del todo: la única va donde iría la sexta (con el `FONT_LEADING` doble).
        let mut corto = ChatLog::default();
        corto.add_text("", "Welcome to Lorencia", MsgType::System, MsgType::All, &fixed);
        assert_eq!(corto.visible()[0].2, 283 + 3 + 4 + 75 + 4);

        // La rueda: con el ratón encima (el primer fotograma solo entra).
        let at = MouseIn { pos: Some((10, 300)), ..Default::default() };
        assert!(log.update_mouse(&at).consumed);
        let up = MouseIn { wheel: 1.0, ..at };
        assert!(log.update_mouse(&up).wheel_used);
        assert_eq!(log.end_line(), 8);
        for _ in 0..20 {
            log.update_mouse(&up);
        }
        assert_eq!(log.end_line(), 5, "no sube más allá de la primera pantalla");
        // Con el fondo y lejos del final, lo nuevo no mueve la vista.
        log.set_frame(true);
        log.add_text("Pepe", "nuevo", MsgType::Chat, MsgType::All, &fixed);
        assert_eq!(log.end_line(), 5);
        // Cerca del final sí.
        log.scrolling(9);
        log.add_text("Pepe", "otro", MsgType::Chat, MsgType::All, &fixed);
        assert_eq!(log.end_line(), 11);
    }

    #[test]
    fn listas_por_tipo() {
        let mut log = ChatLog::default();
        log.add_text("Pepe", "psst", MsgType::Whisper, MsgType::All, &fixed);
        log.add_text("Juan", TEXT_NO_USERS, MsgType::Error, MsgType::Whisper, &fixed);
        log.add_text("Ana", "hola", MsgType::Chat, MsgType::All, &fixed);
        assert_eq!(log.lines(MsgType::All), 3);
        assert_eq!(log.lines(MsgType::Whisper), 2, "el error del susurro también va a los susurros");
        log.change_message(MsgType::Whisper);
        assert_eq!(log.end_line(), 1);
        assert_eq!(log.visible()[1].1.kind, MsgType::Error);
        // 200 como mucho.
        let mut lleno = ChatLog::default();
        for i in 0..250 {
            lleno.add_text("", &format!("{i}"), MsgType::System, MsgType::All, &fixed);
        }
        assert_eq!(lleno.lines(MsgType::All), 200);
        assert_eq!(lleno.visible().last().unwrap().1.text, "249");
    }

    #[test]
    fn tamano_y_transparencia() {
        let mut log = ChatLog::default();
        assert_eq!(log.wnd_height(), 100);
        let mut seen = Vec::new();
        for _ in 0..5 {
            log.size_auto();
            seen.push(log.showing());
        }
        assert_eq!(seen, [9, 12, 15, 3, 6]);
        let mut alphas = Vec::new();
        for _ in 0..5 {
            log.back_alpha_auto();
            alphas.push((log.back_alpha() * 10.0).round() as i32);
        }
        assert_eq!(alphas, [8, 2, 4, 6, 8]);
    }

    #[test]
    fn filtro() {
        let mut log = ChatLog::default();
        log.set_filter_text("/filter oro plata", &fixed);
        log.add_text("Pepe", "vendo oro", MsgType::Chat, MsgType::All, &fixed);
        log.add_text("Pepe", "hola", MsgType::Chat, MsgType::All, &fixed);
        log.add_text("", "sistema", MsgType::System, MsgType::All, &fixed);
        let texts: Vec<_> = log.visible().iter().map(|(_, l, _)| l.text.clone()).collect();
        assert_eq!(texts, ["Filtering has been activated", "vendo oro", "sistema"]);
        log.set_filter_text("/filter", &fixed);
        log.add_text("Pepe", "hola", MsgType::Chat, MsgType::All, &fixed);
        assert_eq!(log.visible().last().unwrap().1.text, "hola");
    }

    #[test]
    fn nombre_con_clic_derecho() {
        let mut log = ChatLog::default();
        log.add_text("Pepe", "hola", MsgType::Chat, MsgType::All, &fixed);
        let y = log.visible()[0].2;
        let m = MouseIn { pos: Some((20, y + 2)), ..Default::default() };
        log.update_mouse(&m);
        let out = log.update_mouse(&MouseIn { right_press: true, ..m });
        assert_eq!(out.whisper_to.as_deref(), Some("Pepe"));
        assert_eq!(log.pointed(), Some(0));
    }

    #[test]
    fn historial() {
        let mut h = Vec::new();
        for s in ["a", "b", "c", "a"] {
            add_history(&mut h, s, 12);
        }
        assert_eq!(h, ["b", "c", "a"]);
        let mut cur = 0;
        step_history(&mut cur, h.len(), true);
        assert_eq!(cur, 2, "arriba: el último");
        step_history(&mut cur, h.len(), false);
        assert_eq!(cur, 0);
        let mut ids = Vec::new();
        for i in 0..10 {
            add_history(&mut ids, &i.to_string(), 5);
        }
        assert_eq!(ids.len(), 6);
    }

    fn warps() -> Vec<WarpName> {
        vec![
            WarpName { index: 2, name: "Lorencia".into(), sub_name: "Lorencia".into() },
            WarpName { index: 23, name: "Icarus".into(), sub_name: "Icarus".into() },
        ]
    }

    fn enter(input: &mut ChatInput, text: &str, log: &mut ChatLog, system: &mut SystemLog) -> Vec<Outgoing> {
        input.open();
        input.chat.set(text);
        input.submit("Heroe", &warps(), log, system, &fixed)
    }

    #[test]
    fn enviar() {
        let (mut log, mut system) = (ChatLog::default(), SystemLog::default());
        let mut input = ChatInput::default();
        assert_eq!(enter(&mut input, "hola", &mut log, &mut system), [Outgoing::Chat("hola".into())]);
        assert!(!input.open);
        // Antes de 0.8 s no se manda otro; el mismo, tampoco antes de 2.8 s.
        assert!(enter(&mut input, "otra", &mut log, &mut system).is_empty());
        for _ in 0..25 {
            input.tick();
        }
        assert!(enter(&mut input, "otra", &mut log, &mut system).len() == 1);
        for _ in 0..25 {
            input.tick();
        }
        assert!(enter(&mut input, "otra", &mut log, &mut system).is_empty());
        assert_eq!(input.chat_history, ["hola", "otra"]);
        for _ in 0..70 {
            input.tick();
        }
        // Al grupo, con su prefijo; los comandos, sin él.
        input.msg_type = InputType::Party;
        assert_eq!(enter(&mut input, "vamos", &mut log, &mut system), [Outgoing::Chat("~vamos".into())]);
        for _ in 0..70 {
            input.tick();
        }
        assert_eq!(enter(&mut input, "/warp Lorencia", &mut log, &mut system), [Outgoing::Warp(2)]);
        assert_eq!(enter(&mut input, "/warp Nada", &mut log, &mut system), [Outgoing::Warp(-1)]);
        // "/move Icarus": sin alas se queda en el cliente.
        assert!(enter(&mut input, "/move Icarus", &mut log, &mut system).is_empty());
        assert_eq!(system.lines().last().unwrap().text, TEXT_ICARUS_NEEDS_WINGS);
        // "/whisper off" se queda y avisa.
        input.msg_type = InputType::Normal;
        for _ in 0..70 {
            input.tick();
        }
        assert!(enter(&mut input, "/whisper off", &mut log, &mut system).is_empty());
        assert!(input.block_whisper);
        // Vacío: nada.
        assert!(enter(&mut input, "", &mut log, &mut system).is_empty());
    }

    #[test]
    fn gestos_al_hablar() {
        use mu_protocol::game::action as at;
        assert_eq!(chat_emote("Hello!"), Some(at::GREETING1));
        assert_eq!(chat_emote("ok Bye"), Some(at::GOODBYE1));
        assert_eq!(chat_emote("hello"), None, "mira mayúsculas, como `FindText`");
        assert_eq!(chat_emote("haha ^^"), Some(at::SMILE1));
        assert_eq!(chat_emote("Great"), Some(at::CHEER1), "el primero que sale");
        assert_eq!(chat_emote("come on"), Some(at::PROVOCATION));
        assert_eq!(chat_emote("Look around"), Some(at::LOOK_AROUND));
        assert_eq!(chat_emote("Thanks; Sorry"), Some(at::GREETING1), "por orden de grupos, no de texto");
        // Va antes del chat y aunque el chat no salga.
        let (mut log, mut system) = (ChatLog::default(), SystemLog::default());
        let mut input = ChatInput::default();
        assert_eq!(enter(&mut input, "Hi", &mut log, &mut system), [Outgoing::Emote(at::GREETING1), Outgoing::Chat("Hi".into())]);
        assert_eq!(enter(&mut input, "Hi", &mut log, &mut system), [Outgoing::Emote(at::GREETING1)]);
        // Susurrando, no.
        input.whisper.set("Elfa");
        for _ in 0..70 {
            input.tick();
        }
        assert_eq!(enter(&mut input, "Hi", &mut log, &mut system), [Outgoing::Whisper("Elfa".into(), "Hi".into())]);
    }

    #[test]
    fn susurrar() {
        let (mut log, mut system) = (ChatLog::default(), SystemLog::default());
        let mut input = ChatInput::default();
        input.whisper.set("Elfa");
        assert_eq!(enter(&mut input, "psst", &mut log, &mut system), [Outgoing::Whisper("Elfa".into(), "psst".into())]);
        assert_eq!(log.visible()[0].1, &Line { id: "Heroe".into(), text: "psst".into(), kind: MsgType::Whisper });
        assert_eq!(input.whisper_id, "Elfa");
        assert_eq!(input.whisper_history, ["Elfa"]);
        // Con F3 (sin la caja del nombre) va al chat normal.
        input.whisper_send = false;
        assert_eq!(enter(&mut input, "hola", &mut log, &mut system), [Outgoing::Chat("hola".into())]);
        // Tab no va a la caja escondida.
        input.tab();
        assert!(!input.whisper_focus);
        input.whisper_send = true;
        input.tab();
        assert!(input.whisper_focus && input.whisper.select_all);
    }

    #[test]
    fn caja_de_texto() {
        let mut b = TextBox::new(5);
        for c in "abcdefg".chars() {
            b.insert(c);
        }
        assert_eq!(b.text, "abcde");
        b.left();
        b.backspace();
        assert_eq!((b.text.as_str(), b.caret), ("abce", 3));
        b.select_all = true;
        b.insert('x');
        assert_eq!(b.text, "x");
        // El texto se mueve para que se vea el cursor.
        let mut b = TextBox::new(44);
        b.set(&"m".repeat(40));
        b.follow_caret(100.0, &fixed);
        assert_eq!(b.scroll, 200.0 - 98.0);
        b.move_to(0);
        b.follow_caret(100.0, &fixed);
        assert_eq!(b.scroll, 0.0);
    }

    #[test]
    fn partir_bocadillo() {
        // El espacio más cercano al centro: el de antes de "tal".
        let (despues, antes) = cut_text("hola que tal estas hoy");
        assert_eq!(antes, "hola que tal ");
        assert_eq!(despues, "estas hoy");
        let (despues, antes) = cut_text("abcdefghijklmnopqrstuv");
        assert_eq!((antes.as_str(), despues.as_str()), ("abcdefghijkl", "mnopqrstuv"));
    }

    #[test]
    fn nombres_de_mapa() {
        assert_eq!(welcome_text(0).as_deref(), Some("Welcome to Lorencia"));
        assert_eq!(welcome_text(51).as_deref(), Some("Welcome to Elveland"));
        assert_eq!(welcome_text(26).as_deref(), Some("Welcome to Kalima"));
        assert_eq!(welcome_text(66), None);
        assert_eq!(map_index_from_movereq(&warps(), "lorencia"), 2);
    }
}
