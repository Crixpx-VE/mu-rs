//! Center-screen notices (`CreateNotice`, `MoveNotices` and `RenderNotices`,
//! ZzzInterface.cpp:480-590): server notices of type 0 (golden, blinking) and 2 (guild, aqua
//! green), `ReceiveNotice` (WSclient.cpp:1577-1611).
//!
//! - Six lines (`MAX_NOTICE`): each new notice takes the next line and, when full, all scroll up one.
//! - Every 300 ticks without new notices (`NoticeTime`, 12 s) an empty line is added: old ones
//!   scroll up until gone.
//! - One 256 pixels wide or more is split in two at the space closest to the center
//!   (`CutText` → `GetClosestBlankFromCenter`, MultiLanguage.cpp:159-190).
//! - Drawn centered at x = 320, from y = 300 in steps of 13 (640×480 screen), with
//!   `g_hFontBold` and a black background at 128.

/// `MAX_NOTICE`.
pub const MAX_NOTICE: usize = 6;
/// `NoticeTime = 300` (ticks).
const NOTICE_TIME: i32 = 300;
/// `Size.cx < 256` in font pixels at 1024×768 (like `FONT` in chat.rs), in 640×480 screen units.
pub const MAX_WIDTH: f32 = 256.0 * 480.0 / 768.0;
/// `GlobalText[483]` (Main 5.2 text_eng.bmd).
pub const TEXT_GUILD_NOTICE: &str = "[Notice for guild members] %s";

/// Notice color (`NOTICE::Color`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NoticeColor {
    /// 0: blinking gold (255, 200, 80).
    #[default]
    Golden,
    /// 1: aqua green (100, 255, 200).
    Guild,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Notice {
    pub text: String,
    pub color: NoticeColor,
}

/// `Notice[MAX_NOTICE]`, `NoticeCount`, `NoticeTime` and `NoticeInverse`.
#[derive(Clone, Debug)]
pub struct Notices {
    lines: [Notice; MAX_NOTICE],
    count: usize,
    time: i32,
    inverse: u32,
}

impl Default for Notices {
    fn default() -> Self {
        Self { lines: Default::default(), count: 0, time: NOTICE_TIME, inverse: 0 }
    }
}

impl Notices {
    /// `ClearNotice` (on entering the game, WSclient.cpp:662): clears the text (not `NoticeCount`).
    pub fn clear(&mut self) {
        self.lines = Default::default();
    }

    /// `ScrollNotice`.
    fn scroll(&mut self) {
        if self.count > MAX_NOTICE - 1 {
            self.count = MAX_NOTICE - 1;
            for i in 1..MAX_NOTICE {
                self.lines[i - 1] = self.lines[i].clone();
            }
        }
    }

    /// `CreateNotice(Text, Color)`; `width` measures with the font (640-screen units).
    pub fn create(&mut self, text: &str, color: NoticeColor, width: &dyn Fn(&str) -> f32) {
        self.scroll();
        self.lines[self.count].color = color;
        if width(text) < MAX_WIDTH {
            self.lines[self.count].text = text.to_string();
            self.count += 1;
        } else {
            let (first, second) = cut_text(text);
            self.lines[self.count].text = first;
            self.count += 1;
            self.scroll();
            self.lines[self.count].color = color;
            self.lines[self.count].text = second;
            self.count += 1;
        }
        self.time = NOTICE_TIME;
    }

    /// One tick (`MoveNotices` and the per-draw `NoticeInverse++`).
    pub fn tick(&mut self, width: &dyn Fn(&str) -> f32) {
        self.inverse = self.inverse.wrapping_add(1);
        let t = self.time;
        self.time -= 1;
        if t <= 0 {
            self.time = NOTICE_TIME;
            self.create("", NoticeColor::Golden, width);
        }
    }

    /// The six lines, top to bottom.
    pub fn lines(&self) -> &[Notice; MAX_NOTICE] {
        &self.lines
    }

    /// Text color (RGBA) of a notice: gold at half alpha half of the time
    /// (`NoticeInverse % 10 < 5`).
    pub fn text_color(&self, color: NoticeColor) -> [u8; 4] {
        match color {
            NoticeColor::Golden if self.inverse % 10 < 5 => [255, 200, 80, 128],
            NoticeColor::Golden => [255, 200, 80, 255],
            NoticeColor::Guild => [100, 255, 200, 255],
        }
    }
}

/// `CutText` on UTF-8 text: (first half up to and including the space, the rest).
fn cut_text(text: &str) -> (String, String) {
    let chars: Vec<char> = text.chars().collect();
    let blank = closest_blank_from_center(&chars);
    let first: String = chars[..(blank + 1).min(chars.len())].iter().collect();
    let second: String = chars[(blank + 1).min(chars.len())..].iter().collect();
    (first, second)
}

/// `CMultiLanguage::GetClosestBlankFromCenter` (MultiLanguage.cpp:159-190).
fn closest_blank_from_center(chars: &[char]) -> usize {
    let len = chars.len();
    let half = len / 2;
    // `wstrText2.find_last_of(L" ")` in [0, half) and `wstrText1.find_first_of(L" ", 1)` in [half, len).
    let last_first = chars[..half].iter().rposition(|&c| c == ' ');
    let first_second = chars[half..].iter().skip(1).position(|&c| c == ' ').map(|p| p + 1);
    match (last_first, first_second) {
        (None, None) => half,
        (None, Some(s)) => s + half,
        (Some(f), None) => f,
        (Some(f), Some(s)) if s as isize <= half as isize - f as isize => s + half,
        (Some(f), Some(_)) => f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn width(s: &str) -> f32 {
        s.chars().count() as f32 * 4.0
    }

    #[test]
    fn suben_y_se_van() {
        let mut n = Notices::default();
        n.create("uno", NoticeColor::Golden, &width);
        n.create("dos", NoticeColor::Guild, &width);
        assert_eq!(n.lines()[0].text, "uno");
        assert_eq!(n.lines()[1], Notice { text: "dos".into(), color: NoticeColor::Guild });
        for i in 0..10 {
            n.create(&format!("{i}"), NoticeColor::Golden, &width);
        }
        // Full: the last line is always the newest.
        let texts: Vec<_> = n.lines().iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["4", "5", "6", "7", "8", "9"]);
        // 300 ticks without notices: an empty one comes in and all scroll up.
        for _ in 0..300 {
            n.tick(&width);
        }
        assert_eq!(n.lines()[5].text, "9");
        n.tick(&width);
        assert_eq!(n.lines()[4].text, "9");
        assert_eq!(n.lines()[5].text, "");
    }

    #[test]
    fn largo_se_parte_por_el_espacio_del_centro() {
        assert_eq!(cut_text("hola que tal amigo"), ("hola que ".into(), "tal amigo".into()));
        assert_eq!(cut_text("abcdef"), ("abcd".into(), "ef".into()), "sin espacios: por la mitad (+1 como el original)");
        let mut n = Notices::default();
        let long = "palabra ".repeat(10);
        n.create(long.trim(), NoticeColor::Golden, &width);
        assert!(!n.lines()[0].text.is_empty() && !n.lines()[1].text.is_empty());
        assert_eq!(format!("{}{}", n.lines()[0].text, n.lines()[1].text), long.trim());
    }

    #[test]
    fn el_dorado_parpadea() {
        let mut n = Notices::default();
        let a: Vec<u8> = (0..10)
            .map(|_| {
                n.tick(&width);
                n.text_color(NoticeColor::Golden)[3]
            })
            .collect();
        assert_eq!(a, [128, 128, 128, 128, 255, 255, 255, 255, 255, 128]);
        assert_eq!(n.text_color(NoticeColor::Guild), [100, 255, 200, 255]);
    }
}
