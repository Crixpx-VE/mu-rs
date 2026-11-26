//! Combat and skill packets: the skill list, attacks and skills (the hero's requests and what the
//! server shows of everyone), hits, deaths, experience and the life/mana updates.
//!
//! Layouts from Main 5.2 (`wsclientinline.h` for what the client sends, `WSclient.h` /
//! `WSclient.cpp` for what it reads) checked against OpenMU's `ClientToServerPackets.xml` and
//! `ServerToClientPackets.xml` (Season 6 Ep3 variants) and against bytes captured from the live
//! OpenMU with the bot (`cargo run -p mu-server --example bot -- ... --atacar`).
//!
//! ```text
//! client → server  Hit (SendRequestAttack)      C1 07 11 <target u16 BE> <action AT_ATTACK1> <direction>
//! client → server  TargetedSkill (SendRequestMagic)
//!                                               C3 07 19 <skill u16 BE> <target u16 BE>
//! client → server  CancelSkill (SendRequestCancelMagic)
//!                                               C3 07 1B <skill u16 BE> <target u16 BE>
//! client → server  AreaSkill (SendRequestMagicContinue)
//!                                               C3 0D 1E <skill u16 BE> <x> <y> <angle> <dest> <tpos>
//!                                                        <target u16 BE> <serial>
//! client → server  AreaSkillHit (SendRequestMagicAttack)
//!                                               C3 len DB <skill u16 BE> <x> <y> <serial> <n>
//!                                                         n × [<target u16 BE> <skill serial>]
//! server → client  SkillList (ReceiveMagicList) C1 len F3 11 <n | FE add | FF remove> <list type>
//!                                                         n × [<slot> <skill u16 LE> <level>]
//! server → client  ObjectHit (ReceiveAttackDamage)
//!                                               C1 0A 11 <id u16 BE> <damage u16 BE> <damage type>
//!                                                        <shield damage u16 BE>
//! server → client  SkillAnimation (ReceiveMagic)
//!                                               C3 09 19 <skill u16 BE> <source u16 BE> <target u16 BE>
//! server → client  AreaSkillAnimation (ReceiveMagicContinue)
//!                                               C3 0A 1E <skill u16 BE> <source u16 BE> <x> <y> <angle>
//! server → client  SkillTargetsAnimation (ReceiveMagicPosition)
//!                                               C1 len 1A <source u16 BE> <skill u16 BE> <x> <y> <n>
//!                                                         n × <target u16 BE>
//! server → client  MagicEffectCancelled (ReceiveMagicFinish)
//!                                               C1 07 1B <skill u16 BE> <target u16 BE>
//! server → client  MagicEffectStatus (ReceiveSkillStatus)
//!                                               C1 07 07 <active> <id u16 BE> <effect>
//! server → client  ObjectKilled (ReceiveDie)    C1 09 17 <killed u16 BE> <skill u16 BE> <killer u16 BE>
//! server → client  ExperienceGained (ReceiveDieExp)
//!                                               C3 09 16 <killed u16 BE> <exp u16 BE> <damage u16 BE>
//! server → client  ExperienceGainedLarge (ReceiveDieExpLarge)
//!                                               C1 0C 9C <killed u16 BE> 00 <exp high u16 LE>
//!                                                        <exp low u16 LE> <damage u16 BE>
//! server → client  CurrentHealth / MaximumHealth (ReceiveLife)
//!                                               C1 09 26 <FF | FE> <life u16 BE> 00 <shield u16 BE>
//! server → client  CurrentMana / MaximumMana (ReceiveMana)
//!                                               C1 08 27 <FF | FE> <mana u16 BE> <AG u16 BE>
//! server → client  PoisonDamage (ReceiveDamage) C1 08 F3 07 <damage u16 BE> <shield u16 BE>
//! server → client  Respawn (ReceiveRevival)     C1 len F3 04 <x> <y> <map> <angle> <life u16 LE>
//!                                                        <mana u16 LE> <shield u16 LE> <AG u16 LE>
//!                                                        <exp u64 BE> <zen u32 LE>
//! server → client  LevelUp (ReceiveLevelUp)     C1 18 F3 05 <level> <points> <max life> <max mana>
//!                                                        <max shield> <max AG> ... (u16 LE)
//! server → client  MonsterSkillAnimation (ReceiveMonsterSkill)
//!                                               C1 0A 69 <skill u16 BE> 00 <attacker u16 LE>
//!                                                        <target u16 LE>
//! server → client  SkillStage (ReceiveSkillCount, MuMain WSclient.h:801-807)
//!                                               C1 07 BA <id u16 BE> <skill> <stage>
//! ```
//!
//! The high bit of the ids (`Key >> 15`) means something in some of them (the original's
//! `Success`: a hit on a player in Chaos Castle, a kill by the hero...): the ids travel as they are,
//! mask them with [`crate::game::OBJECT_ID_MASK`].

use crate::game::{ClientMessage, ServerMessage};
use crate::packet::{Packet, c1};

/// One entry of the hero's skill list (`PRECEIVE_MAGIC_LIST`, WSclient.h:829-835): the slot in
/// `CharacterAttribute->Skill[]`, the skill number (`AT_SKILL_*`, the index of `Skill.bmd`) and its level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkillSlot {
    pub index: u8,
    pub number: u16,
    pub level: u8,
}

/// `Value` of `PHEADER_MAGIC_LIST_COUNT` (`ReceiveMagicList`, WSclient.cpp:1170-1196): one skill
/// added or removed; any other value is the entry count of a list.
pub mod skill_list {
    pub const ADD: u8 = 0xFE;
    pub const REMOVE: u8 = 0xFF;
    /// `ListType` 0: the whole list (the client clears its skills first); 1 adds the entries to the
    /// list; 2 removes them (WSclient.cpp:1188-1207).
    pub const REPLACE: u8 = 0;
    pub const REMOVE_LISTED: u8 = 2;
}

/// The low 4 bits of `DamageType` of [`ServerMessage::ObjectHit`] (`ReceiveAttackDamage`,
/// WSclient.cpp:3007-3040; OpenMU's `DamageKind`). Bit 6 is "double damage" and bit 7 "combo"
/// (`bDoubleEnable`, `bComboEnable`); OpenMU uses bit 7 for "triple" too.
pub mod damage_type {
    pub const NORMAL: u8 = 0;
    /// `DT_PERFECT`: ignores defense.
    pub const PERFECT: u8 = 1;
    pub const EXCELLENT: u8 = 2;
    pub const CRITICAL: u8 = 3;
    /// `DT_MIRROR`: reflected.
    pub const MIRROR: u8 = 4;
    pub const POISON: u8 = 5;
    pub const DOT: u8 = 6;
    pub const DOUBLE: u8 = 0x40;
    pub const COMBO: u8 = 0x80;
}

fn be(p: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*p.get(i)?, *p.get(i + 1)?]))
}

fn le(p: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*p.get(i)?, *p.get(i + 1)?]))
}

fn le32(p: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes(p.get(i..i + 4)?.try_into().ok()?))
}

/// The packet bytes if it is a C1/C3 one.
fn short<'a>(p: &Packet<'a>) -> Option<&'a [u8]> {
    (p.header.size_field_len() == 1).then_some(p.bytes)
}

/// The same packet marked to go encrypted (C1 → C3).
fn encrypted(mut p: Vec<u8>) -> Vec<u8> {
    p[0] = 0xC3;
    p
}

/// A zeroed C1 packet of `len` bytes with its code (and subcode, if any).
fn zeroed(len: usize, code: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; len];
    p[0] = 0xC1;
    p[1] = len as u8;
    p[2..2 + code.len()].copy_from_slice(code);
    p
}

fn put_be(p: &mut [u8], i: usize, v: u16) {
    p[i..i + 2].copy_from_slice(&v.to_be_bytes());
}

fn put_le(p: &mut [u8], i: usize, v: u16) {
    p[i..i + 2].copy_from_slice(&v.to_le_bytes());
}

/// Encodes the combat messages of [`ClientMessage`] (`None` for the others).
pub(crate) fn encode_client(m: &ClientMessage) -> Option<Vec<u8>> {
    Some(match m {
        // `SendRequestAttack`: `spe.Send()`, not encrypted.
        ClientMessage::Hit { target, action, direction } => {
            let [h, l] = target.to_be_bytes();
            c1(&[0x11, h, l, *action, *direction])
        }
        // `SendRequestMagic`: `spe.Send(TRUE)`.
        ClientMessage::TargetedSkill { skill, target } => {
            let ([sh, sl], [th, tl]) = (skill.to_be_bytes(), target.to_be_bytes());
            encrypted(c1(&[0x19, sh, sl, th, tl]))
        }
        ClientMessage::CancelSkill { skill, target } => {
            let ([sh, sl], [th, tl]) = (skill.to_be_bytes(), target.to_be_bytes());
            encrypted(c1(&[0x1B, sh, sl, th, tl]))
        }
        ClientMessage::AreaSkill { skill, x, y, rotation, dest, target_position, target, serial } => {
            let ([sh, sl], [th, tl]) = (skill.to_be_bytes(), target.to_be_bytes());
            encrypted(c1(&[0x1E, sh, sl, *x, *y, *rotation, *dest, *target_position, th, tl, *serial]))
        }
        ClientMessage::AreaSkillHit { skill, x, y, serial, targets } => {
            let targets = &targets[..targets.len().min(MAX_AREA_TARGETS)];
            let [sh, sl] = skill.to_be_bytes();
            let mut body = vec![0xDB, sh, sl, *x, *y, *serial, targets.len() as u8];
            for (key, skill_serial) in targets {
                body.extend_from_slice(&key.to_be_bytes());
                body.push(*skill_serial);
            }
            encrypted(c1(&body))
        }
        _ => return None,
    })
}

/// Targets of one [`ClientMessage::AreaSkillHit`] so it fits in a C1 (9 + 82 × 3 = 255).
pub const MAX_AREA_TARGETS: usize = 82;

pub(crate) fn decode_client(p: &Packet<'_>) -> Option<ClientMessage> {
    let d = short(p)?;
    Some(match p.code() {
        0x11 => ClientMessage::Hit { target: be(d, 3)?, action: *d.get(5)?, direction: *d.get(6)? },
        0x19 => ClientMessage::TargetedSkill { skill: be(d, 3)?, target: be(d, 5)? },
        0x1B => ClientMessage::CancelSkill { skill: be(d, 3)?, target: be(d, 5)? },
        0x1E if d.len() >= 13 => ClientMessage::AreaSkill {
            skill: be(d, 3)?,
            x: d[5],
            y: d[6],
            rotation: d[7],
            dest: d[8],
            target_position: d[9],
            target: be(d, 10)?,
            serial: d[12],
        },
        0xDB => {
            let n = *d.get(8)? as usize;
            let targets = (0..n).map(|k| Some((be(d, 9 + 3 * k)?, *d.get(11 + 3 * k)?))).collect::<Option<Vec<_>>>()?;
            ClientMessage::AreaSkillHit { skill: be(d, 3)?, x: *d.get(5)?, y: *d.get(6)?, serial: *d.get(7)?, targets }
        }
        _ => return None,
    })
}

/// Encodes the combat messages of [`ServerMessage`] (`None` for the others).
pub(crate) fn encode_server(m: &ServerMessage) -> Option<Vec<u8>> {
    Some(match m {
        ServerMessage::SkillList { list_type, skills } => {
            let skills = &skills[..skills.len().min(0xFD)];
            let mut body = vec![0xF3, 0x11, skills.len() as u8, *list_type];
            for s in skills {
                write_slot(&mut body, s);
            }
            c1(&body)
        }
        ServerMessage::SkillAdded(s) => {
            let mut body = vec![0xF3, 0x11, skill_list::ADD, 0];
            write_slot(&mut body, s);
            c1(&body)
        }
        ServerMessage::SkillRemoved { index, number } => {
            let mut body = vec![0xF3, 0x11, skill_list::REMOVE, 0];
            write_slot(&mut body, &SkillSlot { index: *index, number: *number, level: 0 });
            c1(&body)
        }
        ServerMessage::ObjectHit { id, damage, damage_type, shield_damage } => {
            let mut p = zeroed(10, &[0x11]);
            put_be(&mut p, 3, *id);
            put_be(&mut p, 5, *damage);
            p[7] = *damage_type;
            put_be(&mut p, 8, *shield_damage);
            p
        }
        ServerMessage::SkillAnimation { skill, source, target } => {
            let mut p = zeroed(9, &[0x19]);
            put_be(&mut p, 3, *skill);
            put_be(&mut p, 5, *source);
            put_be(&mut p, 7, *target);
            encrypted(p)
        }
        ServerMessage::AreaSkillAnimation { skill, source, x, y, rotation } => {
            let mut p = zeroed(10, &[0x1E]);
            put_be(&mut p, 3, *skill);
            put_be(&mut p, 5, *source);
            p[7] = *x;
            p[8] = *y;
            p[9] = *rotation;
            encrypted(p)
        }
        ServerMessage::SkillTargetsAnimation { source, skill, x, y, targets } => {
            let targets = &targets[..targets.len().min(120)];
            let mut body = vec![0x1A];
            body.extend_from_slice(&source.to_be_bytes());
            body.extend_from_slice(&skill.to_be_bytes());
            body.extend_from_slice(&[*x, *y, targets.len() as u8]);
            for t in targets {
                body.extend_from_slice(&t.to_be_bytes());
            }
            c1(&body)
        }
        ServerMessage::MagicEffectCancelled { skill, target } => {
            let mut p = zeroed(7, &[0x1B]);
            put_be(&mut p, 3, *skill);
            put_be(&mut p, 5, *target);
            p
        }
        ServerMessage::MagicEffectStatus { active, id, effect } => {
            let mut p = zeroed(7, &[0x07]);
            p[3] = *active as u8;
            put_be(&mut p, 4, *id);
            p[6] = *effect;
            p
        }
        ServerMessage::ObjectKilled { killed, skill, killer } => {
            let mut p = zeroed(9, &[0x17]);
            put_be(&mut p, 3, *killed);
            put_be(&mut p, 5, *skill);
            put_be(&mut p, 7, *killer);
            p
        }
        ServerMessage::ExperienceGained { killed, experience, damage } => {
            let mut p = zeroed(9, &[0x16]);
            put_be(&mut p, 3, *killed);
            put_be(&mut p, 5, *experience);
            put_be(&mut p, 7, *damage);
            encrypted(p)
        }
        ServerMessage::ExperienceGainedLarge { killed, experience, damage } => {
            let mut p = zeroed(12, &[0x9C]);
            put_be(&mut p, 3, *killed);
            put_le(&mut p, 6, (experience >> 16) as u16);
            put_le(&mut p, 8, *experience as u16);
            put_be(&mut p, 10, *damage);
            p
        }
        ServerMessage::CurrentHealth { health, shield } => life(0x26, 0xFF, *health, *shield),
        ServerMessage::MaximumHealth { health, shield } => life(0x26, 0xFE, *health, *shield),
        ServerMessage::CurrentMana { mana, ability } => mana_packet(0xFF, *mana, *ability),
        ServerMessage::MaximumMana { mana, ability } => mana_packet(0xFE, *mana, *ability),
        ServerMessage::PoisonDamage { damage, shield } => {
            let mut p = zeroed(8, &[0xF3, 0x07]);
            put_be(&mut p, 4, *damage);
            put_be(&mut p, 6, *shield);
            p
        }
        ServerMessage::Respawn(r) => {
            let mut p = zeroed(28, &[0xF3, 0x04]);
            p[4] = r.x;
            p[5] = r.y;
            p[6] = r.map;
            p[7] = r.direction;
            put_le(&mut p, 8, r.health);
            put_le(&mut p, 10, r.mana);
            put_le(&mut p, 12, r.shield);
            put_le(&mut p, 14, r.ability);
            p[16..24].copy_from_slice(&r.experience.to_be_bytes());
            p[24..28].copy_from_slice(&r.money.to_le_bytes());
            p
        }
        ServerMessage::LevelUp(l) => {
            let mut p = zeroed(24, &[0xF3, 0x05]);
            for (k, v) in l.fields().into_iter().enumerate() {
                put_le(&mut p, 4 + 2 * k, v);
            }
            p
        }
        ServerMessage::MonsterSkillAnimation { skill, attacker, target } => {
            let mut p = zeroed(10, &[0x69]);
            put_be(&mut p, 3, *skill);
            put_le(&mut p, 6, *attacker);
            put_le(&mut p, 8, *target);
            p
        }
        ServerMessage::SkillStage { id, skill, stage } => {
            let mut p = zeroed(7, &[0xBA]);
            put_be(&mut p, 3, *id);
            p[5] = *skill;
            p[6] = *stage;
            p
        }
        _ => return None,
    })
}

fn write_slot(body: &mut Vec<u8>, s: &SkillSlot) {
    body.push(s.index);
    body.extend_from_slice(&s.number.to_le_bytes());
    body.push(s.level);
}

fn read_slot(d: &[u8], i: usize) -> Option<SkillSlot> {
    Some(SkillSlot { index: *d.get(i)?, number: le(d, i + 1)?, level: *d.get(i + 3)? })
}

/// `PRECEIVE_LIFE` (WSclient.h:876-880): `Index`, then `Life[0..1]` and `Life[3..4]` (OpenMU leaves
/// byte 6 zero).
fn life(code: u8, index: u8, health: u16, shield: u16) -> Vec<u8> {
    let mut p = zeroed(9, &[code, index]);
    put_be(&mut p, 4, health);
    put_be(&mut p, 7, shield);
    p
}

/// The same struct read by `ReceiveMana`: mana in `Life[0..1]` and AG in `Life[2..3]`.
fn mana_packet(index: u8, mana: u16, ability: u16) -> Vec<u8> {
    let mut p = zeroed(8, &[0x27, index]);
    put_be(&mut p, 4, mana);
    put_be(&mut p, 6, ability);
    p
}

pub(crate) fn decode_server(p: &Packet<'_>) -> Option<ServerMessage> {
    let d = short(p)?;
    let sub = d.get(3).copied();
    Some(match (p.code(), sub) {
        (0xF3, Some(0x11)) => {
            let value = *d.get(4)?;
            match value {
                skill_list::ADD => ServerMessage::SkillAdded(read_slot(d, 6)?),
                skill_list::REMOVE => {
                    let s = read_slot(d, 6)?;
                    ServerMessage::SkillRemoved { index: s.index, number: s.number }
                }
                n => ServerMessage::SkillList {
                    list_type: *d.get(5)?,
                    skills: (0..n as usize).map(|k| read_slot(d, 6 + 4 * k)).collect::<Option<Vec<_>>>()?,
                },
            }
        }
        (0x11, _) => ServerMessage::ObjectHit {
            id: be(d, 3)?,
            damage: be(d, 5)?,
            damage_type: *d.get(7)?,
            shield_damage: be(d, 8).unwrap_or(0),
        },
        (0x19, _) => ServerMessage::SkillAnimation { skill: be(d, 3)?, source: be(d, 5)?, target: be(d, 7)? },
        (0x1E, _) => ServerMessage::AreaSkillAnimation {
            skill: be(d, 3)?,
            source: be(d, 5)?,
            x: *d.get(7)?,
            y: *d.get(8)?,
            rotation: *d.get(9)?,
        },
        (0x1A, _) => {
            let n = *d.get(9)? as usize;
            ServerMessage::SkillTargetsAnimation {
                source: be(d, 3)?,
                skill: be(d, 5)?,
                x: *d.get(7)?,
                y: *d.get(8)?,
                targets: (0..n).map(|k| be(d, 10 + 2 * k)).collect::<Option<Vec<_>>>()?,
            }
        }
        (0x1B, _) => ServerMessage::MagicEffectCancelled { skill: be(d, 3)?, target: be(d, 5)? },
        (0x07, _) => ServerMessage::MagicEffectStatus { active: *d.get(3)? != 0, id: be(d, 4)?, effect: *d.get(6)? },
        (0x17, _) => ServerMessage::ObjectKilled { killed: be(d, 3)?, skill: be(d, 5)?, killer: be(d, 7)? },
        (0x16, _) => ServerMessage::ExperienceGained { killed: be(d, 3)?, experience: be(d, 5)?, damage: be(d, 7)? },
        (0x9C, _) => ServerMessage::ExperienceGainedLarge {
            killed: be(d, 3)?,
            experience: ((le(d, 6)? as u32) << 16) | le(d, 8)? as u32,
            damage: be(d, 10)?,
        },
        (0x26, Some(0xFF)) => ServerMessage::CurrentHealth { health: be(d, 4)?, shield: be(d, 7)? },
        (0x26, Some(0xFE)) => ServerMessage::MaximumHealth { health: be(d, 4)?, shield: be(d, 7)? },
        (0x27, Some(0xFF)) => ServerMessage::CurrentMana { mana: be(d, 4)?, ability: be(d, 6)? },
        (0x27, Some(0xFE)) => ServerMessage::MaximumMana { mana: be(d, 4)?, ability: be(d, 6)? },
        (0xF3, Some(0x07)) => ServerMessage::PoisonDamage { damage: be(d, 4)?, shield: be(d, 6)? },
        (0xF3, Some(0x04)) => ServerMessage::Respawn(Respawn {
            x: *d.get(4)?,
            y: *d.get(5)?,
            map: *d.get(6)?,
            direction: *d.get(7)?,
            health: le(d, 8)?,
            mana: le(d, 10)?,
            shield: le(d, 12)?,
            ability: le(d, 14)?,
            experience: u64::from_be_bytes(d.get(16..24)?.try_into().ok()?),
            money: le32(d, 24)?,
        }),
        (0xF3, Some(0x05)) => {
            let f = |k: usize| le(d, 4 + 2 * k);
            ServerMessage::LevelUp(LevelUp {
                level: f(0)?,
                points: f(1)?,
                max_health: f(2)?,
                max_mana: f(3)?,
                max_shield: f(4)?,
                max_ability: f(5)?,
                fruit_points: f(6).unwrap_or(0),
                max_fruit_points: f(7).unwrap_or(0),
                negative_fruit_points: f(8).unwrap_or(0),
                max_negative_fruit_points: f(9).unwrap_or(0),
            })
        }
        (0x69, _) => ServerMessage::MonsterSkillAnimation { skill: be(d, 3)?, attacker: le(d, 6)?, target: le(d, 8)? },
        (0xBA, _) => ServerMessage::SkillStage { id: be(d, 3)?, skill: *d.get(5)?, stage: *d.get(6)? },
        _ => return None,
    })
}

/// `PRECEIVE_REVIVAL` (WSclient.h:458-477, `ReceiveRevival`): where the hero comes back after dying
/// and its values. OpenMU's `RespawnAfterDeath` (28 bytes).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Respawn {
    pub x: u8,
    pub y: u8,
    pub map: u8,
    pub direction: u8,
    pub health: u16,
    pub mana: u16,
    pub shield: u16,
    pub ability: u16,
    pub experience: u64,
    pub money: u32,
}

/// `PRECEIVE_LEVEL_UP` (WSclient.h:860-874, `ReceiveLevelUp`). OpenMU's `CharacterLevelUpdate`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LevelUp {
    pub level: u16,
    pub points: u16,
    pub max_health: u16,
    pub max_mana: u16,
    pub max_shield: u16,
    pub max_ability: u16,
    pub fruit_points: u16,
    pub max_fruit_points: u16,
    pub negative_fruit_points: u16,
    pub max_negative_fruit_points: u16,
}

impl LevelUp {
    fn fields(&self) -> [u16; 10] {
        [
            self.level,
            self.points,
            self.max_health,
            self.max_mana,
            self.max_shield,
            self.max_ability,
            self.fruit_points,
            self.max_fruit_points,
            self.negative_fruit_points,
            self.max_negative_fruit_points,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client_round_trip(m: ClientMessage) -> Vec<u8> {
        let wire = m.encode();
        assert_eq!(ClientMessage::decode(&wire), Some(m.clone()), "round trip of {m:?}");
        wire
    }

    fn server_round_trip(m: ServerMessage) -> Vec<u8> {
        let wire = m.encode();
        assert_eq!(ServerMessage::decode(&wire), Some(m.clone()), "round trip of {m:?}");
        wire
    }

    /// `SendRequestAttack(Key, Dir)` (wsclientinline.h:483-492) and `SendRequestMagic`
    /// (wsclientinline.h:513-525): what Main 5.2 puts in the packet.
    #[test]
    fn hero_requests() {
        assert_eq!(
            client_round_trip(ClientMessage::Hit { target: 0x020F, action: crate::game::action::ATTACK1, direction: 3 }),
            [0xC1, 0x07, 0x11, 0x02, 0x0F, 0x78, 0x03]
        );
        assert_eq!(client_round_trip(ClientMessage::TargetedSkill { skill: 44, target: 0x020F }), [0xC3, 0x07, 0x19, 0x00, 0x2C, 0x02, 0x0F]);
        assert_eq!(client_round_trip(ClientMessage::CancelSkill { skill: 6, target: 0x0200 }), [0xC3, 0x07, 0x1B, 0x00, 0x06, 0x02, 0x00]);
        assert_eq!(
            client_round_trip(ClientMessage::AreaSkill {
                skill: 41,
                x: 182,
                y: 130,
                rotation: 64,
                dest: 0x88,
                target_position: 0,
                target: 0x020F,
                serial: 1
            }),
            [0xC3, 0x0D, 0x1E, 0x00, 0x29, 182, 130, 64, 0x88, 0x00, 0x02, 0x0F, 0x01]
        );
        assert_eq!(
            client_round_trip(ClientMessage::AreaSkillHit { skill: 41, x: 182, y: 130, serial: 2, targets: vec![(0x020F, 1), (0x0210, 1)] }),
            [0xC3, 0x0F, 0xDB, 0x00, 0x29, 182, 130, 0x02, 0x02, 0x02, 0x0F, 0x01, 0x02, 0x10, 0x01]
        );
    }

    /// Captured from the live OpenMU on entering with a fresh Dark Knight: its one skill.
    #[test]
    fn skill_list_from_openmu() {
        let wire = [0xC1, 0x0A, 0xF3, 0x11, 0x01, 0x00, 0x00, 0x2C, 0x00, 0x00];
        let m = ServerMessage::SkillList { list_type: 0, skills: vec![SkillSlot { index: 0, number: 44, level: 0 }] };
        assert_eq!(ServerMessage::decode(&wire), Some(m.clone()));
        assert_eq!(m.encode(), wire);
        let added = server_round_trip(ServerMessage::SkillAdded(SkillSlot { index: 1, number: 19, level: 0 }));
        assert_eq!(added, [0xC1, 0x0A, 0xF3, 0x11, 0xFE, 0x00, 0x01, 0x13, 0x00, 0x00]);
        server_round_trip(ServerMessage::SkillRemoved { index: 1, number: 19 });
    }

    /// A fresh Dark Knight fighting spiders in Lorencia against the live OpenMU (bot `--atacar`):
    /// every packet as it arrived, decrypted.
    #[test]
    fn fight_captured_from_openmu() {
        let cases: Vec<(&[u8], ServerMessage)> = vec![
            // Our hit on spider 610: 8 damage.
            (&[0xC1, 0x0A, 0x11, 0x02, 0x62, 0x00, 0x08, 0x00, 0x00, 0x00], ServerMessage::ObjectHit { id: 610, damage: 8, damage_type: 0, shield_damage: 0 }),
            // A spider's hit on the hero (512) that missed, and one of 2.
            (&[0xC1, 0x0A, 0x11, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], ServerMessage::ObjectHit { id: 512, damage: 0, damage_type: 0, shield_damage: 0 }),
            (&[0xC1, 0x0A, 0x11, 0x02, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00], ServerMessage::ObjectHit { id: 512, damage: 2, damage_type: 0, shield_damage: 0 }),
            // The hero's life after it.
            (&[0xC1, 0x09, 0x26, 0xFF, 0x00, 0x6C, 0x00, 0x00, 0x66], ServerMessage::CurrentHealth { health: 108, shield: 102 }),
            // Mana and AG recovering.
            (&[0xC1, 0x08, 0x27, 0xFF, 0x00, 0x14, 0x00, 0x0F], ServerMessage::CurrentMana { mana: 20, ability: 15 }),
            // Spider 579 killed by the hero, then the experience (the killed id with its high bit).
            (&[0xC1, 0x09, 0x17, 0x02, 0x43, 0x00, 0x00, 0x02, 0x00], ServerMessage::ObjectKilled { killed: 579, skill: 0, killer: 512 }),
            (&[0xC3, 0x09, 0x16, 0x82, 0x43, 0x00, 0x21, 0x00, 0x06], ServerMessage::ExperienceGained { killed: 0x8243, experience: 33, damage: 6 }),
            // The hero killed by monster 834, and its respawn in town.
            (&[0xC1, 0x09, 0x17, 0x02, 0x00, 0x00, 0x00, 0x03, 0x42], ServerMessage::ObjectKilled { killed: 512, skill: 0, killer: 834 }),
            (
                &[0xC1, 0x1C, 0xF3, 0x04, 0x8A, 0x86, 0x00, 0x00, 0x70, 0x00, 0x14, 0x00, 0x66, 0x00, 0x19, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x58, 0x00, 0x00, 0x00, 0x00],
                ServerMessage::Respawn(Respawn { x: 138, y: 134, map: 0, direction: 0, health: 112, mana: 20, shield: 102, ability: 25, experience: 88, money: 0 }),
            ),
            // Level 2 after another spider, and the new maximum mana.
            (
                &[0xC1, 0x18, 0xF3, 0x05, 0x02, 0x00, 0x05, 0x00, 0x72, 0x00, 0x15, 0x00, 0x67, 0x00, 0x19, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x02, 0x00],
                ServerMessage::LevelUp(LevelUp {
                    level: 2,
                    points: 5,
                    max_health: 114,
                    max_mana: 21,
                    max_shield: 103,
                    max_ability: 25,
                    fruit_points: 0,
                    max_fruit_points: 2,
                    negative_fruit_points: 0,
                    max_negative_fruit_points: 2,
                }),
            ),
            (&[0xC1, 0x08, 0x27, 0xFE, 0x00, 0x15, 0x00, 0x19], ServerMessage::MaximumMana { mana: 21, ability: 25 }),
        ];
        for (wire, m) in cases {
            assert_eq!(ServerMessage::decode(wire), Some(m.clone()), "{wire:02X?}");
            // The encrypted ones travel as C3; encoding gives the same bytes.
            assert_eq!(m.encode(), wire, "{m:?}");
        }
        // A spider attacking the hero (`ObjectAnimation`, already in `game`): action `AT_ATTACK1`.
        assert_eq!(
            ServerMessage::decode(&[0xC1, 0x09, 0x18, 0x02, 0x62, 0x07, 0x78, 0x02, 0x00]),
            Some(ServerMessage::ObjectAnimation { id: 610, direction: 7, action: crate::game::action::ATTACK1, target: 512 })
        );
    }

    /// OpenMU's `SendSkillStageUpdateAsync(attackerId, stage, 58)` (NovaSkillStartPlugin): id 0x0200,
    /// Nova Start, stage 3.
    #[test]
    fn skill_stage_bytes() {
        let wire = [0xC1, 0x07, 0xBA, 0x02, 0x00, 0x3A, 0x03];
        let m = ServerMessage::SkillStage { id: 0x0200, skill: 58, stage: 3 };
        assert_eq!(ServerMessage::decode(&wire), Some(m.clone()));
        assert_eq!(m.encode(), wire);
    }

    #[test]
    fn server_round_trips() {
        server_round_trip(ServerMessage::ObjectHit { id: 0x020F, damage: 23, damage_type: damage_type::EXCELLENT | damage_type::DOUBLE, shield_damage: 0 });
        server_round_trip(ServerMessage::SkillAnimation { skill: 44, source: 0x0200, target: 0x820F });
        server_round_trip(ServerMessage::AreaSkillAnimation { skill: 41, source: 0x0200, x: 182, y: 130, rotation: 64 });
        server_round_trip(ServerMessage::SkillTargetsAnimation { source: 0x0200, skill: 9, x: 182, y: 130, targets: vec![0x020F, 0x0210] });
        server_round_trip(ServerMessage::MagicEffectCancelled { skill: 27, target: 0x0200 });
        server_round_trip(ServerMessage::MagicEffectStatus { active: true, id: 0x0200, effect: 2 });
        server_round_trip(ServerMessage::ObjectKilled { killed: 0x020F, skill: 0, killer: 0x0200 });
        server_round_trip(ServerMessage::ExperienceGained { killed: 0x820F, experience: 120, damage: 31 });
        server_round_trip(ServerMessage::ExperienceGainedLarge { killed: 0x820F, experience: 0x0001_86A0, damage: 31 });
        server_round_trip(ServerMessage::CurrentHealth { health: 98, shield: 40 });
        server_round_trip(ServerMessage::MaximumHealth { health: 112, shield: 41 });
        server_round_trip(ServerMessage::CurrentMana { mana: 20, ability: 15 });
        server_round_trip(ServerMessage::MaximumMana { mana: 20, ability: 25 });
        server_round_trip(ServerMessage::PoisonDamage { damage: 3, shield: 30 });
        server_round_trip(ServerMessage::MonsterSkillAnimation { skill: 1, attacker: 0x020F, target: 0x0200 });
        server_round_trip(ServerMessage::Respawn(Respawn { x: 130, y: 120, map: 0, direction: 1, health: 112, mana: 20, shield: 40, ability: 25, experience: 9000, money: 77 }));
        server_round_trip(ServerMessage::LevelUp(LevelUp { level: 2, points: 5, max_health: 114, max_mana: 21, max_shield: 42, max_ability: 26, ..Default::default() }));
    }
}
