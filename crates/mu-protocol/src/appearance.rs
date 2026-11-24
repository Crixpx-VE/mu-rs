//! The 18 appearance bytes of a character (`CharacterList`, `AddCharactersToScope`): its class, its
//! pose and the equipment others can see, plus the single-slot change `AppearanceChanged` (C1 25).
//!
//! # Decoding (the client)
//!
//! Byte 0 is `Class` (`class << 3 | pose`, WSclient.cpp:2162-2191) and the other 17 are the
//! `Equipment[EQUIPMENT_LENGTH]` that `ChangeCharacterExt` reads (ZzzCharacter.cpp:12086-12473;
//! `Equipment[k]` here is byte `k + 1`, WSclient.h:355-364 and 537-543):
//!
//! ```text
//! byte  1        right-hand weapon index (low 8 bits)          (Equipment[0])
//! byte  2        left-hand weapon / shield index               (Equipment[1])
//! byte  3        helm index (high nibble) | armor (low)        (Equipment[2])
//! byte  4        pants (high) | gloves (low)                   (Equipment[3])
//! byte  5        boots (high) | wing tier (bits 2-3) | pet (bits 0-1)
//! bytes 6..8     glow level of the 7 pieces, 3 bits each (right weapon in the lowest bits)
//! byte  9        bit 4 of helm..boots (0x80..0x08) | wing within the tier (bits 0-2)
//! byte 10        excellent helm..boots (0x80..0x08), right/left weapon (0x04/0x02) | Dinorant (0x01)
//! byte 11        ancient helm..boots, right/left weapon (same bits) | full ancient set (0x01)
//! byte 12        right weapon kind bits 8-11 (high nibble) | Fenrir 0x04 | Dark Horse 0x01
//! byte 13        left weapon kind bits 8-11 (high) | helm index bits 5-8 (low)
//! byte 14        armor index bits 5-8 (high) | pants (low)
//! byte 15        gloves index bits 5-8 (high) | boots (low)
//! byte 16        pet (bits 5-7) | Fenrir black 0x01 / blue 0x02
//! byte 17        small wings (bits 5-7) | Fenrir gold 0x01
//! ```
//!
//! An empty weapon has kind 0xFFF and an empty armor piece index 0x1FF (all bits set).
//!
//! The wings follow the `PBG_ADD_NEWCHAR_MONK_ITEM` branch of `ChangeCharacterExt`
//! (ZzzCharacter.cpp:12143-12195): Main 5.2 compiles it out, but it is the layout OpenMU writes
//! (`AppearanceSerializer.AddWing`: tier 1/2/3 in byte 5 and the wing within the tier in byte 9)
//! and the one MuMain uses (`GetViewportWingModel`, MuMain ZzzCharacter.cpp:13316-13353). The
//! branch Main 5.2 does compile reads 2nd wings as `MODEL_WING + 2` and would show OpenMU's wings wrong.
//!
//! # Encoding (the server)
//!
//! [`Appearance::encode`] writes them like OpenMU's `AppearanceSerializer` (GameServer/RemoteView/
//! AppearanceSerializer.cs:112-422): glow level `(level - 1) / 2` (ItemExtensions.cs:28), excellent
//! and ancient bits only for the five armor pieces (`SetHand` writes neither), empty pieces with all
//! their index bits set and "no pet" as `byte 5 |= 3`.

/// Visible slots, the same numbers as the inventory equipment slots and as the high nibble of
/// `Item[1]` in `AppearanceChanged` (`ReceiveChangePlayer`, WSclient.cpp:1885-2083).
pub mod slot {
    pub const WEAPON_RIGHT: usize = 0;
    pub const WEAPON_LEFT: usize = 1;
    pub const HELM: usize = 2;
    pub const ARMOR: usize = 3;
    pub const PANTS: usize = 4;
    pub const GLOVES: usize = 5;
    pub const BOOTS: usize = 6;
    pub const WING: usize = 7;
    pub const HELPER: usize = 8;
    pub const COUNT: usize = 9;
}

/// Item groups used here (`ITEM_*` / `MAX_ITEM_INDEX` = 512 per group).
const GROUP_HELM: u8 = 7;
const GROUP_WING: u8 = 12;
const GROUP_HELPER: u8 = 13;
const MAX_ITEM_INDEX: u16 = 512;

/// Fenrir (`ITEM_HELPER + 37`) and the kinds in its `Option1` (ZzzCharacter.cpp:12326-12368).
pub const FENRIR: u16 = 37;
pub mod fenrir {
    pub const RED: u8 = 0;
    pub const BLACK: u8 = 1;
    pub const BLUE: u8 = 2;
    pub const GOLD: u8 = 4;
}

/// A visible piece of equipment: what `ChangeCharacterExt` / `ReceiveChangePlayer` leave in the
/// character's `PART_t` (`Type`, `Level`, `Option1`, `ExtOption`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct VisibleItem {
    pub group: u8,
    pub index: u16,
    /// Item level used for the glow: `LevelConvert` of the 3-bit glow level (0, 3, 5, 7 ... 15).
    /// Always 0 for wings and pets (`Wing.Level = 0`, `Helper.Level = 0`).
    pub level: u8,
    /// `Option1`: from the appearance only "excellent or not" (0/1); from `AppearanceChanged` the
    /// excellent bits (`Item[3] & 63`). For Fenrir, its kind (see [`fenrir`]).
    pub excellent: u8,
    /// `ExtOption`: from the appearance 1 = ancient set item; from `AppearanceChanged` `Item[4]`.
    pub ancient: u8,
}

impl VisibleItem {
    pub fn new(group: u8, index: u16) -> Self {
        Self { group, index, ..Default::default() }
    }

    /// Original item type (`group * MAX_ITEM_INDEX + index`).
    pub fn kind(&self) -> u16 {
        self.group as u16 * MAX_ITEM_INDEX + self.index
    }

    fn from_kind(kind: u16) -> Self {
        Self::new((kind / MAX_ITEM_INDEX) as u8, kind % MAX_ITEM_INDEX)
    }
}

/// A character's look, decoded from its 18 appearance bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Appearance {
    /// `CharacterClassNumber` (byte 0 `>> 3`).
    pub class: u8,
    /// `CharacterPose` (byte 0 `& 7`): 1 just teleported, 2 sitting, 3 leaning, 4 healing.
    pub pose: u8,
    /// The visible equipment by [`slot`].
    pub equipment: [Option<VisibleItem>; slot::COUNT],
    /// `ExtendState` (`Equipment[10] & 0x01`, ZzzCharacter.cpp:12469): a full ancient set is worn.
    pub full_ancient_set: bool,
}

/// `LevelConvert` (ZzzCharacter.cpp:12050-12064): the 3-bit glow level → item level.
pub fn level_convert(glow: u8) -> u8 {
    match glow {
        1..=7 => glow * 2 + 1,
        _ => 0,
    }
}

/// OpenMU's `GetGlowLevel` (ItemExtensions.cs:28): `(level - 1) / 2` (C# division, so 0 stays 0).
pub fn glow_level(level: u8) -> u8 {
    (level.saturating_sub(1) / 2).min(7)
}

/// The five armor pieces: (slot, item group, index nibble byte and whether it is the high nibble,
/// mask of bit 4 in byte 9 (also the excellent/ancient bit of bytes 10/11), byte of index bits
/// 5-8 and whether high nibble), ZzzCharacter.cpp:12396-12462 and AppearanceSerializer.cs:130-138.
const ARMOR_PIECES: [(usize, u8, usize, bool, u8, usize, bool); 5] = [
    (slot::HELM, GROUP_HELM, 3, true, 0x80, 13, false),
    (slot::ARMOR, GROUP_HELM + 1, 3, false, 0x40, 14, true),
    (slot::PANTS, GROUP_HELM + 2, 4, true, 0x20, 14, false),
    (slot::GLOVES, GROUP_HELM + 3, 4, false, 0x10, 15, true),
    (slot::BOOTS, GROUP_HELM + 4, 5, true, 0x08, 15, false),
];

/// The two hands: (slot, index byte, byte with kind bits 8-11 in its high nibble, mask of the
/// excellent/ancient bit), ZzzCharacter.cpp:12102-12141 and 12464-12467.
const HANDS: [(usize, usize, usize, u8); 2] = [(slot::WEAPON_RIGHT, 1, 12, 0x04), (slot::WEAPON_LEFT, 2, 13, 0x02)];

/// `ChangeCharacterExt`'s pets from byte 16 bits 5-7 (ZzzCharacter.cpp:12275-12290): value → index.
const PETS: [(u8, u16); 6] = [(32, 64), (64, 65), (128, 67), (224, 80), (160, 106), (96, 123)];

/// OpenMU's pets in byte 16 (`AddPet`, AppearanceSerializer.cs:398-421): index → value. Rudolph
/// (66) is written as 0x80, which the client reads as `ITEM_HELPER + 67`.
const OPENMU_PETS: [(u16, u8); 6] = [(80, 0xE0), (106, 0xA0), (123, 0x60), (66, 0x80), (65, 0x40), (64, 0x20)];

fn nibble(b: u8, high: bool) -> u8 {
    if high { b >> 4 } else { b & 0x0F }
}

fn set_nibble(b: &mut u8, high: bool, v: u8) {
    *b |= if high { (v << 4) & 0xF0 } else { v & 0x0F };
}

impl Appearance {
    /// A character of class `class` with nothing visible.
    pub fn new(class: u8) -> Self {
        Self { class, ..Default::default() }
    }

    /// `ChangeCharacterExt` (ZzzCharacter.cpp:12086-12473) on the 18 bytes.
    pub fn decode(a: &[u8; 18]) -> Self {
        let mut out = Self { class: a[0] >> 3, pose: a[0] & 0x07, ..Default::default() };
        // `Level = Equipment[5] << 16 | Equipment[6] << 8 | Equipment[7]`; piece k at bits 3k..3k+2.
        let levels = (a[6] as u32) << 16 | (a[7] as u32) << 8 | a[8] as u32;
        let level = |k: usize| level_convert(((levels >> (3 * k)) & 7) as u8);
        let flags = |mask: u8| ((a[10] & mask != 0) as u8, (a[11] & mask != 0) as u8);

        for (s, index_byte, kind_byte, mask) in HANDS {
            // `ExtType = (Equipment[11] & 240) << 4 | Equipment[0]`; 0x0FFF = nothing.
            let kind = ((a[kind_byte] & 0xF0) as u16) << 4 | a[index_byte] as u16;
            if kind != 0x0FFF {
                let (excellent, ancient) = flags(mask);
                out.equipment[s] = Some(VisibleItem { level: level(s), excellent, ancient, ..VisibleItem::from_kind(kind) });
            }
        }

        for (s, group, nib, nib_high, bit, ext, ext_high) in ARMOR_PIECES {
            // `ExtType = nibble + bit · 16 + nibble · 32`; 0x1FF = the class's own piece.
            let index = nibble(a[nib], nib_high) as u16 + (a[9] & bit != 0) as u16 * 16 + nibble(a[ext], ext_high) as u16 * 32;
            if index != 0x1FF {
                let (excellent, ancient) = flags(bit);
                out.equipment[s] = Some(VisibleItem { group, index, level: level(s), excellent, ancient });
            }
        }

        // Wings: the tier (`(Equipment[4] >> 2) & 3`) and the wing within it (`Equipment[8] & 7`).
        let code = (a[9] & 0x07) as u16;
        let wing = match (a[5] >> 2) & 3 {
            // `MODEL_WING + Type - 1` (wing 0 does not exist: Type 0 gives no model).
            1 if code == 4 => Some((GROUP_WING, 41)),
            1 => code.checked_sub(1).map(|i| (GROUP_WING, i)),
            2 => Some(match code {
                5 => (GROUP_HELPER, 30),
                6 => (GROUP_WING, 42),
                7 => (GROUP_WING, 49),
                _ => (GROUP_WING, 2 + code),
            }),
            3 => Some(match code {
                // The small wings: `MODEL_WING + 129 + (Equipment[16] >> 5)`.
                0 => (GROUP_WING, 129 + (a[17] >> 5) as u16),
                6 => (GROUP_WING, 43),
                7 => (GROUP_WING, 50),
                _ => (GROUP_WING, 35 + code),
            }),
            _ => None,
        };
        out.equipment[slot::WING] = wing.map(|(g, i)| VisibleItem::new(g, i));

        // Pets and mounts (ZzzCharacter.cpp:12254-12368), in the same order: later ones win.
        let pet = a[5] & 3;
        let mut helper = if pet == 3 {
            // Dinorant: `Equipment[9] & 0x01`.
            (a[10] & 0x01 != 0).then_some(3)
        } else {
            let high = a[16] & 0xE0;
            Some(PETS.iter().find(|&&(v, _)| v == high).map_or(pet as u16, |&(_, i)| i))
        };
        let mut fenrir_kind = 0;
        if a[12] & 0x01 != 0 {
            // Dark Horse.
            helper = Some(4);
        }
        if a[12] & 0x04 != 0 {
            helper = Some(FENRIR);
            fenrir_kind = if a[17] & 0x01 != 0 { fenrir::GOLD } else { a[16] & 3 };
        }
        out.equipment[slot::HELPER] = helper.map(|i| VisibleItem { excellent: fenrir_kind, ..VisibleItem::new(GROUP_HELPER, i) });

        out.full_ancient_set = a[11] & 0x01 != 0;
        out
    }

    /// The 18 bytes as OpenMU's `AppearanceSerializer.WritePreviewCharSet` writes them.
    pub fn encode(&self) -> [u8; 18] {
        let mut a = [0u8; 18];
        a[0] = ((self.class << 3) & 0xF8) | (self.pose & 0x07);
        let e = &self.equipment;

        for (s, index_byte, kind_byte, _) in HANDS {
            match e[s] {
                None => {
                    a[index_byte] = 0xFF;
                    a[kind_byte] |= 0xF0;
                }
                // OpenMU writes `Group << 5`, the same as kind bits 8-11 for indices below 256.
                Some(it) => {
                    a[index_byte] = it.kind() as u8;
                    a[kind_byte] |= ((it.kind() >> 8) << 4) as u8;
                }
            }
        }

        for (s, _, nib, nib_high, bit, ext, ext_high) in ARMOR_PIECES {
            match e[s] {
                None => {
                    set_nibble(&mut a[nib], nib_high, 0x0F);
                    a[9] |= bit;
                    set_nibble(&mut a[ext], ext_high, 0x0F);
                }
                Some(it) => {
                    set_nibble(&mut a[nib], nib_high, it.index as u8);
                    let multi = it.index / 16;
                    if multi % 2 == 1 {
                        a[9] |= bit;
                    }
                    set_nibble(&mut a[ext], ext_high, (multi / 2) as u8);
                    if it.excellent != 0 {
                        a[10] |= bit;
                    }
                    if it.ancient != 0 {
                        a[11] |= bit;
                    }
                }
            }
        }

        // `SetItemLevels`: the first 7 slots, 3 bits each.
        let levels = (0..7).filter_map(|k| e[k].map(|it| (glow_level(it.level) as u32) << (3 * k))).fold(0, |l, v| l | v);
        a[6] = (levels >> 16) as u8;
        a[7] = (levels >> 8) as u8;
        a[8] = levels as u8;

        if self.full_ancient_set {
            a[11] |= 0x01;
        }

        // `AddWing`: OpenMU switches on the item number only (the Cape of Lord is a helper).
        if let Some(w) = e[slot::WING].filter(|w| w.group == GROUP_WING || (w.group, w.index) == (GROUP_HELPER, 30)) {
            let (tier, code, small) = match w.index {
                0 => (1, 1, 0),
                1 => (1, 2, 0),
                2 => (1, 3, 0),
                41 => (1, 4, 0),
                3 => (2, 1, 0),
                4 => (2, 2, 0),
                5 => (2, 3, 0),
                6 => (2, 4, 0),
                30 => (2, 5, 0),
                42 => (2, 6, 0),
                49 => (2, 7, 0),
                36 => (3, 1, 0),
                37 => (3, 2, 0),
                38 => (3, 3, 0),
                39 => (3, 4, 0),
                40 => (3, 5, 0),
                43 => (3, 6, 0),
                50 => (3, 7, 0),
                130..=135 => (3, 0, (w.index - 129) as u8),
                _ => (0, 0, 0),
            };
            a[5] |= tier << 2;
            a[9] |= code;
            a[17] |= small << 5;
        }

        // `AddPet`.
        match e[slot::HELPER].filter(|h| h.group == GROUP_HELPER) {
            None => a[5] |= 0x03,
            Some(h) => {
                match h.index {
                    0..=2 => a[5] |= h.index as u8,
                    3 => {
                        a[5] |= 0x03;
                        a[10] |= 0x01;
                    }
                    4 => {
                        a[5] |= 0x03;
                        a[12] |= 0x01;
                    }
                    FENRIR => {
                        a[5] |= 0x03;
                        a[10] &= 0xFE;
                        a[12] &= 0xFE;
                        a[12] |= 0x04;
                        a[16] = 0;
                        if h.excellent & fenrir::BLACK != 0 {
                            a[16] |= 0x01;
                        }
                        if h.excellent & fenrir::BLUE != 0 {
                            a[16] |= 0x02;
                        }
                        if h.excellent & fenrir::GOLD != 0 {
                            a[17] |= 0x01;
                        }
                    }
                    _ => a[5] |= 0x03,
                }
                if let Some(&(_, v)) = OPENMU_PETS.iter().find(|&&(i, _)| i == h.index) {
                    a[16] |= v;
                }
            }
        }
        a
    }

    /// `ReceiveChangePlayer` (WSclient.cpp:1871-2088): slot `slot` now shows `item` (`None` =
    /// nothing; slots without a `case`, the rings and the pendant, change nothing). What the original
    /// does not write keeps its previous value: the weapons' `ExtOption`, the wing's `Option1` and
    /// `ExtOption` and every pet field except Fenrir's `Option1`.
    pub fn apply_change(&mut self, slot: usize, item: Option<VisibleItem>) {
        let Some(old) = self.equipment.get(slot).copied() else { return };
        let keep = old.unwrap_or_default();
        self.equipment[slot] = item.map(|it| match slot {
            slot::WEAPON_RIGHT | slot::WEAPON_LEFT => VisibleItem { ancient: keep.ancient, ..it },
            slot::WING => VisibleItem { level: 0, excellent: keep.excellent, ancient: keep.ancient, ..it },
            slot::HELPER => VisibleItem {
                level: 0,
                excellent: if it.index == FENRIR && it.group == GROUP_HELPER { it.excellent } else { keep.excellent },
                ancient: keep.ancient,
                ..it
            },
            _ => it,
        });
    }
}

/// `AppearanceChanged` item bytes (`PCHANGE_CHARACTER::Item`, WSclient.h:653-658): the S6 item
/// with the slot and the glow level in `Item[1]` (OpenMU AppearanceChangedPlugIn.cs:49-64).
/// Returns (slot, item); the item as `ReceiveChangePlayer` reads it: `ConvertItemType` (0x1FFF =
/// removed), `LevelConvert(Item[1] & 0xF)`, `Item[3] & 63` and `Item[4]`.
pub fn decode_changed_item(item: &[u8; 12]) -> (usize, Option<VisibleItem>) {
    let slot = (item[1] >> 4) as usize;
    // `ConvertItemType` (ZzzInfomation.cpp:379-384).
    let kind = item[0] as u16 + (item[3] as u16 & 128) * 2 + (item[5] as u16 & 240) * 32;
    let visible = (kind != 0x1FFF).then(|| VisibleItem {
        level: level_convert(item[1] & 0x0F),
        excellent: item[3] & 63,
        ancient: item[4],
        ..VisibleItem::from_kind(kind)
    });
    (slot, visible)
}

/// Inverse of [`decode_changed_item`] as OpenMU's `ItemSerializer` + `AppearanceChangedPlugIn` write it
/// (without what the client does not read: durability, options, harmony, sockets are empty).
pub fn encode_changed_item(slot: usize, item: Option<VisibleItem>) -> [u8; 12] {
    let mut b = [0xFF; 12];
    if let Some(it) = item {
        b[..7].fill(0);
        b[0] = it.index as u8;
        b[3] = (it.excellent & 63) | (((it.index >> 8) & 1) as u8) << 7;
        b[4] = it.ancient;
        b[5] = (it.group & 0x0F) << 4;
    }
    b[1] = (slot as u8) << 4 | item.map_or(0, |it| glow_level(it.level));
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(group: u8, index: u16, level: u8, excellent: u8, ancient: u8) -> Option<VisibleItem> {
        Some(VisibleItem { group, index, level, excellent, ancient })
    }

    /// A Blade Knight as OpenMU's `WritePreviewCharSet` writes it, computed by hand: Lightning Sword
    /// (0/14) +9, a shield (6/9) +0, excellent helm 17 +11, ancient armor 40 +13, excellent and
    /// ancient pants 51 +15, no gloves, boots 5 +3, Wings of Dragon, gold Fenrir, full set.
    const OPENMU_BLADE_KNIGHT: [u8; 18] =
        [0x30, 0x0E, 0x09, 0x18, 0x3F, 0x5B, 0x04, 0x7D, 0x44, 0xB3, 0xA0, 0x61, 0x04, 0xC0, 0x11, 0xF0, 0x00, 0x01];

    fn blade_knight() -> Appearance {
        let mut a = Appearance::new(6);
        a.equipment[slot::WEAPON_RIGHT] = item(0, 14, 9, 0, 0);
        a.equipment[slot::WEAPON_LEFT] = item(6, 9, 0, 0, 0);
        a.equipment[slot::HELM] = item(7, 17, 11, 1, 0);
        a.equipment[slot::ARMOR] = item(8, 40, 13, 0, 1);
        a.equipment[slot::PANTS] = item(9, 51, 15, 1, 1);
        a.equipment[slot::BOOTS] = item(11, 5, 3, 0, 0);
        a.equipment[slot::WING] = item(12, 5, 0, 0, 0);
        a.equipment[slot::HELPER] = item(13, FENRIR, 0, fenrir::GOLD, 0);
        a.full_ancient_set = true;
        a
    }

    #[test]
    fn decodifica_lo_que_escribe_openmu() {
        assert_eq!(Appearance::decode(&OPENMU_BLADE_KNIGHT), blade_knight());
        assert_eq!(blade_knight().encode(), OPENMU_BLADE_KNIGHT);
    }

    #[test]
    fn sin_equipo() {
        for class in [0, 4, 8, 17, 20, 25] {
            let a = Appearance::new(class);
            assert_eq!(Appearance::decode(&a.encode()), a);
        }
        // `SetEmptyArmor` and `SetHand(null)`: all index bits set, and "no pet".
        assert_eq!(
            Appearance::new(4).encode(),
            [0x20, 0xFF, 0xFF, 0xFF, 0xFF, 0xF3, 0, 0, 0, 0xF8, 0, 0, 0xF0, 0xFF, 0xFF, 0xFF, 0, 0]
        );
    }

    #[test]
    fn pose_y_clase() {
        let a = Appearance { pose: 2, ..Appearance::new(23) };
        let b = a.encode();
        assert_eq!(b[0], (23 << 3) | 2);
        assert_eq!(Appearance::decode(&b), a);
    }

    #[test]
    fn nivel_de_brillo() {
        // `LevelConvert` and OpenMU's `(level - 1) / 2`.
        assert_eq!((0..8).map(level_convert).collect::<Vec<_>>(), [0, 3, 5, 7, 9, 11, 13, 15]);
        assert_eq!([0, 1, 2, 3, 4, 9, 13, 14, 15].map(glow_level), [0, 0, 0, 1, 1, 4, 6, 6, 7]);
    }

    #[test]
    fn todas_las_alas() {
        // Every wing OpenMU's `AddWing` knows comes back as the same item.
        let wings = [0, 1, 2, 41, 3, 4, 5, 6, 42, 49, 36, 37, 38, 39, 40, 43, 50, 130, 131, 132, 133, 134, 135];
        for index in wings {
            let mut a = Appearance::new(0);
            a.equipment[slot::WING] = item(12, index, 0, 0, 0);
            assert_eq!(Appearance::decode(&a.encode()), a, "ala {index}");
        }
        // The Cape of Lord is a helper item (`MODEL_HELPER + 30`).
        let mut a = Appearance::new(16);
        a.equipment[slot::WING] = item(13, 30, 0, 0, 0);
        let b = a.encode();
        assert_eq!((b[5] >> 2 & 3, b[9] & 7), (2, 5));
        assert_eq!(Appearance::decode(&b), a);
        // Small Wings of Heaven: tier 3, code 0 and byte 17 = 4 << 5.
        a.equipment[slot::WING] = item(12, 133, 0, 0, 0);
        let b = a.encode();
        assert_eq!((b[5] >> 2 & 3, b[9] & 7, b[17]), (3, 0, 0x80));
    }

    #[test]
    fn mascotas() {
        for (index, excellent) in [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (FENRIR, fenrir::RED), (FENRIR, fenrir::BLACK), (FENRIR, fenrir::BLUE)] {
            let mut a = Appearance::new(16);
            a.equipment[slot::HELPER] = item(13, index, 0, excellent, 0);
            assert_eq!(Appearance::decode(&a.encode()), a, "mascota {index}");
        }
        // OpenMU writes the newer pets with `byte 5 |= 3`, so `ChangeCharacterExt` (Type 3, no
        // Dinorant bit) shows none: only with bits 0-1 below 3 does it read byte 16.
        let mut a = Appearance::new(0);
        a.equipment[slot::HELPER] = item(13, 64, 0, 0, 0);
        let mut b = a.encode();
        assert_eq!(b[16], 0x20);
        assert_eq!(Appearance::decode(&b).equipment[slot::HELPER], None);
        b[5] &= !3;
        assert_eq!(Appearance::decode(&b).equipment[slot::HELPER], item(13, 64, 0, 0, 0));
        b[16] = 0x80;
        assert_eq!(Appearance::decode(&b).equipment[slot::HELPER], item(13, 67, 0, 0, 0));
    }

    #[test]
    fn indices_altos_de_armadura() {
        // Index 0..=510 of each piece goes and comes back (511 = empty).
        for index in [0, 15, 16, 31, 32, 63, 64, 100, 255, 300, 510] {
            let mut a = Appearance::new(4);
            for (s, group, ..) in ARMOR_PIECES {
                a.equipment[s] = item(group, index, 7, 0, 0);
            }
            assert_eq!(Appearance::decode(&a.encode()), a, "índice {index}");
        }
    }

    #[test]
    fn cambio_de_apariencia() {
        // OpenMU: Kris (0/5) +9 with excellent bit 0x02 and durability 42 in slot 0; `Item[1]` is
        // overwritten with `slot << 4 | glow level`.
        let kris = [0x05, 0x04, 0x2A, 0x02, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(decode_changed_item(&kris), (0, item(0, 5, 9, 2, 0)));
        // Wings taken off: all FF but `Item[1]`.
        let sin_alas = [0xFF, 0x70, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(decode_changed_item(&sin_alas), (7, None));
        assert_eq!(encode_changed_item(7, None), sin_alas);
        for (s, it) in [(0, item(0, 5, 9, 2, 0)), (2, item(7, 300, 15, 63, 5)), (8, item(13, 37, 0, 4, 0))] {
            assert_eq!(decode_changed_item(&encode_changed_item(s, it)), (s, it));
        }
    }

    #[test]
    fn aplicar_cambio() {
        let mut a = blade_knight();
        // A new right-hand weapon keeps the old `ExtOption` (`ReceiveChangePlayer` does not write it).
        a.equipment[slot::WEAPON_RIGHT].as_mut().unwrap().ancient = 1;
        a.apply_change(slot::WEAPON_RIGHT, item(0, 5, 9, 2, 0));
        assert_eq!(a.equipment[slot::WEAPON_RIGHT], item(0, 5, 9, 2, 1));
        // Armor pieces take everything.
        a.apply_change(slot::HELM, item(7, 1, 5, 0, 6));
        assert_eq!(a.equipment[slot::HELM], item(7, 1, 5, 0, 6));
        // Wings and pets are always level 0; Fenrir takes its kind.
        a.apply_change(slot::WING, item(12, 36, 13, 0, 0));
        assert_eq!(a.equipment[slot::WING], item(12, 36, 0, 0, 0));
        a.apply_change(slot::HELPER, item(13, FENRIR, 9, fenrir::BLUE, 0));
        assert_eq!(a.equipment[slot::HELPER], item(13, FENRIR, 0, fenrir::BLUE, 0));
        a.apply_change(slot::GLOVES, None);
        assert_eq!(a.equipment[slot::GLOVES], None);
        // A ring: nothing changes.
        let before = a;
        a.apply_change(10, item(13, 8, 0, 0, 0));
        assert_eq!(a, before);
    }
}
