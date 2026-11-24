//! The scripts of the S6 quest system (`CQuestMng`, QuestMng.cpp): the texts
//! (`Data/Local/<lang>/QuestWords_<lang>.bmd`) and the NPC dialogues (`Data/Local/NPCDialogue.bmd`)
//! that the NPC dialogue window (`CNewUINPCDialogue`) shows.
//!
//! `LoadQuestWordsScript` (QuestMng.cpp:99-136): records of a packed `{ int m_nIndex; short
//! m_nWordsLen; }` header (6 bytes) and `m_nWordsLen` bytes of text, each part through
//! `BuxConvert` on its own; the text is a C string (up to its first zero).
//!
//! `LoadNPCDialogueScript` (QuestMng.cpp:37-66): 88-byte records (`DWORD` index +
//! `SNPCDialogue`, QuestMng.h:12-16), each through `BuxConvert`. The index is
//! `NPC number × 0x10000 + dialogue state`; `m_nNPCWords` is a text index and `m_anAnswer` holds
//! `QM_MAX_ND_ANSWER` = 10 pairs (answer text, result): a result up to 900 is the next state, above
//! it an action (901 quest list, 902 buff, 903/904 join a gens, 905 leave it, 906/907 gens reward).
//!
//! Both maps are `std::map::insert`: the first record of an index wins.

use std::collections::HashMap;
use std::path::Path;

use crate::{Error, Result, fs};

/// `QM_MAX_ND_ANSWER` (QuestMng.h:8).
pub const MAX_ND_ANSWER: usize = 10;
/// `sizeof(DWORD) + sizeof(SNPCDialogue)`.
const DIALOGUE_RECORD: usize = 4 + 4 + MAX_ND_ANSWER * 2 * 4;

/// `SNPCDialogue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NpcDialogue {
    /// `m_nNPCWords`: the text the NPC says.
    pub npc_words: i32,
    /// `m_anAnswer`: (answer text, result) pairs; a text of 0 ends the list.
    pub answers: [(i32, i32); MAX_ND_ANSWER],
}

/// `BuxConvert` (XOR `FC CF AB`, restarting for each part).
fn bux(buf: &[u8]) -> Vec<u8> {
    const BUX: [u8; 3] = [0xFC, 0xCF, 0xAB];
    buf.iter().enumerate().map(|(i, &b)| b ^ BUX[i % 3]).collect()
}

/// A C string in Windows-1252 (the English client's code page).
fn c_string(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    b[..end].iter().map(|&c| crate::minimap::cp1252(c)).collect()
}

/// Reads a `QuestWords_<lang>.bmd`.
pub fn read_words(buf: &[u8]) -> Result<HashMap<i32, String>> {
    let mut out = HashMap::new();
    let mut p = 0;
    while p + 6 <= buf.len() {
        let h = bux(&buf[p..p + 6]);
        let index = i32::from_le_bytes([h[0], h[1], h[2], h[3]]);
        let len = i16::from_le_bytes([h[4], h[5]]);
        p += 6;
        if len < 0 || p + len as usize > buf.len() {
            return Err(Error::Format(format!("QuestWords: texto {index} de {len} bytes cortado")));
        }
        let text = c_string(&bux(&buf[p..p + len as usize]));
        p += len as usize;
        out.entry(index).or_insert(text);
    }
    Ok(out)
}

/// Reads `NPCDialogue.bmd`. A trailing incomplete record is ignored (`fread` returns 0).
pub fn read_dialogues(buf: &[u8]) -> HashMap<u32, NpcDialogue> {
    let mut out = HashMap::new();
    for rec in buf.chunks_exact(DIALOGUE_RECORD) {
        let d = bux(rec);
        let int = |o: usize| i32::from_le_bytes([d[o], d[o + 1], d[o + 2], d[o + 3]]);
        let mut answers = [(0, 0); MAX_ND_ANSWER];
        for (i, a) in answers.iter_mut().enumerate() {
            *a = (int(8 + i * 8), int(12 + i * 8));
        }
        out.entry(int(0) as u32).or_insert(NpcDialogue { npc_words: int(4), answers });
    }
    out
}

/// `CQuestMng`'s scripts.
#[derive(Debug, Default, Clone)]
pub struct QuestScripts {
    pub words: HashMap<i32, String>,
    pub dialogues: HashMap<u32, NpcDialogue>,
}

impl QuestScripts {
    /// `LoadNPCDialogueScript` and `LoadQuestWordsScript` (QuestMng.cpp:12-15): `lang` is
    /// `g_strSelectedML` ("Eng").
    pub fn load(data: &Path, lang: &str) -> Result<Self> {
        let local = fs::find(data, "Local").ok_or_else(|| Error::NotFound(data.join("Local")))?;
        let dialogues = read_dialogues(&fs::read(&local, "NPCDialogue.bmd")?);
        let lang_dir = fs::find(&local, lang).ok_or_else(|| Error::NotFound(local.join(lang)))?;
        let words = read_words(&fs::read(&lang_dir, &format!("QuestWords_{}.bmd", lang.to_ascii_lowercase()))?)?;
        Ok(Self { words, dialogues })
    }

    /// `GetWords` (QuestMng.cpp:291-298).
    pub fn word(&self, index: i32) -> Option<&str> {
        self.words.get(&index).map(String::as_str)
    }

    fn dialogue(&self, npc: u16, state: u32) -> Option<&NpcDialogue> {
        // `(DWORD)m_nNPCIndex * 0x10000 + dwDlgState`.
        self.dialogues.get(&((npc as u32).wrapping_mul(0x10000).wrapping_add(state)))
    }

    /// `GetNPCDlgNPCWords` (QuestMng.cpp:300-312): nothing without an NPC (`m_nNPCIndex` 0).
    pub fn npc_words(&self, npc: u16, state: u32) -> Option<&str> {
        if npc == 0 {
            return None;
        }
        self.word(self.dialogue(npc, state)?.npc_words)
    }

    /// `GetNPCDlgAnswer` (QuestMng.cpp:314-332).
    pub fn answer(&self, npc: u16, state: u32, answer: usize) -> Option<&str> {
        if npc == 0 {
            return None;
        }
        let text = self.dialogue(npc, state)?.answers.get(answer)?.0;
        if text == 0 { None } else { self.word(text) }
    }

    /// `GetNPCDlgAnswerResult` (QuestMng.cpp:334-352): `NULL` (0) without an NPC, -1 without the
    /// dialogue or the answer.
    pub fn answer_result(&self, npc: u16, state: u32, answer: usize) -> i32 {
        if npc == 0 {
            return 0;
        }
        let Some(&(text, result)) = self.dialogue(npc, state).and_then(|d| d.answers.get(answer)) else { return -1 };
        if text == 0 { -1 } else { result }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn data() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../dist/cliente/Data")
    }

    fn word_record(index: i32, text: &str) -> Vec<u8> {
        let mut t = text.as_bytes().to_vec();
        t.push(0);
        let mut h = index.to_le_bytes().to_vec();
        h.extend((t.len() as i16).to_le_bytes());
        let mut out = bux(&h);
        out.extend(bux(&t));
        out
    }

    #[test]
    fn synthetic_scripts() {
        let mut words = word_record(1007, "Go back.");
        words.extend(word_record(1000, "Hello"));
        words.extend(word_record(1000, "Second one is ignored"));
        let words = read_words(&words).unwrap();
        assert_eq!(words.get(&1007).map(String::as_str), Some("Go back."));
        assert_eq!(words.get(&1000).map(String::as_str), Some("Hello"), "the first record wins");
        assert!(read_words(&word_record(5, "cut")[..8]).is_err());

        let mut rec = vec![0u8; DIALOGUE_RECORD];
        rec[0..4].copy_from_slice(&(543u32 * 0x10000 + 3).to_le_bytes());
        rec[4..8].copy_from_slice(&1000i32.to_le_bytes());
        rec[8..12].copy_from_slice(&1007i32.to_le_bytes());
        rec[12..16].copy_from_slice(&903i32.to_le_bytes());
        let mut buf = bux(&rec);
        buf.extend([1, 2, 3]);
        let s = QuestScripts { words, dialogues: read_dialogues(&buf) };
        assert_eq!(s.npc_words(543, 3), Some("Hello"));
        assert_eq!(s.npc_words(0, 3), None);
        assert_eq!(s.answer(543, 3, 0), Some("Go back."));
        assert_eq!(s.answer(543, 3, 1), None);
        assert_eq!(s.answer_result(543, 3, 0), 903);
        assert_eq!(s.answer_result(543, 3, 1), -1, "no answer");
        assert_eq!(s.answer_result(543, 4, 0), -1, "no dialogue");
        assert_eq!(s.answer_result(0, 3, 0), 0, "no NPC");
    }

    /// The English client's scripts: the Gens Duprian steward and the Elf Soldier.
    #[test]
    fn real_scripts() {
        let data = data();
        if !data.exists() {
            return;
        }
        let s = QuestScripts::load(&data, "Eng").expect("NPCDialogue.bmd and QuestWords_eng.bmd");
        assert_eq!(s.dialogues.len(), 58);
        assert_eq!(s.npc_words(543, 0), Some("How do you do? I am Gregory, the Gens Duprian Steward. What is your business with our gens?"));
        assert_eq!(s.answer(543, 0, 0), Some("Accept a gens quest."));
        assert_eq!(s.answer_result(543, 0, 0), 901);
        assert_eq!(s.answer_result(543, 0, 5), 906);
        assert_eq!(s.answer_result(543, 3, 0), 903);
        assert_eq!(s.answer_result(544, 3, 0), 904);
        assert_eq!(s.answer_result(543, 4, 0), 905);
        assert_eq!(s.answer_result(257, 0, 0), 902);
        assert_eq!(s.answer(257, 0, 1), Some("Accept a quest."));
        assert!(s.npc_words(257, 0).is_some_and(|w| w.contains(';')), "';' splits the paragraphs");
        assert_eq!(s.word(1501), Some("Select a quest."));
        assert_eq!(s.word(1502), Some("There is no available quest. Please come back later."));
        assert_eq!(s.npc_words(543, 999), Some("NPC dialog script error!"));
    }
}
