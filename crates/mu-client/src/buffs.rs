//! The buffs and debuffs of every character in view, as the server tells them (`g_CharacterRegisterBuff`
//! / `g_CharacterUnRegisterBuff`): the effects list of `AddCharactersToScope` / `AddMonstersToScope`
//! (`s_BuffCount` + `BuffState[]`, WSclient.cpp `ReceiveCreatePlayerViewport` /
//! `ReceiveCreateMonsterViewport`) and `MagicEffectStatus` (`ReceiveSkillStatus`, MuMain
//! WSclient.cpp:3890-3990). The effects engine reads them through `Actor::buffs`
//! (`g_isCharacterBuff`).
//!
//! Not here: the buffs the client gives monsters itself when they appear (`CreateMonster`'s
//! `g_CharacterRegisterBuff`, MuMain's monster setup) and the buff icons of the interface.

use std::collections::HashMap;

use bevy::prelude::*;
use mu_protocol::game::{OBJECT_ID_MASK, ServerMessage};

use crate::fx::tables::ids::eBuff_Count;
use crate::scene::GameScene;

pub struct BuffsPlugin;

impl Plugin for BuffsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Buffs>()
            .add_systems(PreUpdate, receive.after(crate::net::NetSystems))
            .add_systems(OnExit(GameScene::Game), |mut b: ResMut<Buffs>| b.0.clear());
    }
}

/// `eBuffState`s of each object in view, by its server key.
#[derive(Resource, Default, Debug, PartialEq)]
pub struct Buffs(pub HashMap<u16, Vec<i32>>);

impl Buffs {
    /// The buffs of `key` (empty if none).
    pub fn of(&self, key: u16) -> &[i32] {
        self.0.get(&key).map_or(&[], Vec::as_slice)
    }

    /// What one server message changes.
    pub fn apply(&mut self, m: &ServerMessage) {
        let valid = |b: i32| b > 0 && b < eBuff_Count;
        let mut set = |key: u16, effects: &[u8]| {
            let list: Vec<i32> = effects.iter().map(|&b| b as i32).filter(|&b| valid(b)).collect();
            if list.is_empty() {
                self.0.remove(&key);
            } else {
                self.0.insert(key, list);
            }
        };
        match m {
            // A key back in scope is a new object (`CreateCharacter` clears its buffs).
            ServerMessage::AddCharactersToScope(list) => list.iter().for_each(|c| set(c.id & OBJECT_ID_MASK, &c.effects)),
            ServerMessage::AddMonstersToScope(list) => list.iter().for_each(|c| set(c.id & OBJECT_ID_MASK, &c.effects)),
            // `ReceiveSkillStatus`: `State == 1` registers it (ignoring `eBuffNone` and the ones past
            // `eBuff_Count`), otherwise it is removed.
            ServerMessage::MagicEffectStatus { active, id, effect } => {
                let (key, b) = (id & OBJECT_ID_MASK, *effect as i32);
                if *active {
                    if valid(b) {
                        let list = self.0.entry(key).or_default();
                        if !list.contains(&b) {
                            list.push(b);
                        }
                    }
                } else if let Some(list) = self.0.get_mut(&key) {
                    list.retain(|&x| x != b);
                    if list.is_empty() {
                        self.0.remove(&key);
                    }
                }
            }
            ServerMessage::MapObjectOutOfScope(ids) => ids.iter().for_each(|id| _ = self.0.remove(&(id & OBJECT_ID_MASK))),
            _ => {}
        }
    }
}

fn receive(mut from: MessageReader<crate::net::FromServer>, mut buffs: ResMut<Buffs>) {
    for m in from.read() {
        buffs.apply(&m.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mu_protocol::game::{ScopeCharacter, ScopeMonster};

    #[test]
    fn buffs_follow_the_server() {
        let mut b = Buffs::default();
        // A monster with Poison (55) and a player with Cloaking (18) and an unknown 250.
        b.apply(&ServerMessage::AddMonstersToScope(vec![ScopeMonster { id: 0x8000 | 7, effects: vec![55], ..Default::default() }]));
        b.apply(&ServerMessage::AddCharactersToScope(vec![ScopeCharacter { id: 9, effects: vec![18, 250], ..Default::default() }]));
        assert_eq!((b.of(7), b.of(9)), (&[55][..], &[18][..]));
        // `MagicEffectStatus` adds and removes; the last one gone, the key goes.
        b.apply(&ServerMessage::MagicEffectStatus { active: true, id: 7, effect: 2 });
        b.apply(&ServerMessage::MagicEffectStatus { active: true, id: 7, effect: 2 });
        assert_eq!(b.of(7), &[55, 2]);
        b.apply(&ServerMessage::MagicEffectStatus { active: false, id: 7, effect: 55 });
        b.apply(&ServerMessage::MagicEffectStatus { active: false, id: 7, effect: 2 });
        assert!(!b.0.contains_key(&7));
        b.apply(&ServerMessage::MapObjectOutOfScope(vec![9]));
        assert_eq!(b, Buffs::default());
    }
}
