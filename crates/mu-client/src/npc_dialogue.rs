//! The NPC dialogue window: `SEASON3B::CNewUINPCDialogue` (NewUINPCDialogue.cpp), the talk window
//! of the S6 quest system's NPCs (the Gens stewards 543/544, the Elf Soldier 257, the Karutan
//! mercenary NPCs 566-568). The texts are `CQuestMng`'s scripts ([`mu_data::quest`]).
//!
//! - **Opening** (`ReceiveNPCDlgUIStart`, C3 F9 01, WSclient.cpp:9881-9890): unless it is already
//!   open, `SetNPC(number)` (the name from the monster table), `SetContributePoint` and
//!   `Show(INTERFACE_NPC_DIALOGUE)`, which hides the inventory and the character window
//!   (`HideAllGroupA`, NewUISystem.cpp:891-895) and runs `ProcessOpening`: dialogue state 0 and
//!   `SOUND_INTERFACE01` (NewUINPCDialogue.cpp:373-378). npc.rs keeps the dialog open/closed state
//!   and sends `SendExitInventory` when it closes (`ProcessClosing`, 380-388, which also plays
//!   `SOUND_CLICK01`, played here).
//! - **Closing**: the bottom close button or the top X (`ProcessBtns`, 98-109), Esc (`UpdateKeyEvent`,
//!   207-219, npc.rs), walking, a map change or another NPC (npc.rs).
//! - **The pages** (`SetCurNPCWords`, `SetCurSelTexts`, `CalculateSelTextMaxPage`, 401-493): the
//!   NPC's words split to 160 pixels (`DivideStringByPixel`, `;` starts a paragraph), 7 lines a page;
//!   the answers ("1. ...") below the line, 11 lines a page, shown once the last page of words is
//!   reached. The arrows turn the pages.
//! - **An answer** (`UpdateSelTextMouseEvent` and `ProcessSelTextResult`, 166-205 and 547-601): a
//!   result up to 900 goes to that dialogue state; 901 asks for the NPC's quests (F6 30, answered
//!   with the quest list, `ProcessQuestListReceive`), 902 asks for the buff (F6 31) and closes, 903
//!   and 904 join a gens (F8 01), 905 leaves it (F8 03), 906 and 907 ask for the gens reward
//!   (F8 09); the server's answers pick the state that tells the result
//!   (`ProcessGensJoiningReceive`, `ProcessGensSecessionReceive`, `ProcessGensRewardReceive`,
//!   626-723). While a request waits, the answers do not react (`m_bCanClick`).
//! - **The quest list**: picking a quest sends `SendQuestSelection` (F6 0A); the server answers
//!   with the quest's step (`ReceiveQuestQSSelSentence` → `SetCurQuestProgress`), shown by the
//!   quest progress window (`CNewUIQuestProgress`, NewUIQuestProgress.cpp) in the same place: the
//!   NPC's words, the hero's words and the answers (accept, refuse), sent with
//!   `SendQuestSelAnswer` (F6 0B). Not yet: its requests and rewards list and complete button
//!   (`REQUEST_REWARD_MODE`, from C1 F6 0C), the window of the steps without NPC
//!   (`CNewUIQuestProgressByEtc`) and the quest list window that tracks the running quests.
//! - **The gens contribution** (`RenderContributePoint`, 319-331): the steward of the hero's own
//!   gens shows the contribution points the server sent.
//!
//! The server is the only authority: joining, leaving or the reward only change when it answers.

use bevy::prelude::*;
use mu_data::quest::QuestScripts;
use mu_protocol::game::{ClientMessage, ServerMessage};

use crate::Args;
use crate::char_info::{absolute, font_size, label};
use crate::npc::{NpcDialog, NpcDialogSystems, WindowKind};
use crate::scene::GameScene;
use crate::sound::SoundQueue;

pub struct NpcDialoguePlugin;

impl Plugin for NpcDialoguePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NpcDialogueWindow>()
            .init_resource::<GensInfluence>()
            .add_systems(Startup, load_scripts)
            .add_systems(OnEnter(GameScene::Game), spawn_root)
            .add_systems(PreUpdate, follow_dialog.after(NpcDialogSystems).after(crate::net::NetSystems))
            .add_systems(Update, (update_mouse, draw).chain().run_if(in_state(GameScene::Game)))
            .add_systems(OnExit(GameScene::Game), crate::scene::despawn::<DialogueScreen>)
            .add_systems(OnExit(GameScene::Game), |mut w: ResMut<NpcDialogueWindow>, mut g: ResMut<GensInfluence>| {
                *w = NpcDialogueWindow::default();
                g.0 = 0;
            });
    }
}

// --- Measurements (640×480 screen) -------------------------------------------------------------

/// `Create(m_pNewUIMng, 640-190, 0)` (NewUISystem.cpp:430-431); `ND_WIDTH`, `ND_HEIGHT`
/// (NewUINPCDialogue.h:41-42).
const POS: Vec2 = Vec2::new(640.0 - 190.0, 0.0);
const W: f32 = 190.0;
const H: f32 = 429.0;
/// `ND_NPC_MAX_LINE_PER_PAGE`, `ND_SEL_TEXT_MAX_LINE_PER_PAGE`, `ND_TEXT_GAP` (NewUINPCDialogue.cpp:12-14).
const NPC_LINES_PER_PAGE: usize = 7;
const SEL_LINES_PER_PAGE: usize = 11;
const GAP: f32 = 15.0;
/// `ND_NPC_LINE_MAX`, `ND_WORDS_ROW_MAX`, `ND_QUEST_INDEX_MAX_COUNT`, `ND_SEL_TEXT_LINE_MAX`,
/// `ND_SEL_TEXT_PAGE_LIMIT` (NewUINPCDialogue.h:9-13).
const NPC_LINE_MAX: usize = 35;
const WORDS_ROW_MAX: usize = 64;
const QUEST_INDEX_MAX: usize = 20;
const SEL_TEXT_LINE_MAX: usize = (QUEST_INDEX_MAX + 1) * 2;
const SEL_TEXT_PAGE_LIMIT: usize = 4;
/// The width the texts are split to (`DivideStringByPixel(..., 160)`).
const TEXT_WIDTH: f32 = 160.0;
/// The arrows and the close button (`Create`, NewUINPCDialogue.cpp:39-51), inside the window.
const BTN_PROGRESS_L: (f32, f32, f32, f32) = (131.0, 165.0, 17.0, 18.0);
const BTN_PROGRESS_R: (f32, f32, f32, f32) = (153.0, 165.0, 17.0, 18.0);
const BTN_SEL_L: (f32, f32, f32, f32) = (131.0, 372.0, 17.0, 18.0);
const BTN_SEL_R: (f32, f32, f32, f32) = (153.0, 372.0, 17.0, 18.0);
const BTN_CLOSE: (f32, f32, f32, f32) = (13.0, 392.0, 36.0, 29.0);
/// The top X (`CheckMouseIn(m_Pos.x+169, m_Pos.y+7, 13, 12)`, 105).
const TOP_X: (f32, f32, f32, f32) = (169.0, 7.0, 13.0, 12.0);

/// `GlobalText[1002]` and `[2986]` (Main 5.2's Data/Local/Eng/text_eng.bmd).
const CLOSE_TIP: &str = "Close";
fn contribution_text(points: u32) -> String {
    format!("Gain Contribution: {points}")
}

/// Text colors (`SetTextColor`, `RenderText`).
const NAME_COLOR: [u8; 3] = [150, 255, 240];
const TEXT_COLOR: [u8; 3] = [255, 230, 210];
/// `glColor4f(0.5f, 0.7f, 0.3f, 0.5f)` of the hovered answer (`RenderSelTextBlock`, 284).
const BLOCK_RGBA: [u8; 4] = [127, 178, 76, 127];

/// Gens NPC numbers (`RenderContributePoint`, `SetContributePoint`).
const DUPRIAN_STEWARD: u16 = 543;
const VANERT_STEWARD: u16 = 544;

// --- Text splitting ----------------------------------------------------------------------------

/// `CutStr` (UIControls.cpp:39-96) on one paragraph: lines that fit `width` (in the units of
/// `measure`) or 63 characters (`iOutStrLength - 1`), cut after the last space, at most
/// `max_lines`.
fn cut_str(text: &str, width: f32, max_lines: usize, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut w: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < w.len() && out.len() < max_lines {
        let prefix: String = w[..i].iter().collect();
        if measure(&prefix) >= width || i >= WORDS_ROW_MAX - 1 {
            // `find_last_of(L" ", iCharIndex)`: the last space at or before `i`.
            i = match w[..=i.min(w.len() - 1)].iter().rposition(|&c| c == ' ') {
                Some(p) => p,
                None => i.saturating_sub(1),
            };
            out.push(w[..=i].iter().collect());
            w.drain(..=i);
            i = 0;
        } else {
            i += 1;
        }
    }
    if out.len() < max_lines && i > 0 {
        out.push(w[..i].iter().collect());
    }
    out
}

/// `DivideStringByPixel(dst, rows, 64, src, 160, space_insert)` (UsefulDef.h:43-49 with
/// `LJH_MOD_TO_USE_DIVIDESTRINGBYPIXEL_FUNC`'s `';'` new line, UsefulDef.cpp:34-62): each
/// paragraph (`strtok`, empty ones skipped) through [`cut_str`], with a leading space if
/// `space_insert`. The original gives each paragraph `rows` lines of its own and writes past the
/// buffer if they add up to more; here the total stops at `rows`.
fn divide_by_pixel(src: &str, rows: usize, space_insert: bool, width: f32, measure: &dyn Fn(&str) -> f32) -> Vec<String> {
    let mut out = Vec::new();
    for token in src.split(';').filter(|t| !t.is_empty()) {
        let token = if space_insert { format!(" {token}") } else { token.to_string() };
        out.extend(cut_str(&token, width, rows, measure));
    }
    out.truncate(rows);
    out
}

// --- State -------------------------------------------------------------------------------------

/// `LOWER_VIEW_MODE`: the answers show once the last page of words is reached.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum LowerView {
    #[default]
    NonSelTexts,
    SelTexts,
}

/// The window's buttons.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Btn {
    ProgressL,
    ProgressR,
    SelL,
    SelR,
    Close,
}

impl Btn {
    fn rect(self) -> (f32, f32, f32, f32) {
        match self {
            Btn::ProgressL => BTN_PROGRESS_L,
            Btn::ProgressR => BTN_PROGRESS_R,
            Btn::SelL => BTN_SEL_L,
            Btn::SelR => BTN_SEL_R,
            Btn::Close => BTN_CLOSE,
        }
    }
}

/// What an answer asks for besides changing the shown state.
#[derive(Clone, Debug, PartialEq)]
enum Action {
    Send(ClientMessage),
    /// The request and then `Hide(INTERFACE_NPC_DIALOGUE)`.
    SendAndClose(ClientMessage),
}

/// `CNewUINPCDialogue`'s state.
#[derive(Default, Debug)]
pub struct Dialogue {
    /// `g_QuestMng.GetNPCIndex()` and `GetNPCName()`.
    npc: u16,
    npc_name: String,
    /// `m_dwContributePoint`.
    contribution: u32,
    /// `m_dwCurDlgIndex` and `m_bQuestListMode`.
    cur_dlg: u32,
    quest_list_mode: bool,
    /// `m_aszNPCWords`, `m_nSelNPCPage`, `m_nMaxNPCPage`.
    npc_words: Vec<String>,
    sel_npc_page: usize,
    max_npc_page: usize,
    /// `m_aszSelTexts` (the lines), `m_nSelTextCount`, `m_anSelTextLine` (lines of each answer).
    sel_texts: Vec<String>,
    sel_text_count: usize,
    sel_text_line: Vec<usize>,
    /// `m_nSelSelText`: the hovered answer, 1-based (0 = none).
    sel_sel_text: usize,
    sel_sel_text_page: usize,
    max_sel_text_page: usize,
    lines_per_page: [usize; SEL_TEXT_PAGE_LIMIT],
    count_per_page: [usize; SEL_TEXT_PAGE_LIMIT],
    lower: LowerView,
    /// `m_adwQuestIndex`.
    quest_index: Vec<u32>,
    /// `m_bCanClick`.
    can_click: bool,
    /// The `Lock` of `m_btnProgressL/R` and `m_btnSelTextL/R`.
    lock_progress_l: bool,
    lock_progress_r: bool,
    lock_sel_l: bool,
    lock_sel_r: bool,
}

/// How the texts are measured and split: the font's widths (640 screen) and the 160 pixels of the
/// original minus its `- 5` screen pixels (`iTargetPixelWidth*g_fScreenRate_x-5`).
pub struct Layout<'a> {
    pub measure: &'a dyn Fn(&str) -> f32,
    pub width: f32,
}

impl Dialogue {
    fn locked(&self, b: Btn) -> bool {
        match b {
            Btn::ProgressL => self.lock_progress_l,
            Btn::ProgressR => self.lock_progress_r,
            Btn::SelL => self.lock_sel_l,
            Btn::SelR => self.lock_sel_r,
            Btn::Close => false,
        }
    }

    /// `SetContents` (390-399).
    fn set_contents(&mut self, s: &QuestScripts, l: &Layout, index: u32) {
        self.cur_dlg = index;
        self.set_npc_words(s, l, 0);
        self.set_sel_texts(s, l);
        self.can_click = true;
        self.lock_progress_l = true;
        self.lock_sel_l = true;
        self.sel_sel_text = 0;
    }

    /// `SetCurNPCWords` (401-431).
    fn set_npc_words(&mut self, s: &QuestScripts, l: &Layout, quest_count: usize) {
        self.npc_words.clear();
        let src = if self.quest_list_mode {
            s.word(if quest_count > 0 { 1501 } else { 1502 })
        } else {
            s.npc_words(self.npc, self.cur_dlg)
        };
        self.npc_words = src.map(|t| divide_by_pixel(t, NPC_LINE_MAX, true, l.width, l.measure)).unwrap_or_default();
        let lines = self.npc_words.len();
        if lines < 1 {
            return;
        }
        self.max_npc_page = (lines - 1) / NPC_LINES_PER_PAGE;
        if self.max_npc_page >= 1 {
            self.lower = LowerView::NonSelTexts;
            self.lock_progress_r = false;
        } else {
            self.lower = LowerView::SelTexts;
            self.lock_progress_r = true;
        }
        self.sel_npc_page = 0;
    }

    /// One answer's lines: `DivideStringByPixel(..., 2, ND_WORDS_ROW_MAX, text, 160, false)`.
    fn push_sel_text(&mut self, text: &str, l: &Layout) -> usize {
        let lines = divide_by_pixel(text, 2, false, l.width, l.measure);
        let n = lines.len();
        self.sel_texts.extend(lines);
        self.sel_text_line.push(n);
        n
    }

    /// `SetCurSelTexts` (433-463).
    fn set_sel_texts(&mut self, s: &QuestScripts, l: &Layout) {
        self.sel_texts.clear();
        self.sel_text_line.clear();
        let mut sum = 0;
        let mut i = 0;
        while i < mu_data::quest::MAX_ND_ANSWER {
            let Some(answer) = s.answer(self.npc, self.cur_dlg, i) else { break };
            sum += self.push_sel_text(&format!("{}. {answer}", i + 1), l);
            if sum >= SEL_TEXT_LINE_MAX {
                break;
            }
            i += 1;
        }
        self.sel_text_count = i;
        self.calculate_sel_text_max_page(i);
    }

    /// `CalculateSelTextMaxPage` (465-493).
    fn calculate_sel_text_max_page(&mut self, count: usize) {
        self.sel_sel_text_page = 0;
        self.max_sel_text_page = 0;
        self.lines_per_page = [0; SEL_TEXT_PAGE_LIMIT];
        self.count_per_page = [0; SEL_TEXT_PAGE_LIMIT];
        for i in 0..count {
            let lines = self.sel_text_line.get(i).copied().unwrap_or(0);
            let page = self.max_sel_text_page;
            self.count_per_page[page] += 1;
            self.lines_per_page[page] += lines;
            if self.lines_per_page[page] > SEL_LINES_PER_PAGE {
                self.count_per_page[page] -= 1;
                self.lines_per_page[page] -= lines;
                // The original stops at `ND_SEL_TEXT_PAGE_LIMIT` with the index one past its arrays.
                if self.max_sel_text_page + 1 == SEL_TEXT_PAGE_LIMIT {
                    break;
                }
                self.max_sel_text_page += 1;
                self.count_per_page[self.max_sel_text_page] += 1;
                self.lines_per_page[self.max_sel_text_page] += lines;
            }
        }
        self.lock_sel_r = self.max_sel_text_page < 1;
    }

    /// `SetQuestListText` (495-539).
    fn set_quest_list_text(&mut self, s: &QuestScripts, l: &Layout, quests: &[u32]) {
        self.quest_index = quests.iter().copied().take(QUEST_INDEX_MAX).collect();
        let n = self.quest_index.len();
        self.sel_text_count = n + 1;
        self.sel_texts.clear();
        self.sel_text_line.clear();
        let mut rows = 0;
        let mut i = 0;
        while i < self.sel_text_count {
            let text = if i == self.sel_text_count - 1 {
                s.word(1007).map(|w| format!("{}. {w}", i + 1))
            } else {
                s.subject(self.quest_index[i]).map(|w| format!("{}. [Q]{w}", i + 1))
            };
            let Some(text) = text else { break };
            rows += self.push_sel_text(&text, l);
            if rows > SEL_TEXT_LINE_MAX {
                break;
            }
            i += 1;
        }
        self.calculate_sel_text_max_page(i);
    }

    /// `ProcessQuestListReceive` (603-612).
    fn quest_list_received(&mut self, s: &QuestScripts, l: &Layout, quests: &[u32]) {
        self.quest_list_mode = true;
        self.set_npc_words(s, l, quests.len());
        self.set_quest_list_text(s, l, quests);
        self.can_click = true;
        self.lock_progress_l = true;
        self.lock_sel_l = true;
        self.sel_sel_text = 0;
    }

    /// `ProcessSelTextResult` (547-601) for the answer `self.sel_sel_text`.
    fn sel_text_result(&mut self, s: &QuestScripts, l: &Layout) -> Option<Action> {
        if self.quest_list_mode {
            if self.sel_sel_text == self.sel_text_count {
                self.quest_list_mode = false;
                self.set_contents(s, l, 0);
                None
            } else {
                let quest = *self.quest_index.get(self.sel_sel_text.checked_sub(1)?)?;
                Some(Action::Send(ClientMessage::QuestSelect { quest, selected: self.sel_sel_text as u8 }))
            }
        } else {
            let result = s.answer_result(self.npc, self.cur_dlg, self.sel_sel_text.wrapping_sub(1));
            if result <= 900 {
                // A missing answer (-1) becomes the DWORD 0xFFFFFFFF: an empty state.
                self.set_contents(s, l, result as u32);
                return None;
            }
            Some(match result {
                901 => Action::Send(ClientMessage::AvailableQuestsRequest),
                902 => Action::SendAndClose(ClientMessage::NpcBuffRequest),
                903 => Action::Send(ClientMessage::GensJoin { gens: 1 }),
                904 => Action::Send(ClientMessage::GensJoin { gens: 2 }),
                905 => Action::Send(ClientMessage::GensLeave),
                906 => Action::Send(ClientMessage::GensReward { gens: 1 }),
                907 => Action::Send(ClientMessage::GensReward { gens: 2 }),
                _ => {
                    self.set_contents(s, l, 999);
                    return None;
                }
            })
        }
    }

    /// `ProcessGensJoiningReceive` (626-656): the state that tells the result; the hero's gens on
    /// success. Unknown results change nothing.
    fn gens_join_received(&mut self, s: &QuestScripts, l: &Layout, result: u8, gens: u8, influence: &mut u8) {
        let state = match result {
            0 => {
                *influence = gens;
                5
            }
            1 => 9,
            2 => 11,
            3 => 8,
            4 => 10,
            5 => 12,
            6 => 18,
            7 => 19,
            _ => return,
        };
        self.set_contents(s, l, state);
    }

    /// `ProcessGensSecessionReceive` (677-695).
    fn gens_leave_received(&mut self, s: &QuestScripts, l: &Layout, result: u8, influence: &mut u8) {
        let state = match result {
            0 => {
                *influence = 0;
                16
            }
            1 => 15,
            2 => 14,
            3 => 17,
            _ => return,
        };
        self.set_contents(s, l, state);
    }

    /// `ProcessGensRewardReceive` (697-723).
    fn gens_reward_received(&mut self, s: &QuestScripts, l: &Layout, result: u8) {
        let state = match result {
            0 => 20,
            1 => 21,
            2 => 22,
            3 => 23,
            4 => 24,
            5 => 17,
            6 => 25,
            _ => return,
        };
        self.set_contents(s, l, state);
    }

    /// `m_btnProgressR` (110-127).
    fn progress_right(&mut self) {
        self.sel_npc_page = (self.sel_npc_page + 1).min(self.max_npc_page);
        if self.sel_npc_page == self.max_npc_page && self.lower == LowerView::NonSelTexts {
            self.lower = LowerView::SelTexts;
        }
        if self.sel_npc_page == self.max_npc_page && self.lower != LowerView::NonSelTexts {
            self.lock_progress_r = true;
        }
        if self.max_npc_page != 0 {
            self.lock_progress_l = false;
        }
    }

    /// `m_btnProgressL` (128-138).
    fn progress_left(&mut self) {
        self.sel_npc_page = self.sel_npc_page.saturating_sub(1);
        if self.sel_npc_page == 0 {
            self.lock_progress_l = true;
        }
        self.lock_progress_r = false;
    }

    /// `m_btnSelTextR` (139-150).
    fn sel_right(&mut self) {
        self.sel_sel_text_page = (self.sel_sel_text_page + 1).min(self.max_sel_text_page);
        if self.sel_sel_text_page == self.max_sel_text_page {
            self.lock_sel_r = true;
        }
        if self.max_sel_text_page != 0 {
            self.lock_sel_l = false;
        }
    }

    /// `m_btnSelTextL` (151-161).
    fn sel_left(&mut self) {
        self.sel_sel_text_page = self.sel_sel_text_page.saturating_sub(1);
        if self.sel_sel_text_page == 0 {
            self.lock_sel_l = true;
        }
        self.lock_sel_r = false;
    }

    /// The answers of the shown page: their index range.
    fn page_answers(&self) -> std::ops::Range<usize> {
        let page = self.sel_sel_text_page.min(SEL_TEXT_PAGE_LIMIT - 1);
        let start: usize = self.count_per_page[..page].iter().sum();
        start..start + self.count_per_page[page]
    }

    /// The answer under the mouse (`UpdateSelTextMouseEvent`, 166-205), 1-based; `None` outside
    /// the answers' columns (`m_nSelSelText` 0).
    fn answer_at(&self, m: Vec2) -> usize {
        if m.x < 11.0 || m.x > 179.0 {
            return 0;
        }
        let mut bottom = 203.0;
        for i in self.page_answers() {
            let top = bottom;
            bottom += self.sel_text_line.get(i).copied().unwrap_or(0) as f32 * GAP;
            if top <= m.y && m.y < bottom {
                return i + 1;
            }
        }
        0
    }
}

// --- The quest progress window -----------------------------------------------------------------

/// `QP_NPC_MAX_LINE_PER_PAGE`, `QP_TEXT_GAP` (NewUIQuestProgress.cpp:13-14), `QP_PLAYER_LINE_MAX`
/// (NewUIQuestProgress.h).
const QP_PLAYER_LINE_MAX: usize = 9;
/// `m_btnProgressL/R` (NewUIQuestProgress.cpp:39-43).
const QP_BTN_L: (f32, f32, f32, f32) = (131.0, 168.0, 17.0, 18.0);
const QP_BTN_R: (f32, f32, f32, f32) = (153.0, 168.0, 17.0, 18.0);
/// `RenderText` (263-301): "Quest" is written in the code.
const QP_TITLE: &str = "Quest";
const QP_TITLE_COLOR: [u8; 3] = [230, 230, 230];
const QP_SUBJECT_COLOR: [u8; 3] = [36, 242, 252];
const QP_NPC_COLOR: [u8; 3] = [255, 255, 10];
const QP_PLAYER_COLOR: [u8; 3] = [255, 185, 10];
/// `GlobalText[2814]` (English text from MuMain Localization/Game.en.resx:7234).
const QUEST_COMPLETED: &str = "You've successfully completed the quest.";

/// `LOWER_VIEW_MODE` of `CNewUIQuestProgress`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
enum ProgressView {
    #[default]
    NonPlayerWords,
    PlayerWords,
    RequestReward,
}

/// `CNewUIQuestProgress`'s state (NewUIQuestProgress.cpp): what the NPC says about a quest step,
/// the hero's words and the answers (accept, refuse...).
#[derive(Default, Debug)]
pub struct Progress {
    /// `m_dwCurQuestIndex`.
    quest: u32,
    /// `g_QuestMng.GetNPCName()`: the dialogue's NPC.
    npc_name: String,
    /// `GetSubject(m_dwCurQuestIndex)`.
    subject: String,
    npc_words: Vec<String>,
    sel_npc_page: usize,
    max_npc_page: usize,
    /// `m_aszPlayerWords`: 2 lines of the hero's words, then the answers.
    player_words: Vec<String>,
    /// `m_anAnswerLine`.
    answer_line: [usize; mu_data::quest::MAX_ANSWER],
    /// `m_nSelAnswer`, 1-based.
    sel_answer: usize,
    lower: ProgressView,
    can_click: bool,
    lock_l: bool,
    lock_r: bool,
}

impl Progress {
    /// `SetContents` (355-385).
    fn set_contents(&mut self, s: &QuestScripts, l: &Layout, quest: u32) {
        if quest == 0 {
            return;
        }
        self.quest = quest;
        self.subject = s.subject(quest).unwrap_or_default().to_string();
        self.set_npc_words(s, l);
        self.can_click = true;
        self.lock_l = true;
        if s.step_answer(quest, 0).is_some() {
            self.set_player_words(s, l);
            self.lower = ProgressView::NonPlayerWords;
            self.sel_answer = 0;
            self.lock_r = false;
        } else {
            // TODO: `SetCurRequestReward` (438-480): the requests and rewards list box and the
            // complete button (`SendRequestQuestComplete`, F6 0D), from the conditions and rewards
            // of `ReceiveQuestQSRequestReward` (C1 F6 0C), not decoded yet.
            warn!("progreso de misión {quest:#010x}: falta la lista de requisitos y premios (F6 0C)");
            self.lower = ProgressView::RequestReward;
            self.lock_r = self.max_npc_page == 0;
        }
    }

    /// `SetCurNPCWords` (387-403).
    fn set_npc_words(&mut self, s: &QuestScripts, l: &Layout) {
        self.npc_words =
            s.step_npc_words(self.quest).map(|t| divide_by_pixel(t, NPC_LINE_MAX, true, l.width, l.measure)).unwrap_or_default();
        let lines = self.npc_words.len();
        if lines < 1 {
            return;
        }
        self.max_npc_page = (lines - 1) / NPC_LINES_PER_PAGE;
        self.sel_npc_page = 0;
    }

    /// `SetCurPlayerWords` (405-436): the answers start at row 2 whatever the hero's words take.
    fn set_player_words(&mut self, s: &QuestScripts, l: &Layout) {
        let mut rows = vec![String::new(); QP_PLAYER_LINE_MAX];
        self.answer_line = [0; mu_data::quest::MAX_ANSWER];
        if let Some(words) = s.step_player_words(self.quest) {
            for (i, line) in divide_by_pixel(words, 2, true, l.width, l.measure).into_iter().enumerate() {
                rows[i] = line;
            }
        }
        let mut row = 2;
        for i in 0..mu_data::quest::MAX_ANSWER {
            let Some(answer) = s.step_answer(self.quest, i) else { break };
            // `sprintf(szAnswer, "%d.", i + 1)`: no space after the dot.
            let lines = divide_by_pixel(&format!("{}.{answer}", i + 1), 2, false, l.width, l.measure);
            self.answer_line[i] = lines.len();
            for line in lines {
                // The original writes past its 9 rows here; they are dropped.
                if let Some(r) = rows.get_mut(row) {
                    *r = line;
                }
                row += 1;
            }
            if row >= QP_PLAYER_LINE_MAX {
                break;
            }
        }
        self.player_words = rows;
    }

    /// `m_btnProgressR` (109-126): past the last page of words come the hero's words.
    fn progress_right(&mut self) {
        if self.sel_npc_page == self.max_npc_page {
            if self.lower == ProgressView::NonPlayerWords {
                self.lower = ProgressView::PlayerWords;
            }
        } else {
            self.sel_npc_page = (self.sel_npc_page + 1).min(self.max_npc_page);
        }
        if self.sel_npc_page == self.max_npc_page && self.lower != ProgressView::NonPlayerWords {
            self.lock_r = true;
        }
        if self.max_npc_page != 0 {
            self.lock_l = false;
        }
    }

    /// `m_btnProgressL` (127-137).
    fn progress_left(&mut self) {
        self.sel_npc_page = self.sel_npc_page.saturating_sub(1);
        if self.sel_npc_page == 0 {
            self.lock_l = true;
        }
        self.lock_r = false;
    }

    /// The answer under the mouse (`UpdateSelTextMouseEvent`, 152-184), 1-based.
    fn answer_at(&self, m: Vec2) -> usize {
        if m.x < 11.0 || m.x > 179.0 {
            return 0;
        }
        let mut bottom = 251.0;
        for (i, &lines) in self.answer_line.iter().enumerate() {
            let top = bottom;
            bottom += lines as f32 * GAP;
            if top <= m.y && m.y < bottom {
                return i + 1;
            }
        }
        0
    }
}

/// `CQuestMng::SetCurQuestProgress` (QuestMng.cpp:261-289) for a step the server sent.
#[derive(Debug, PartialEq)]
enum StepOutcome {
    /// `LOWORD == 0x00FF`: the quest is over; the progress windows close and `GlobalText[2814]`.
    Completed,
    /// `m_byUIType` 0: `CNewUIQuestProgress`.
    Progress,
    /// Otherwise `CNewUIQuestProgressByEtc`.
    ByEtc,
}

fn step_outcome(s: &QuestScripts, quest: u32) -> StepOutcome {
    if quest & 0xFFFF == 0x00FF {
        StepOutcome::Completed
    } else if s.step_ui_type(quest) == 0 {
        StepOutcome::Progress
    } else {
        StepOutcome::ByEtc
    }
}

/// The window: `IsVisible(INTERFACE_NPC_DIALOGUE)` and its state.
#[derive(Resource, Default)]
pub struct NpcDialogueWindow {
    open: bool,
    d: Dialogue,
    /// [`NpcDialog::serial`] of the dialog shown.
    serial: u32,
    /// `IsVisible(INTERFACE_QUEST_PROGRESS)` and its state (same place, opened over the dialogue).
    progress_open: bool,
    p: Progress,
    progress_serial: u32,
    /// The mouse in the window (window coordinates) and the button held, for the frames.
    cursor: Option<Vec2>,
    held: bool,
}

impl NpcDialogueWindow {
    /// The mouse (640×480 screen) is over the window: a click there does not walk.
    pub fn blocks(&self, c: Vec2) -> bool {
        (self.open || self.progress_open) && inside(c - POS, (0.0, 0.0, W, H))
    }
}

/// `Hero->m_byGensInfluence`: 0 none, 1 Duprian, 2 Vanert (`ReceivePlayerGensInfluence` and the
/// dialogue's join and leave answers).
#[derive(Resource, Default)]
pub struct GensInfluence(pub u8);

/// `CheckMouseIn` (integer mouse position).
fn inside(c: Vec2, (x, y, w, h): (f32, f32, f32, f32)) -> bool {
    let (mx, my) = (c.x.floor(), c.y.floor());
    mx >= x && mx < x + w && my >= y && my < y + h
}

/// The scripts, loaded at start (`LoadQuestScript`).
#[derive(Resource)]
struct Scripts(QuestScripts);

fn load_scripts(args: Res<Args>, mut commands: Commands) {
    match QuestScripts::load(&args.data, "Eng") {
        Ok(s) => commands.insert_resource(Scripts(s)),
        // The original shows "... file not found." and quits (QuestMng.cpp:39-48).
        Err(e) => warn!("diálogo de NPC: sin los guiones de misiones: {e}"),
    }
}

// --- Systems -----------------------------------------------------------------------------------

/// The text width at the window's resolution.
fn with_layout<R>(windows: &Query<&Window>, metrics: &crate::chat::TextMetrics, f: impl FnOnce(&Layout) -> R) -> R {
    let ww = windows.single().map_or(1024, |w| w.physical_width());
    let font = font_size(ww);
    let measure = |s: &str| metrics.width_at(s, font);
    f(&Layout { measure: &measure, width: TEXT_WIDTH - 5.0 * 640.0 / ww.max(1) as f32 })
}

/// Opens and closes with npc.rs' dialog and applies the server's answers.
#[allow(clippy::too_many_arguments)]
fn follow_dialog(
    mut from: MessageReader<crate::net::FromServer>,
    mut dialog: ResMut<NpcDialog>,
    mut w: ResMut<NpcDialogueWindow>,
    mut gens: ResMut<GensInfluence>,
    scripts: Option<Res<Scripts>>,
    (windows, metrics): (Query<&Window>, Res<crate::chat::TextMetrics>),
    monsters: Query<&crate::monsters::Monster>,
    (mut inv, mut char_info): (ResMut<crate::inventory::Inventory>, ResMut<crate::char_info::CharInfoWindow>),
    mut sound: ResMut<SoundQueue>,
    (mut net, mut chat): (ResMut<crate::net::Net>, ResMut<crate::chat::Chat>),
) {
    let empty = QuestScripts::default();
    let s = scripts.as_ref().map_or(&empty, |s| &s.0);
    let is_dialogue = dialog.kind() == Some(WindowKind::NpcDialogue);
    // Closed by npc.rs (Esc, walking, another window): `ProcessClosing`, except when the quest
    // progress window hid it (`HideAllGroupA`'s `ShowInterface(..., false)`).
    if w.open && (!is_dialogue || w.serial != dialog.serial) {
        w.open = false;
        w.d = Dialogue::default();
        if dialog.kind() != Some(WindowKind::QuestProgress) {
            sound.play("SOUND_CLICK01");
        }
        info!("diálogo de NPC: se cierra");
    }
    if w.progress_open && (dialog.kind() != Some(WindowKind::QuestProgress) || w.progress_serial != dialog.serial) {
        // `CNewUIQuestProgress::ProcessClosing` (NewUIQuestProgress.cpp:347-353).
        w.progress_open = false;
        w.p = Progress::default();
        sound.play("SOUND_CLICK01");
        info!("progreso de misión: se cierra");
    }
    for m in from.read() {
        match &m.0 {
            // `ReceiveNPCDlgUIStart`: `SetNPC` and `SetContributePoint` unless it is open.
            ServerMessage::OpenNpcDialog { npc, contribution } if !w.open => {
                w.d.npc = *npc;
                let name = monsters
                    .iter()
                    .find(|m| Some(m.key) == dialog.npc_key && m.number == *npc)
                    .map(|m| m.def.display_name())
                    .unwrap_or_else(|| mu_data::monsters::defs(*npc, -1)[0].display_name());
                w.d.npc_name = name.to_string();
                // Only the stewards keep it (`SetContributePoint`, 541-545).
                w.d.contribution = if matches!(*npc, DUPRIAN_STEWARD | VANERT_STEWARD) { *contribution } else { 0 };
            }
            ServerMessage::GensInfo { gens: g, .. } => gens.0 = *g,
            // The dialogue's answers count only while it is shown (WSclient.cpp:9742-9899).
            ServerMessage::AvailableQuests { quests, .. } if w.open => {
                with_layout(&windows, &metrics, |l| w.d.quest_list_received(s, l, quests));
            }
            ServerMessage::GensJoinResult { result, gens: g } if w.open => {
                with_layout(&windows, &metrics, |l| w.d.gens_join_received(s, l, *result, *g, &mut gens.0));
            }
            ServerMessage::GensLeaveResult { result } if w.open => {
                with_layout(&windows, &metrics, |l| w.d.gens_leave_received(s, l, *result, &mut gens.0));
            }
            ServerMessage::GensRewardResult { result } if w.open => {
                with_layout(&windows, &metrics, |l| w.d.gens_reward_received(s, l, *result));
            }
            // `ReceiveQuestQSSelSentence` and `ReceiveQuestQSRequestReward` → `SetCurQuestProgress`
            // (QuestMng.cpp:261-289). (`SetQuestRequestReward`, the requests and rewards, is not
            // decoded yet; `AddCurQuestIndexList` and `SetEPRequestRewardState` feed the quest list
            // window, which does not exist yet.)
            ServerMessage::QuestStepInfo { quest } | ServerMessage::QuestRequestReward { quest } => match step_outcome(s, *quest) {
                StepOutcome::Completed => {
                    if w.progress_open {
                        crate::npc::close_dialog(&mut dialog, &mut net, &mut inv, "misión terminada");
                    }
                    chat.add("", QUEST_COMPLETED, crate::chat_log::MsgType::Error, crate::chat_log::MsgType::All, &metrics);
                }
                StepOutcome::Progress => {
                    with_layout(&windows, &metrics, |l| w.p.set_contents(s, l, *quest));
                    if !w.progress_open {
                        // `Show(INTERFACE_QUEST_PROGRESS)`: `HideAllGroupA` (the dialogue hides
                        // without its packet) and `ProcessOpening` (`SOUND_INTERFACE01`).
                        w.p.npc_name = w.d.npc_name.clone();
                        if inv.open {
                            inv.toggle();
                        }
                        char_info.close();
                        dialog.replace_kind(WindowKind::QuestProgress);
                        w.progress_open = true;
                        w.progress_serial = dialog.serial;
                        sound.play("SOUND_INTERFACE01");
                        info!("progreso de misión {quest:#010x}: se abre");
                    }
                }
                // TODO: `CNewUIQuestProgressByEtc` (NewUIQuestProgressByEtc.cpp), the steps the
                // NPC does not give (`m_byUIType` != 0).
                StepOutcome::ByEtc => warn!("progreso de misión {quest:#010x}: falta la ventana de misiones sin NPC"),
            },
            _ => {}
        }
    }
    // The dialogue window's own state followed `dialog` before the step; with the progress window
    // open over it, it is already hidden.
    if w.open && dialog.kind() == Some(WindowKind::QuestProgress) {
        w.open = false;
        w.d = Dialogue::default();
    }
    let is_dialogue = dialog.kind() == Some(WindowKind::NpcDialogue);
    if is_dialogue && !w.open {
        // `Show(INTERFACE_NPC_DIALOGUE)`: `HideAllGroupA` and `ProcessOpening`.
        if inv.open {
            inv.toggle();
        }
        char_info.close();
        w.open = true;
        w.serial = dialog.serial;
        w.d.quest_list_mode = false;
        with_layout(&windows, &metrics, |l| w.d.set_contents(s, l, 0));
        sound.play("SOUND_INTERFACE01");
        info!("diálogo de NPC {} ({}): se abre", w.d.npc, w.d.npc_name);
    }
}

/// `UpdateMouseEvent` (84-96): the buttons, then the answers.
#[allow(clippy::too_many_arguments)]
fn update_mouse(
    windows: Query<&Window>,
    mouse: Res<ButtonInput<MouseButton>>,
    game_ui: Res<crate::options::GameUi>,
    metrics: Res<crate::chat::TextMetrics>,
    scripts: Option<Res<Scripts>>,
    mut w: ResMut<NpcDialogueWindow>,
    (mut dialog, mut net, mut inv): (ResMut<NpcDialog>, ResMut<crate::net::Net>, ResMut<crate::inventory::Inventory>),
    mut sound: ResMut<SoundQueue>,
) {
    if !w.open && !w.progress_open {
        return;
    }
    let cursor = windows
        .single()
        .ok()
        .and_then(crate::mainframe::cursor_virtual)
        .filter(|&c| !game_ui.modal() && !game_ui.blocks(c))
        .map(|c| (c - POS).floor());
    w.cursor = cursor;
    w.held = mouse.pressed(MouseButton::Left);
    let Some(m) = cursor else { return };
    let (pressed, released) = (mouse.just_pressed(MouseButton::Left), mouse.just_released(MouseButton::Left));
    // `CNewUIButton::Process`: true on the release over it (when not locked).
    let clicked = |d: &Dialogue, b: Btn| released && !d.locked(b) && inside(m, b.rect());
    let close = |dialog: &mut NpcDialog, net: &mut crate::net::Net, inv: &mut crate::inventory::Inventory, why: &str| {
        crate::npc::close_dialog(dialog, net, inv, why);
    };

    if w.progress_open {
        // `CNewUIQuestProgress::ProcessBtns` (97-150).
        let p = &mut w.p;
        if released && inside(m, BTN_CLOSE) {
            close(&mut dialog, &mut net, &mut inv, "botón cerrar del progreso de misión");
            return;
        }
        if pressed && inside(m, TOP_X) {
            close(&mut dialog, &mut net, &mut inv, "X del progreso de misión");
            return;
        }
        if released && !p.lock_r && inside(m, QP_BTN_R) {
            p.progress_right();
            sound.play("SOUND_CLICK01");
            return;
        }
        if released && !p.lock_l && inside(m, QP_BTN_L) {
            p.progress_left();
            sound.play("SOUND_CLICK01");
            return;
        }
        // (`m_btnComplete` comes with the requests list, TODO.)
        // `UpdateSelTextMouseEvent` (152-184).
        if p.lower != ProgressView::PlayerWords || !p.can_click {
            return;
        }
        p.sel_answer = p.answer_at(m);
        if p.sel_answer != 0 && released {
            let packet = ClientMessage::QuestProceed { quest: p.quest, answer: p.sel_answer as u8 };
            info!("progreso de misión: se manda {packet:?}");
            if net.connected() {
                net.send(packet);
            }
            sound.play("SOUND_CLICK01");
            p.can_click = false;
        }
        return;
    }

    // `ProcessBtns` (98-164).
    if clicked(&w.d, Btn::Close) {
        close(&mut dialog, &mut net, &mut inv, "botón cerrar del diálogo");
        return;
    }
    if pressed && inside(m, TOP_X) {
        close(&mut dialog, &mut net, &mut inv, "X del diálogo");
        return;
    }
    let turned = if clicked(&w.d, Btn::ProgressR) {
        w.d.progress_right();
        true
    } else if clicked(&w.d, Btn::ProgressL) {
        w.d.progress_left();
        true
    } else if clicked(&w.d, Btn::SelR) {
        w.d.sel_right();
        true
    } else if clicked(&w.d, Btn::SelL) {
        w.d.sel_left();
        true
    } else {
        false
    };
    if turned {
        sound.play("SOUND_CLICK01");
        return;
    }

    // `UpdateSelTextMouseEvent` (166-205).
    if w.d.lower != LowerView::SelTexts || !w.d.can_click {
        return;
    }
    w.d.sel_sel_text = w.d.answer_at(m);
    if w.d.sel_sel_text != 0 && released {
        w.d.can_click = false;
        let empty = QuestScripts::default();
        let s = scripts.as_ref().map_or(&empty, |s| &s.0);
        let action = with_layout(&windows, &metrics, |l| w.d.sel_text_result(s, l));
        sound.play("SOUND_CLICK01");
        let send = |net: &mut crate::net::Net, p: ClientMessage| {
            info!("diálogo de NPC: se manda {p:?}");
            if net.connected() {
                net.send(p);
            }
        };
        match action {
            Some(Action::Send(p)) => send(&mut net, p),
            Some(Action::SendAndClose(p)) => {
                send(&mut net, p);
                close(&mut dialog, &mut net, &mut inv, "el diálogo pide el buff");
            }
            None => {}
        }
    }
}

// --- Drawing -----------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Tex {
    Back,
    Top,
    Left,
    Right,
    Bottom,
    Line,
    BtnL,
    BtnR,
    BtnClose,
    ContributeBg,
}

impl Tex {
    const ALL: [Tex; 10] =
        [Tex::Back, Tex::Top, Tex::Left, Tex::Right, Tex::Bottom, Tex::Line, Tex::BtnL, Tex::BtnR, Tex::BtnClose, Tex::ContributeBg];

    /// `LoadImages` (343-356).
    fn file(self) -> &'static str {
        match self {
            Tex::Back => "newui_msgbox_back.jpg",
            Tex::Top => "newui_item_back04.tga",
            Tex::Left => "newui_item_back02-L.tga",
            Tex::Right => "newui_item_back02-R.tga",
            Tex::Bottom => "newui_item_back03.tga",
            Tex::Line => "newui_myquest_Line.tga",
            Tex::BtnL => "Quest_bt_L.tga",
            Tex::BtnR => "Quest_bt_R.tga",
            Tex::BtnClose => "newui_exit_00.tga",
            Tex::ContributeBg => "Gens_point.tga",
        }
    }
}

/// A piece of the window (coordinates inside it).
#[derive(Clone, PartialEq, Debug)]
enum Piece {
    /// `RenderImage(tex, x, y, w, h, 0, sv)`: the w × h part of the texture from (0, sv).
    Image { tex: Tex, x: f32, y: f32, w: f32, h: f32, sv: f32 },
    /// `RenderColor`.
    Fill { x: f32, y: f32, w: f32, h: f32, rgba: [u8; 4] },
    /// `RenderText`: centered in `center` when set (`RT3_SORT_CENTER`).
    Text { x: f32, y: f32, center: Option<f32>, text: String, color: [u8; 3], bold: bool },
    /// The close button's tooltip (`CNewUIButton::Render`, NewUIButton.cpp:439-460).
    Tooltip { cx: f32, y: f32, text: String },
}

fn img(tex: Tex, x: f32, y: f32, w: f32, h: f32) -> Piece {
    Piece::Image { tex, x, y, w, h, sv: 0.0 }
}

/// `Render` (226-252).
fn pieces(d: &Dialogue, gens: u8, cursor: Option<Vec2>, held: bool) -> Vec<Piece> {
    let mut out = Vec::new();
    // `RenderBackImage` (254-263).
    out.push(img(Tex::Back, 0.0, 0.0, W, H));
    out.push(img(Tex::Top, 0.0, 0.0, W, 64.0));
    out.push(img(Tex::Left, 0.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Right, W - 21.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Bottom, 0.0, H - 45.0, W, 45.0));
    out.push(img(Tex::Line, 1.0, 181.0, 188.0, 21.0));

    // `RenderSelTextBlock` (265-287).
    if d.lower == LowerView::SelTexts && d.sel_sel_text != 0 {
        let start = d.page_answers().start;
        let lines = |i: usize| d.sel_text_line.get(i).copied().unwrap_or(0) as f32;
        let y = 203.0 + (start..d.sel_sel_text - 1).map(|i| GAP * lines(i)).sum::<f32>();
        out.push(Piece::Fill { x: 11.0, y, w: 168.0, h: GAP * lines(d.sel_sel_text - 1), rgba: BLOCK_RGBA });
    }

    // `RenderText` (289-317).
    out.push(Piece::Text { x: 0.0, y: 12.0, center: Some(W), text: d.npc_name.clone(), color: NAME_COLOR, bold: true });
    for i in 0..NPC_LINES_PER_PAGE {
        if let Some(line) = d.npc_words.get(i + NPC_LINES_PER_PAGE * d.sel_npc_page) {
            out.push(Piece::Text { x: 13.0, y: 59.0 + GAP * i as f32, center: None, text: line.clone(), color: TEXT_COLOR, bold: false });
        }
    }
    if d.lower == LowerView::SelTexts {
        let page = d.sel_sel_text_page.min(SEL_TEXT_PAGE_LIMIT - 1);
        let start: usize = d.lines_per_page[..page].iter().sum();
        for i in 0..d.lines_per_page[page] {
            if let Some(line) = d.sel_texts.get(start + i) {
                out.push(Piece::Text { x: 13.0, y: 207.0 + GAP * i as f32, center: None, text: line.clone(), color: TEXT_COLOR, bold: false });
            }
        }
    }

    // `RenderContributePoint` (319-331).
    if (d.npc == DUPRIAN_STEWARD && gens == 1) || (d.npc == VANERT_STEWARD && gens == 2) {
        out.push(img(Tex::ContributeBg, 11.0, 27.0, 168.0, 18.0));
        out.push(Piece::Text { x: 0.0, y: 30.0, center: Some(W), text: contribution_text(d.contribution), color: TEXT_COLOR, bold: false });
    }

    // The buttons: frame 0, or 1 held down over it (`ChangeButtonImgState(true, img)`); the locked
    // ones are not drawn.
    let over = |b: Btn| cursor.is_some_and(|c| inside(c, b.rect()));
    for (b, tex) in [(Btn::ProgressL, Tex::BtnL), (Btn::ProgressR, Tex::BtnR), (Btn::SelL, Tex::BtnL), (Btn::SelR, Tex::BtnR), (Btn::Close, Tex::BtnClose)] {
        if d.locked(b) {
            continue;
        }
        let (x, y, w, h) = b.rect();
        let sv = if held && over(b) { h } else { 0.0 };
        out.push(Piece::Image { tex, x, y, w, h, sv });
    }
    if over(Btn::Close) {
        let (x, y, w, h) = BTN_CLOSE;
        out.push(Piece::Tooltip { cx: x + w / 2.0, y: y + h + 2.0, text: CLOSE_TIP.to_string() });
    }
    out
}

/// `CNewUIQuestProgress::Render` (205-233).
fn progress_pieces(p: &Progress, hero_name: &str, cursor: Option<Vec2>, held: bool) -> Vec<Piece> {
    let mut out = Vec::new();
    // `RenderBackImage` (235-243).
    out.push(img(Tex::Back, 0.0, 0.0, W, H));
    out.push(img(Tex::Top, 0.0, 0.0, W, 64.0));
    out.push(img(Tex::Left, 0.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Right, W - 21.0, 64.0, 21.0, 320.0));
    out.push(img(Tex::Bottom, 0.0, H - 45.0, W, 45.0));
    out.push(img(Tex::Line, 1.0, 181.0, 188.0, 21.0));

    // `RenderSelTextBlock` (245-261).
    if p.lower == ProgressView::PlayerWords && p.sel_answer != 0 {
        let y = 251.0 + p.answer_line[..p.sel_answer - 1].iter().map(|&n| n as f32 * GAP).sum::<f32>();
        out.push(Piece::Fill { x: 11.0, y, w: 168.0, h: GAP * p.answer_line[p.sel_answer - 1] as f32, rgba: BLOCK_RGBA });
    }

    // `RenderText` (263-301): bold for the titles and names.
    let text = |x: f32, y: f32, center: Option<f32>, t: &str, color: [u8; 3], bold: bool| Piece::Text {
        x,
        y,
        center,
        text: t.to_string(),
        color,
        bold,
    };
    out.push(text(0.0, 12.0, Some(W), QP_TITLE, QP_TITLE_COLOR, true));
    out.push(text(0.0, 27.0, Some(W), &p.subject, QP_SUBJECT_COLOR, true));
    out.push(text(13.0, 51.0, None, &p.npc_name, QP_NPC_COLOR, true));
    if p.lower != ProgressView::RequestReward {
        out.push(text(13.0, 207.0, None, hero_name, QP_PLAYER_COLOR, true));
    }
    for i in 0..NPC_LINES_PER_PAGE {
        if let Some(line) = p.npc_words.get(i + NPC_LINES_PER_PAGE * p.sel_npc_page) {
            out.push(text(13.0, 66.0 + GAP * i as f32, None, line, TEXT_COLOR, false));
        }
    }
    if p.lower == ProgressView::PlayerWords {
        for (i, line) in p.player_words.iter().enumerate().filter(|(_, l)| !l.is_empty()) {
            // The hero's two lines at 222, the answers 3 lower (225).
            let y = if i < 2 { 222.0 } else { 225.0 } + GAP * i as f32;
            out.push(text(13.0, y, None, line, TEXT_COLOR, false));
        }
    }

    // The buttons (the complete button and the list box of `REQUEST_REWARD_MODE`: TODO).
    let over = |r: (f32, f32, f32, f32)| cursor.is_some_and(|c| inside(c, r));
    for (locked, tex, r) in [(p.lock_l, Tex::BtnL, QP_BTN_L), (p.lock_r, Tex::BtnR, QP_BTN_R), (false, Tex::BtnClose, BTN_CLOSE)] {
        if locked {
            continue;
        }
        let (x, y, w, h) = r;
        out.push(Piece::Image { tex, x, y, w, h, sv: if held && over(r) { h } else { 0.0 } });
    }
    if over(BTN_CLOSE) {
        let (x, y, w, h) = BTN_CLOSE;
        out.push(Piece::Tooltip { cx: x + w / 2.0, y: y + h + 2.0, text: CLOSE_TIP.to_string() });
    }
    out
}

/// The textures and their sizes.
#[derive(Resource)]
struct DialogueTextures(std::collections::HashMap<Tex, (Handle<Image>, Vec2)>);

#[derive(Component)]
struct DialogueRoot;

/// The 640×480 screen the window sits on.
#[derive(Component)]
struct DialogueScreen;

fn spawn_root(mut commands: Commands, args: Res<Args>, mut images: ResMut<Assets<Image>>) {
    let textures = Tex::ALL
        .iter()
        .filter_map(|&t| crate::scene::interface_image(&args, &mut images, t.file()).map(|h| (t, h)))
        .collect();
    commands.insert_resource(DialogueTextures(textures));
    // The 640×480 screen centered and bottom-aligned, like the character window.
    commands
        .spawn((
            Name::new("Diálogo de NPC"),
            DialogueScreen,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(0.0),
                left: Val::Percent(50.0),
                margin: UiRect::left(Val::Px(-320.0)),
                width: Val::Px(640.0),
                height: Val::Px(480.0),
                ..default()
            },
            // Layer 3.1 (`GetLayerDepth`), below the window menu and the options.
            GlobalZIndex(0),
        ))
        .with_children(|p| {
            p.spawn((DialogueRoot, absolute(POS.x, POS.y, W, H), Visibility::Hidden));
        });
}

/// Rebuilds the window when what it draws changes.
fn draw(
    mut commands: Commands,
    w: Res<NpcDialogueWindow>,
    gens: Res<GensInfluence>,
    hero: Option<Res<crate::account::Hero>>,
    windows: Query<&Window>,
    textures: Option<Res<DialogueTextures>>,
    mut root: Query<(Entity, &mut Visibility), With<DialogueRoot>>,
    mut last: Local<(Option<Entity>, Vec<Piece>, f32)>,
) {
    let (Ok((root, mut visibility)), Some(textures)) = (root.single_mut(), textures) else { return };
    let open = w.open || w.progress_open;
    let shown = if open { Visibility::Inherited } else { Visibility::Hidden };
    if *visibility != shown {
        *visibility = shown;
    }
    if !open {
        return;
    }
    let pieces = if w.progress_open {
        progress_pieces(&w.p, hero.as_ref().map_or("", |h| h.name.as_str()), w.cursor, w.held)
    } else {
        pieces(&w.d, gens.0, w.cursor, w.held)
    };
    let font = font_size(windows.single().map_or(1024, |w| w.physical_width()));
    if last.0 == Some(root) && last.1 == pieces && last.2 == font {
        return;
    }
    commands.entity(root).despawn_children();
    commands.entity(root).with_children(|p| {
        for piece in &pieces {
            match piece {
                Piece::Image { tex, x, y, w, h, sv } => {
                    let Some((handle, _)) = textures.0.get(tex) else { continue };
                    let mut image = ImageNode::new(handle.clone());
                    image.rect = Some(Rect::new(0.0, *sv, *w, sv + h));
                    p.spawn((absolute(*x, *y, *w, *h), image));
                }
                Piece::Fill { x, y, w, h, rgba: [r, g, b, a] } => {
                    p.spawn((absolute(*x, *y, *w, *h), BackgroundColor(Color::srgba_u8(*r, *g, *b, *a))));
                }
                Piece::Text { x, y, center: Some(cw), text, color, bold } => {
                    let mut node = absolute(*x, *y, *cw, font);
                    node.justify_content = JustifyContent::Center;
                    p.spawn(node).with_children(|c| {
                        c.spawn(label(text, *color, *bold, font));
                    });
                }
                Piece::Text { x, y, center: None, text, color, bold } => {
                    p.spawn((
                        Node { position_type: PositionType::Absolute, left: Val::Px(*x), top: Val::Px(*y), ..default() },
                        label(text, *color, *bold, font),
                    ));
                }
                Piece::Tooltip { cx, y, text } => {
                    // Black box with alpha 180, 6 wider than the text, centered under the button.
                    let guess = text.chars().count() as f32 * font * 0.5 + 6.0;
                    p.spawn((
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
                        c.spawn(label(text, [255, 255, 255], false, font));
                    });
                }
            }
        }
    });
    *last = (Some(root), pieces, font);
}

#[cfg(test)]
mod tests {
    use super::*;
    use mu_data::quest::NpcDialogue;
    use std::collections::HashMap;

    /// Every character 5 units wide.
    fn layout_measure(s: &str) -> f32 {
        s.chars().count() as f32 * 5.0
    }

    fn layout() -> Layout<'static> {
        Layout { measure: &layout_measure, width: 160.0 }
    }

    /// The Gens Duprian steward's states 0, 3 and 5 and the quest-list words.
    fn scripts() -> QuestScripts {
        let mut words = HashMap::new();
        for (i, t) in [
            (1000, "How do you do?"),
            (1005, "Accept a gens quest."),
            (1001, "Ask about Gens Duprian."),
            (1003, "Join Gens Duprian."),
            (1063, "Gens Ranking Reward"),
            (1009, "Oh! You wish to become a member?"),
            (1010, "Yes. I wish to join."),
            (1011, "No. I will give it more thought."),
            (1014, "Congratulations."),
            (1017, "You have to be at least level 50 in order to join the gens."),
            (1007, "Go back."),
            (1500, "NPC dialog script error!"),
            (1501, "Select a quest."),
            (1502, "There is no available quest. Please come back later."),
            (18901, "Spider Hunt!"),
        ] {
            words.insert(i, t.to_string());
        }
        let dlg = |npc_words, answers: &[(i32, i32)]| {
            let mut a = [(0, 0); 10];
            a[..answers.len()].copy_from_slice(answers);
            NpcDialogue { npc_words, answers: a }
        };
        let mut dialogues = HashMap::new();
        let key = |s: u32| 543 * 0x10000 + s;
        dialogues.insert(key(0), dlg(1000, &[(1005, 901), (1001, 1), (1003, 3), (1063, 906)]));
        dialogues.insert(key(3), dlg(1009, &[(1010, 903), (1011, 0)]));
        dialogues.insert(key(5), dlg(1014, &[(1007, 0)]));
        dialogues.insert(key(8), dlg(1017, &[(1007, 0)]));
        dialogues.insert(key(999), dlg(1500, &[(1007, 0)]));
        let mut progress = HashMap::new();
        progress.insert(
            0x0012_0000,
            mu_data::quest::QuestProgress { ui_type: 0, npc_words: 0, player_words: 0, answers: [0; 5], subject: 18901, summary: 0 },
        );
        QuestScripts { words, dialogues, progress }
    }

    fn opened() -> Dialogue {
        let mut d = Dialogue { npc: 543, ..Default::default() };
        d.set_contents(&scripts(), &layout(), 0);
        d
    }

    #[test]
    fn splitting_like_cut_str() {
        let m = &layout_measure;
        // 160 / 5 = 32 characters fit; the line is cut after the last space.
        let lines = divide_by_pixel("aaaa bbbb cccc dddd eeee ffff gggg hhhh", NPC_LINE_MAX, true, 160.0, m);
        assert_eq!(lines, vec![" aaaa bbbb cccc dddd eeee ffff ", "gggg hhhh"]);
        // `;` starts a paragraph (empty ones skipped), each with its leading space.
        assert_eq!(divide_by_pixel("one;;two", NPC_LINE_MAX, true, 160.0, m), vec![" one", " two"]);
        assert_eq!(divide_by_pixel("1. Go back.", 2, false, 160.0, m), vec!["1. Go back."]);
        // No space: cut where it does not fit.
        let long = "x".repeat(40);
        assert_eq!(cut_str(&long, 160.0, 2, m), vec!["x".repeat(32), "x".repeat(8)]);
        // At most `max_lines`, and never more than 63 characters a line.
        assert_eq!(cut_str(&"y ".repeat(60), 10_000.0, 1, m).len(), 1);
        assert!(cut_str(&"z".repeat(100), 10_000.0, 5, m).iter().all(|l| l.chars().count() <= 63));
    }

    #[test]
    fn opening_shows_state_0_with_its_answers() {
        let d = opened();
        assert_eq!(d.npc_words, vec![" How do you do?"]);
        assert_eq!(d.sel_text_count, 4);
        assert_eq!(d.sel_texts[0], "1. Accept a gens quest.");
        assert_eq!(d.lower, LowerView::SelTexts, "one page of words: the answers show at once");
        assert!(d.lock_progress_l && d.lock_progress_r && d.lock_sel_l && d.lock_sel_r);
        assert!(d.can_click);
        assert_eq!((d.count_per_page[0], d.lines_per_page[0]), (4, 4));
    }

    #[test]
    fn answers_go_to_states_or_ask_the_server() {
        let (s, l) = (scripts(), layout());
        let mut d = opened();
        // Mouse over the third answer: 203 + 2 lines × 15.
        d.sel_sel_text = d.answer_at(Vec2::new(20.0, 233.0));
        assert_eq!(d.sel_sel_text, 3);
        assert_eq!(d.answer_at(Vec2::new(5.0, 233.0)), 0, "left of the answers");
        assert_eq!(d.answer_at(Vec2::new(20.0, 203.0 + 4.0 * 15.0)), 0, "below the last one");
        assert_eq!(d.sel_text_result(&s, &l), None);
        assert_eq!(d.cur_dlg, 3, "Join Gens Duprian. → state 3");
        // The second answer takes two lines (32 characters fit).
        assert_eq!(d.sel_texts, vec!["1. Yes. I wish to join.", "2. No. I will give it more ", "thought."]);
        assert_eq!(d.sel_text_line, vec![1, 2]);
        d.sel_sel_text = 1;
        d.can_click = false;
        assert_eq!(d.sel_text_result(&s, &l), Some(Action::Send(ClientMessage::GensJoin { gens: 1 })));
        assert!(!d.can_click, "waits for the server");
        // Level too low (the captured answer), then joined.
        let mut gens = 0;
        d.gens_join_received(&s, &l, 3, 1, &mut gens);
        assert_eq!((d.cur_dlg, gens, d.can_click), (8, 0, true));
        d.gens_join_received(&s, &l, 0, 1, &mut gens);
        assert_eq!((d.cur_dlg, gens), (5, 1));
        d.gens_join_received(&s, &l, 0xEE, 1, &mut gens);
        assert_eq!(d.cur_dlg, 5, "unknown results change nothing");
        let mut out = 1;
        d.gens_leave_received(&s, &l, 0, &mut out);
        assert_eq!((d.cur_dlg, out), (16, 0));
        d.gens_reward_received(&s, &l, 6);
        assert_eq!(d.cur_dlg, 25);
        // An unknown action: the script error state.
        let mut d = opened();
        d.sel_sel_text = 4;
        assert_eq!(d.sel_text_result(&s, &l), Some(Action::Send(ClientMessage::GensReward { gens: 1 })));
        d.sel_sel_text = 1;
        assert_eq!(d.sel_text_result(&s, &l), Some(Action::Send(ClientMessage::AvailableQuestsRequest)));
    }

    #[test]
    fn quest_list() {
        let (s, l) = (scripts(), layout());
        let mut d = opened();
        d.quest_list_received(&s, &l, &[]);
        assert_eq!(d.npc_words, vec![" There is no available quest. ", "Please come back later."]);
        assert_eq!(d.sel_texts, vec!["1. Go back."]);
        d.quest_list_received(&s, &l, &[0x0012_0000]);
        assert_eq!(d.npc_words, vec![" Select a quest."]);
        assert_eq!(d.sel_texts, vec!["1. [Q]Spider Hunt!", "2. Go back."]);
        d.sel_sel_text = 1;
        assert_eq!(d.sel_text_result(&s, &l), Some(Action::Send(ClientMessage::QuestSelect { quest: 0x0012_0000, selected: 1 })));
        // The last line goes back to state 0.
        d.sel_sel_text = 2;
        assert_eq!(d.sel_text_result(&s, &l), None);
        assert!(!d.quest_list_mode);
        assert_eq!(d.cur_dlg, 0);
    }

    #[test]
    fn pages_of_words_and_answers() {
        let (s, l) = (scripts(), layout());
        let mut d = Dialogue { npc: 543, ..Default::default() };
        // 20 lines of words: 3 pages; the answers wait for the last one.
        let mut s2 = s.clone();
        s2.words.insert(1000, (0..20).map(|i| format!("line{i}")).collect::<Vec<_>>().join(";"));
        d.set_contents(&s2, &l, 0);
        assert_eq!((d.max_npc_page, d.lower, d.lock_progress_r), (2, LowerView::NonSelTexts, false));
        d.progress_right();
        assert_eq!((d.sel_npc_page, d.lower, d.lock_progress_l), (1, LowerView::NonSelTexts, false));
        d.progress_right();
        assert_eq!((d.sel_npc_page, d.lower, d.lock_progress_r), (2, LowerView::SelTexts, true));
        d.progress_left();
        d.progress_left();
        assert_eq!((d.sel_npc_page, d.lock_progress_l, d.lock_progress_r), (0, true, false));
        // Answers of 2 lines each: 5 fit in 11 lines, the 6th goes to the next page.
        d.sel_text_line = vec![2; 6];
        d.calculate_sel_text_max_page(6);
        assert_eq!((d.max_sel_text_page, d.count_per_page, d.lock_sel_r), (1, [5, 1, 0, 0], false));
        d.sel_right();
        assert_eq!((d.sel_sel_text_page, d.lock_sel_r, d.lock_sel_l), (1, true, false));
        assert_eq!(d.page_answers(), 5..6);
        assert_eq!(d.answer_at(Vec2::new(20.0, 210.0)), 6);
    }

    /// The Spider Hunt's offer (0x120000: words, hero's words and two answers) and its accepted step
    /// (0x120001, no answers).
    fn quest_scripts() -> QuestScripts {
        let mut s = scripts();
        for (i, t) in [(18001, "Spiders!"), (18002, "I will help."), (18003, "Accept."), (18004, "Refuse."), (18006, "Hunt 10.")] {
            s.words.insert(i, t.to_string());
        }
        let step = |npc_words, player_words, answers: [i32; 5]| mu_data::quest::QuestProgress {
            ui_type: 0,
            npc_words,
            player_words,
            answers,
            subject: 18901,
            summary: 0,
        };
        s.progress.insert(0x0012_0000, step(18001, 18002, [18003, 18004, 0, 0, 0]));
        s.progress.insert(0x0012_0001, step(18006, 0, [0; 5]));
        s
    }

    #[test]
    fn quest_progress_offer_and_answers() {
        let (s, l) = (quest_scripts(), layout());
        assert_eq!(step_outcome(&s, 0x0012_0000), StepOutcome::Progress);
        assert_eq!(step_outcome(&s, 0x0012_00FF), StepOutcome::Completed);
        let mut by_etc = s.clone();
        by_etc.progress.get_mut(&0x0012_0001).unwrap().ui_type = 1;
        assert_eq!(step_outcome(&by_etc, 0x0012_0001), StepOutcome::ByEtc);

        let mut p = Progress::default();
        p.set_contents(&s, &l, 0x0012_0000);
        assert_eq!((p.subject.as_str(), p.npc_words.clone()), ("Spider Hunt!", vec![" Spiders!".to_string()]));
        assert_eq!(p.player_words[..4], [" I will help.", "", "1.Accept.", "2.Refuse."]);
        assert_eq!(p.answer_line, [1, 1, 0, 0, 0]);
        // The answers show after the arrow.
        assert_eq!((p.lower, p.lock_r, p.lock_l), (ProgressView::NonPlayerWords, false, true));
        p.progress_right();
        assert_eq!((p.lower, p.lock_r), (ProgressView::PlayerWords, true));
        assert_eq!(p.answer_at(Vec2::new(20.0, 251.0 + 15.0)), 2);
        assert_eq!(p.answer_at(Vec2::new(20.0, 251.0 + 30.0)), 0);
        let pieces = progress_pieces(&p, "Probador1", None, false);
        assert!(pieces.iter().any(|x| matches!(x, Piece::Text { y, text, .. } if *y == 255.0 && text == "1.Accept.")));
        assert!(pieces.iter().any(|x| matches!(x, Piece::Text { y, text, .. } if *y == 207.0 && text == "Probador1")));
        // The accepted step has no answers: the requests mode (its list is TODO).
        p.set_contents(&s, &l, 0x0012_0001);
        assert_eq!((p.lower, p.lock_r), (ProgressView::RequestReward, true));
        assert!(!progress_pieces(&p, "Probador1", None, false).iter().any(|x| matches!(x, Piece::Text { text, .. } if text == "Probador1")));
    }

    #[test]
    fn drawing() {
        let mut d = opened();
        d.npc_name = "Gens Duprian Steward".into();
        let p = pieces(&d, 0, None, false);
        assert!(p.iter().any(|p| matches!(p, Piece::Text { y, text, .. } if *y == 12.0 && text == "Gens Duprian Steward")));
        assert!(p.iter().any(|p| matches!(p, Piece::Text { y, text, .. } if *y == 207.0 && text == "1. Accept a gens quest.")));
        // Locked arrows are not drawn; the close button is.
        assert!(!p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::BtnL | Tex::BtnR, .. })));
        assert!(p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::BtnClose, .. })));
        assert!(!p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::ContributeBg, .. })), "not a member");
        d.contribution = 120;
        let p = pieces(&d, 1, Some(Vec2::new(20.0, 400.0)), true);
        assert!(p.iter().any(|p| matches!(p, Piece::Text { text, .. } if text == "Gain Contribution: 120")));
        assert!(p.iter().any(|p| matches!(p, Piece::Image { tex: Tex::BtnClose, sv, .. } if *sv == 29.0)), "held down");
        assert!(p.iter().any(|p| matches!(p, Piece::Tooltip { text, .. } if text == "Close")));
        // The hovered answer's block.
        d.sel_sel_text = 2;
        let p = pieces(&d, 0, None, false);
        assert!(p.contains(&Piece::Fill { x: 11.0, y: 218.0, w: 168.0, h: 15.0, rgba: BLOCK_RGBA }));
    }
}
