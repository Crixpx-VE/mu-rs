//! Game Server: one task per connection and a shared world with the players inside.
//!
//! Connection flow (the S6 client's):
//! 1. Greeting (`GameServerEntered`) with the player id.
//! 2. Login: if the account doesn't exist it is created with that password (private server); if it
//!    exists, it must match. An account can't be connected twice.
//! 3. Character list, create (name of 4 to 10 letters or digits, unique) and delete (with
//!    the account password as the code).
//! 4. Select character: its information (`CharacterInformation`) and it enters the world on its map; it
//!    sees the other players on that map and they see it.
//! 5. Walk: the destination tile is stored and relayed to the others on the map.
//! 6. Map move (`M`): with the `MapMoveChecksumKey` key, level and zen (warp.rs), pays,
//!    disappears for those on the old map and receives `MapChanged`; once the client has loaded
//!    (`ClientReadyAfterMapChange`) it and those on the new map see each other.
//! 7. Leave (or drop): where it was is saved and it disappears for the others.
//! 8. Chat (`PublicChatMessage`): reaches everyone who sees it, itself included, with its name (not
//!    the one the packet says), like OpenMU's `ChatMessageNormalProcessor`. The party (`~`),
//!    guild (`@`, `@@`) and Gens (`$`) channels go to nobody since there are no parties, guilds or
//!    Gens yet. Anything starting with `/` is a command: only `/move <map>` (like the `M` window).
//! 9. Whisper (`WhisperMessage`): to the connected character with that exact name (`strcmp`); if
//!    none, `WhisperResult` with 0 (the client shows "No users").
//! 10. If it can't change map, it is told with a blue notice (`ServerMessage` type 1) with
//!    OpenMU's texts (`WarpAction.cs`); the Icarus one is the client's `GlobalText[263]`.
//! 11. Walk-in gates (`EnterGateRequest`): with the `Warps::enter_gate` rules it goes to the
//!    destination gate as with `M` (no zen). If the gate isn't on its map there is no reply (like
//!    `WarpGateHandlerPlugIn`); if the level is too low or it's too far, `MapChanged` without a map change
//!    on its tile (OpenMU's `MapChangeFailedAsync`) so the client can continue.
//! 12. Animations (`AnimationRequest`): relayed as `ObjectAnimation` to the others who see it
//!    (OpenMU's `AnimationHandlerPlugIn`) and the pose (sitting, leaning, healing) is stored for
//!    those who see it appear later; walking clears it.
//!
//! 13. Monsters and NPCs (monsters.rs): each player sees those on its map within [`VIEW_RANGE`] tiles
//!    (`AddMonstersToScope`, and `MapObjectOutOfScope` when moving away); a [`TICK`] loop makes them roam
//!    and sends each walk (`ObjectWalked`) to those who see them.
//!
//! Between players, for now "seeing" means being on the same map (no view radius).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mu_protocol::appearance::Appearance;
use mu_protocol::crypto::ServerCrypto;
use mu_protocol::game::{
    self, CharacterInformation, CharacterListEntry, ClientMessage, LoginResult, ScopeCharacter, ServerMessage,
};
use mu_protocol::packet::PacketBuffer;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use crate::classes;
use crate::db::{CreateError, Db};
use crate::monsters::{MAX_MONSTERS, Mobs};
use crate::spawn::Spawns;
use crate::warp::{Refused, Warps};

/// A player inside the world.
struct InWorld {
    account: String,
    name: String,
    class: classes::ClassIndex,
    map: u8,
    x: u8,
    y: u8,
    rotation: u8,
    /// `CharacterPose` (`action::pose`): 0 standing.
    pose: u8,
    /// Visible on its map. After `MapChanged` it isn't until the client has loaded the new
    /// map (`ClientReadyAfterMapChange`, like OpenMU's `PlayerMapTransitions`).
    ready: bool,
    /// Packets for it (in plain text; its writer task encrypts them).
    out: UnboundedSender<Vec<u8>>,
    /// Monsters and NPCs its client has created.
    mobs_seen: HashSet<u16>,
}

impl InWorld {
    fn scope(&self, id: u16) -> ScopeCharacter {
        ScopeCharacter {
            id,
            x: self.x,
            y: self.y,
            // No equipment is sent yet (no inventory on this server).
            appearance: Appearance { pose: self.pose, ..Appearance::new(game::class_number(self.class)) },
            name: self.name.clone(),
            target_x: self.x,
            target_y: self.y,
            rotation: self.rotation,
            // `CharacterHeroState::Normal` (3): no PK (with 0 the client paints it as an NPC).
            hero_state: 3,
        }
    }
}

#[derive(Default)]
struct World {
    players: HashMap<u16, InWorld>,
    /// Accounts connected right now.
    online: HashSet<String>,
    next_id: u16,
    mobs: Mobs,
}

/// Tiles within which a player sees monsters (`InfoRange` from OpenMU's configuration, 12).
pub const VIEW_RANGE: u8 = 12;

/// How often monsters walk and decide.
const TICK: Duration = Duration::from_millis(50);

/// Player ids: above the monster ones.
const FIRST_PLAYER_ID: u16 = MAX_MONSTERS as u16 + 1;

pub struct Shared {
    db: Mutex<Db>,
    world: Mutex<World>,
    spawns: Spawns,
    warps: Warps,
    pub idle_timeout: Duration,
}

impl Shared {
    pub fn new(db: Db, spawns: Spawns, warps: Warps, mobs: Mobs) -> Self {
        let world = World { mobs, ..Default::default() };
        Self { db: Mutex::new(db), world: Mutex::new(world), spawns, warps, idle_timeout: Duration::from_secs(300) }
    }
}

pub async fn serve(listener: TcpListener, shared: Arc<Shared>) -> std::io::Result<()> {
    let ticker = shared.clone();
    tokio::spawn(async move {
        let mut every = tokio::time::interval(TICK);
        every.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            every.tick().await;
            tick(&ticker);
        }
    });
    loop {
        let (socket, addr) = listener.accept().await?;
        let shared = shared.clone();
        tokio::spawn(async move {
            match session(socket, &shared).await {
                Ok(()) => eprintln!("[game] {addr} desconectado"),
                Err(e) => eprintln!("[game] {addr} desconectado: {e}"),
            }
        });
    }
}

/// Connection state.
struct Session {
    id: u16,
    account: Option<String>,
    /// Character inside the world.
    in_world: bool,
    /// Last map-move key (`m_dwMapMoveKeyValue`): the one sent on entering and then
    /// each one from [`game::map_move_key`].
    move_key: u32,
    out: UnboundedSender<Vec<u8>>,
}

impl Session {
    fn send(&self, msg: ServerMessage) {
        let _ = self.out.send(msg.encode());
    }
}

async fn session(socket: TcpStream, shared: &Arc<Shared>) -> std::io::Result<()> {
    socket.set_nodelay(true)?;
    let (mut rd, mut wr) = socket.into_split();
    let (out, mut rx) = unbounded_channel::<Vec<u8>>();
    // Writer: encrypts (C3/C4) and sends in order.
    let writer = tokio::spawn(async move {
        let mut crypto = ServerCrypto::default();
        while let Some(p) = rx.recv().await {
            if wr.write_all(&crypto.encrypt(&p)).await.is_err() {
                break;
            }
        }
    });

    let id = {
        let mut w = shared.world.lock().unwrap();
        // Ids from `FIRST_PLAYER_ID` to 0x7FFF not in use.
        loop {
            w.next_id = if w.next_id >= 0x7FFF || w.next_id < FIRST_PLAYER_ID { FIRST_PLAYER_ID } else { w.next_id + 1 };
            if !w.players.contains_key(&w.next_id) {
                break w.next_id;
            }
        }
    };
    let mut s = Session { id, account: None, in_world: false, move_key: 0, out };
    s.send(ServerMessage::GameServerEntered { success: true, player_id: id, version: game::CLIENT_VERSION });

    let result = read_loop(&mut rd, &mut s, shared).await;
    leave(&mut s, shared, true);
    drop(s);
    let _ = writer.await;
    result
}

async fn read_loop(rd: &mut tokio::net::tcp::OwnedReadHalf, s: &mut Session, shared: &Arc<Shared>) -> std::io::Result<()> {
    let mut crypto = ServerCrypto::default();
    let mut buffer = PacketBuffer::default();
    let mut chunk = [0u8; 4096];
    loop {
        let n = tokio::time::timeout(shared.idle_timeout, rd.read(&mut chunk))
            .await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "sin actividad"))??;
        if n == 0 {
            return Ok(());
        }
        buffer.extend(&chunk[..n]);
        while let Some(packet) = buffer
            .next_packet()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("byte inválido {:#04X}", e.0)))?
        {
            let plain = match crypto.decrypt(&packet) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[game] {}: no se pudo descifrar: {e:?}", s.id);
                    continue;
                }
            };
            match ClientMessage::decode(&plain) {
                Some(msg) => {
                    if !handle(msg, s, shared) {
                        return Ok(());
                    }
                }
                None => eprintln!("[game] {}: paquete desconocido {:02X?}", s.id, &plain[..plain.len().min(8)]),
            }
        }
    }
}

/// Handles a message. `false` = close the connection.
fn handle(msg: ClientMessage, s: &mut Session, shared: &Arc<Shared>) -> bool {
    match msg {
        ClientMessage::Login { username, password, .. } => {
            if s.account.is_some() {
                return true;
            }
            let result = login(&username, &password, shared);
            if result == LoginResult::Okay {
                s.account = Some(username.clone());
                eprintln!("[game] {}: entra la cuenta {username}", s.id);
            }
            s.send(ServerMessage::LoginResponse(result));
        }
        ClientMessage::RequestCharacterList { .. } => {
            let Some(account) = s.account.clone() else { return true };
            if s.in_world {
                leave(s, shared, false);
            }
            let list = shared.db.lock().unwrap().characters(&account);
            // Without the shop cards: every class can be created.
            let unlock = 0x0F;
            s.send(ServerMessage::CharacterClassCreationUnlock { flags: unlock });
            s.send(ServerMessage::CharacterList {
                unlock_flags: unlock,
                move_count: 0,
                vault_extended: false,
                characters: list
                    .iter()
                    .map(|c| CharacterListEntry {
                        slot: c.slot,
                        name: c.name.clone(),
                        level: c.level,
                        status: 0,
                        appearance: Appearance::new(game::class_number(c.class)),
                        guild_position: c.guild_status,
                    })
                    .collect(),
            });
        }
        ClientMessage::CreateCharacter { name, class } => {
            let Some(account) = s.account.clone() else { return true };
            let class = game::base_class_index(class);
            let created = shared.db.lock().unwrap().create_character(&account, &name, class, |map| shared.spawns.pick(map));
            match created {
                Ok(c) => {
                    eprintln!("[game] {}: {account} crea a {} (clase {class})", s.id, c.name);
                    s.send(ServerMessage::CharacterCreated {
                        name: c.name,
                        slot: c.slot,
                        level: c.level,
                        class: game::class_number(c.class),
                    });
                }
                Err(CreateError::BadName | CreateError::Full) => s.send(ServerMessage::CharacterCreationFailed),
            }
        }
        ClientMessage::DeleteCharacter { name, security_code } => {
            let Some(account) = s.account.clone() else { return true };
            let mut db = shared.db.lock().unwrap();
            // `CharacterDeleteResult`: 1 deleted, 2 wrong code, 0 failed.
            let result = if !db.check_password(&account, &security_code) {
                2
            } else if db.delete_character(&account, &name) {
                eprintln!("[game] {}: {account} borra a {name}", s.id);
                1
            } else {
                0
            };
            s.send(ServerMessage::CharacterDeleteResponse { result });
        }
        ClientMessage::SelectCharacter { name } => {
            let Some(account) = s.account.clone() else { return true };
            if s.in_world {
                return true;
            }
            let Some(c) = shared.db.lock().unwrap().character(&account, &name) else { return true };
            enter_world(s, shared, &account, &c);
        }
        ClientMessage::Walk { source_x, source_y, rotation, directions } => walk(s, shared, source_x, source_y, rotation, directions),
        ClientMessage::Logout { kind } => {
            leave(s, shared, false);
            s.send(ServerMessage::LogoutResponse { kind });
            if kind == 2 {
                // To the server list: the account is released.
                if let Some(a) = s.account.take() {
                    shared.world.lock().unwrap().online.remove(&a);
                }
            }
        }
        ClientMessage::WarpCommandRequest { key, index } => warp(s, shared, key, index),
        ClientMessage::ClientReadyAfterMapChange => map_ready(s, shared),
        ClientMessage::Ping { .. } => {}
        ClientMessage::PublicChat { message, .. } => chat(s, shared, &message),
        ClientMessage::Whisper { target, message } => whisper(s, shared, &target, &message),
        ClientMessage::EnterGate { gate, .. } => enter_gate(s, shared, gate),
        ClientMessage::Animation { rotation, action } => animation(s, shared, rotation, action),
        // No items or NPC stores yet. Refusing is the only safe answer until the inventory is
        // stored with atomic operations (AGENTS.md: the server is the only authority over items).
        ClientMessage::ItemMove { .. } => s.send(ServerMessage::ItemMoveFailed),
        ClientMessage::BuyItem { .. } => s.send(ServerMessage::BuyFailed { closed: false }),
        ClientMessage::SellItem { .. } => s.send(ServerMessage::SellResult { result: 0, money: 0 }),
        ClientMessage::TalkToNpc { .. }
        | ClientMessage::CloseNpc
        | ClientMessage::CloseVault
        // No vault yet (it never opens here): nothing to move or unlock.
        | ClientMessage::VaultMoveMoney { .. }
        | ClientMessage::VaultPassword { .. }
        | ClientMessage::CloseCrafting
        | ClientMessage::GuildMasterAnswer { .. }
        | ClientMessage::RepairItem { .. }
        // No Gens or quest system on this server yet (OpenMU serves them).
        | ClientMessage::GensJoin { .. }
        | ClientMessage::GensLeave
        | ClientMessage::GensReward { .. }
        | ClientMessage::AvailableQuestsRequest
        | ClientMessage::NpcBuffRequest
        | ClientMessage::QuestSelect { .. }
        | ClientMessage::QuestProceed { .. }
        | ClientMessage::LegacyQuestState { .. }
        // No combat on this server yet (OpenMU is the one in use).
        | ClientMessage::Hit { .. }
        | ClientMessage::TargetedSkill { .. }
        | ClientMessage::CancelSkill { .. }
        | ClientMessage::AreaSkill { .. }
        | ClientMessage::AreaSkillHit { .. } => {}
    }
    true
}

fn login(username: &str, password: &str, shared: &Shared) -> LoginResult {
    if username.is_empty() || password.is_empty() {
        return LoginResult::InvalidPassword;
    }
    let mut db = shared.db.lock().unwrap();
    match db.account(username) {
        None => db.create_account(username, password),
        Some(_) if !db.check_password(username, password) => return LoginResult::InvalidPassword,
        Some(_) => {}
    }
    drop(db);
    let mut w = shared.world.lock().unwrap();
    if !w.online.insert(username.to_string()) {
        return LoginResult::AccountAlreadyConnected;
    }
    LoginResult::Okay
}

fn enter_world(s: &mut Session, shared: &Shared, account: &str, c: &crate::db::Character) {
    let max = classes::maximums(c.class, c.level, &c.stats);
    s.send(ServerMessage::CharacterInformation(CharacterInformation {
        x: c.x,
        y: c.y,
        map: c.map,
        direction: c.direction,
        experience: c.experience,
        next_experience: classes::experience_for_level(c.level),
        level_up_points: c.level_up_points,
        strength: c.stats.strength,
        agility: c.stats.agility,
        vitality: c.stats.vitality,
        energy: c.stats.energy,
        health: c.health.min(max.health),
        max_health: max.health,
        mana: c.mana.min(max.mana),
        max_mana: max.mana,
        shield: max.shield,
        max_shield: max.shield,
        ability: max.ability,
        max_ability: max.ability,
        money: c.money,
        leadership: c.stats.leadership,
        // `CharacterHeroState::Normal`.
        hero_state: 3,
        ..Default::default()
    }));
    // The map-move key (`GCSendMapMoveChecksum` / `GenerateSeedValue`).
    s.move_key = shared.warps.random() as u32;
    s.send(ServerMessage::MapMoveChecksumKey { key: s.move_key });
    let me = InWorld {
        account: account.to_string(),
        name: c.name.clone(),
        class: c.class,
        map: c.map,
        x: c.x,
        y: c.y,
        rotation: c.direction,
        pose: 0,
        ready: true,
        out: s.out.clone(),
        mobs_seen: HashSet::new(),
    };
    let mut w = shared.world.lock().unwrap();
    show_each_other(&w, s.id, &me);
    eprintln!("[game] {}: {} entra en el mapa {} ({}, {})", s.id, c.name, c.map, c.x, c.y);
    w.players.insert(s.id, me);
    refresh_mobs(&mut w, s.id);
    s.in_world = true;
}

/// The monsters `id` must see now: those within [`VIEW_RANGE`] appear and those no longer
/// in range disappear.
fn refresh_mobs(w: &mut World, id: u16) {
    let World { players, mobs, .. } = w;
    let Some(p) = players.get_mut(&id).filter(|p| p.ready) else { return };
    let near = |x: u8, y: u8| x.abs_diff(p.x) <= VIEW_RANGE && y.abs_diff(p.y) <= VIEW_RANGE;
    let gone: Vec<u16> =
        p.mobs_seen.iter().copied().filter(|&m| mobs.get(m).is_none_or(|m| m.map != p.map || !near(m.x, m.y))).collect();
    for chunk in gone.chunks(game::MAX_OUT_OF_SCOPE_IDS) {
        let _ = p.out.send(ServerMessage::MapObjectOutOfScope(chunk.to_vec()).encode());
    }
    for m in &gone {
        p.mobs_seen.remove(m);
    }
    let new: Vec<_> = mobs.list.iter().filter(|m| m.map == p.map && near(m.x, m.y) && !p.mobs_seen.contains(&m.id)).map(|m| m.scope()).collect();
    for chunk in new.chunks(64) {
        let _ = p.out.send(ServerMessage::AddMonstersToScope(chunk.to_vec()).encode());
    }
    p.mobs_seen.extend(new.iter().map(|m| m.id));
}

/// Monsters take their steps and decide; then each player sees those nearby.
fn tick(shared: &Shared) {
    let mut w = shared.world.lock().unwrap();
    let observed: HashSet<u16> = w.players.values().flat_map(|p| p.mobs_seen.iter().copied()).collect();
    let walks = w.mobs.tick(std::time::Instant::now(), |id| observed.contains(&id));
    for walk in walks {
        let packet = walk.message.encode();
        for p in w.players.values().filter(|p| p.ready && p.mobs_seen.contains(&walk.id)) {
            let _ = p.out.send(packet.clone());
        }
    }
    let ids: Vec<u16> = w.players.keys().copied().collect();
    for id in ids {
        refresh_mobs(&mut w, id);
    }
}

/// `me` (not yet in `w.players`, or is `id`) sees those on the map and they see it.
fn show_each_other(w: &World, id: u16, me: &InWorld) {
    let visible = |(pid, p): &(&u16, &InWorld)| **pid != id && p.ready && p.map == me.map;
    let others: Vec<ScopeCharacter> = w.players.iter().filter(visible).map(|(pid, p)| p.scope(*pid)).collect();
    if !others.is_empty() {
        let _ = me.out.send(ServerMessage::AddCharactersToScope(others).encode());
    }
    let mine = ServerMessage::AddCharactersToScope(vec![me.scope(id)]).encode();
    for (_, p) in w.players.iter().filter(visible) {
        let _ = p.out.send(mine.clone());
    }
}

/// `WarpCommandRequest` (the `M` window): first the key and then [`warp_to`].
fn warp(s: &mut Session, shared: &Shared, key: u32, index: u16) {
    if !s.in_world {
        return;
    }
    // `CheckKeyValue` (KeyGenerater.cpp:51-63): must be the one after the last.
    let expected = game::map_move_key(s.move_key);
    if key != expected {
        eprintln!("[game] {}: clave de mover incorrecta ({key:#010X}, esperaba {expected:#010X})", s.id);
        return;
    }
    s.move_key = expected;
    warp_to(s, shared, index);
}

/// A blue notice in the player's chat (OpenMU's `ShowBlueMessageAsync`).
fn blue_notice(s: &Session, text: impl Into<String>) {
    s.send(ServerMessage::Notice { kind: game::notice_kind::BLUE, text: text.into() });
}

/// Why it can't go, as told to the player; `None` = nothing is said (a destination or a
/// gate that don't exist are the client's or the data's fault, not the player's).
fn refused_message(why: Refused) -> Option<String> {
    match why {
        // OpenMU's `WarpAction.CheckRequirements`.
        Refused::Level { needed } => Some(format!("You need to be level {needed} in order to warp")),
        Refused::Money { needed } => Some(format!("You need {needed} in order to warp")),
        // `GlobalText[263]`: what the original client says when trying without wings (ZzzInterface.cpp:9301-9305).
        Refused::NeedsWings => Some("You can enter Icarus only with wings, dinorant, fenrirr".to_string()),
        Refused::UnknownWarp | Refused::BadGate | Refused::NotEntrance | Refused::TooFar => None,
    }
}

/// The same when crossing a gate on foot: the level with `GlobalText[350]`, what the original client
/// shows in `CheckGate` (ZzzInterface.cpp:7095-7100) before requesting it (OpenMU says "Your level is
/// too low to enter this map."; only reached if the client didn't check).
fn gate_refused_message(why: Refused) -> Option<String> {
    match why {
        Refused::Level { needed } => Some(format!("Only characters over level {needed} can enter.")),
        other => refused_message(other),
    }
}

/// What is written to the log.
fn refused_log(why: Refused, money: u32) -> String {
    match why {
        Refused::UnknownWarp => "no existe".to_string(),
        Refused::Level { needed } => format!("necesita nivel {needed}"),
        Refused::Money { needed } => format!("necesita {needed} de zen (tiene {money})"),
        Refused::NeedsWings => "Icarus pide alas".to_string(),
        Refused::BadGate => "puerta inexistente".to_string(),
        Refused::NotEntrance => "no es una puerta de entrada de su mapa".to_string(),
        Refused::TooFar => "está lejos de la puerta".to_string(),
    }
}

/// Move to destination `index` of the list (`M` window or `/move`): destination, level and zen; then
/// pays, leaves the old map and tells the client where it goes (`MapChanged`). If not possible, it
/// is told in chat (see [`refused_message`]).
fn warp_to(s: &Session, shared: &Shared, index: u16) {
    let Some(account) = s.account.clone() else { return };
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get(&s.id) else { return };
    if !me.ready {
        return;
    }
    let name = me.name.clone();
    let Some(c) = shared.db.lock().unwrap().character(&account, &name) else { return };
    let (landing, cost) = match shared.warps.check(index, c.class, c.level, c.money) {
        Ok(ok) => ok,
        Err(why) => {
            eprintln!("[game] {}: {name} no puede ir al destino {index}: {}", s.id, refused_log(why, c.money));
            if let Some(text) = refused_message(why) {
                blue_notice(s, text);
            }
            return;
        }
    };
    let money = c.money - cost;
    shared.db.lock().unwrap().set_money(&account, &name, money);
    s.send(ServerMessage::InventoryMoneyUpdate { money });
    move_to(s, shared, &mut w, &account, landing);
    eprintln!("[game] {}: {name} va al destino {index}: mapa {} ({}, {}), paga {cost}", s.id, landing.map, landing.x, landing.y);
}

/// Takes the player to `landing`: saves it, disappears for those on the old map and it receives
/// `MapChanged`; it is seen again with `ClientReadyAfterMapChange` ([`map_ready`]).
fn move_to(s: &Session, shared: &Shared, w: &mut World, account: &str, landing: crate::warp::Landing) {
    let Some(me) = w.players.get(&s.id) else { return };
    shared.db.lock().unwrap().update_position(account, &me.name, landing.map, landing.x, landing.y, landing.angle);
    let old_map = me.map;
    let gone = ServerMessage::MapObjectOutOfScope(vec![s.id]).encode();
    for (_, p) in w.players.iter().filter(|(pid, p)| **pid != s.id && p.ready && p.map == old_map) {
        let _ = p.out.send(gone.clone());
    }
    let Some(me) = w.players.get_mut(&s.id) else { return };
    (me.map, me.x, me.y, me.rotation, me.pose, me.ready) = (landing.map, landing.x, landing.y, landing.angle, 0, false);
    // The client clears everything from the map when loading the new one.
    me.mobs_seen.clear();
    s.send(ServerMessage::MapChanged { is_map_change: true, map: landing.map as u16, x: landing.x, y: landing.y, rotation: landing.angle });
}

/// `EnterGateRequest` (the hero steps on a gate): with the `Warps::enter_gate` rules, to its
/// destination gate. Gate 0 is the wizard's teleport, which doesn't exist yet.
fn enter_gate(s: &Session, shared: &Shared, gate: u16) {
    let Some(account) = s.account.clone() else { return };
    if !s.in_world || gate == 0 {
        return;
    }
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get(&s.id).filter(|p| p.ready) else { return };
    let (name, map, x, y, rotation) = (me.name.clone(), me.map, me.x, me.y, me.rotation);
    let Some(c) = shared.db.lock().unwrap().character(&account, &name) else { return };
    match shared.warps.enter_gate(gate, map, (x, y), c.class, c.level) {
        Ok(landing) => {
            move_to(s, shared, &mut w, &account, landing);
            eprintln!("[game] {}: {name} cruza la puerta {gate}: mapa {} ({}, {})", s.id, landing.map, landing.x, landing.y);
        }
        Err(why) => {
            eprintln!("[game] {}: {name} no puede cruzar la puerta {gate} desde el mapa {map} ({x}, {y}): {}", s.id, refused_log(why, c.money));
            // A gate not on its map is only logged (`WarpGateHandlerPlugIn`).
            if matches!(why, Refused::NotEntrance | Refused::BadGate) {
                return;
            }
            if let Some(text) = gate_refused_message(why) {
                blue_notice(s, text);
            }
            // `MapChangeFailedAsync` → `SendMapChangedAsync(map, x, y, rotation, false)` (MapChangePlugIn.cs).
            s.send(ServerMessage::MapChanged { is_map_change: false, map: map as u16, x, y, rotation });
        }
    }
}

/// `AnimationRequest`: turns, stores the pose and the others on the map see it (not itself).
fn animation(s: &Session, shared: &Shared, rotation: u8, action: u8) {
    if !s.in_world {
        return;
    }
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get_mut(&s.id).filter(|p| p.ready) else { return };
    me.rotation = rotation & 7;
    me.pose = game::action::pose(action);
    let map = me.map;
    let packet = ServerMessage::ObjectAnimation { id: s.id, direction: rotation, action, target: 0 }.encode();
    for (id, p) in &w.players {
        if *id != s.id && p.ready && p.map == map {
            let _ = p.out.send(packet.clone());
        }
    }
}

/// `ClientReadyAfterMapChange`: the new map has loaded; it and those on that map see each other.
fn map_ready(s: &Session, shared: &Shared) {
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get_mut(&s.id).filter(|p| !p.ready) else { return };
    me.ready = true;
    let Some(me) = w.players.get(&s.id) else { return };
    show_each_other(&w, s.id, me);
    refresh_mobs(&mut w, s.id);
}

/// Chat message channel by its prefix (OpenMU's `ChatMessageAction.GetMessageType`; the
/// client adds them in NewUIChatInputBox.cpp:478-490 and strips them in `ReceiveChat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Channel {
    Normal,
    Party,
    Guild,
    Alliance,
    Gens,
    Command,
}

fn channel(message: &str) -> Channel {
    if message.starts_with("@@") {
        Channel::Alliance
    } else if message.starts_with('@') {
        Channel::Guild
    } else if message.starts_with('~') {
        Channel::Party
    } else if message.starts_with('$') {
        Channel::Gens
    } else if message.starts_with('/') {
        Channel::Command
    } else {
        Channel::Normal
    }
}

/// `PublicChatMessage`: to everyone on the map who is in it (itself too, so it sees what it says).
fn chat(s: &Session, shared: &Shared, message: &str) {
    if !s.in_world {
        return;
    }
    let ch = channel(message);
    if ch == Channel::Command {
        command(s, shared, message);
        return;
    }
    let w = shared.world.lock().unwrap();
    let Some(me) = w.players.get(&s.id).filter(|p| p.ready) else { return };
    if ch != Channel::Normal {
        eprintln!("[game] {}: {} habla por el canal {ch:?}, que aún no existe: no le llega a nadie", s.id, me.name);
        return;
    }
    eprintln!("[game] {}: {} dice: {message}", s.id, me.name);
    let packet = ServerMessage::ChatMessage { sender: me.name.clone(), message: message.to_string() }.encode();
    for p in w.players.values().filter(|p| p.ready && p.map == me.map) {
        let _ = p.out.send(packet.clone());
    }
}

/// Chat commands:
/// - `/move <map>` (the name or subname from the `M` window list, case-insensitive,
///   like `GetMapIndexFromMovereq`), with the same rules as the window.
/// - `/goldnotice <text>`, Game Master only: golden notice in the center of everyone's screen
///   (OpenMU's `NoticeChatCommandPlugIn` → `MessageType.GoldenCenter`, `ReceiveNotice` type 0).
///
/// Everything else is ignored.
fn command(s: &Session, shared: &Shared, message: &str) {
    let mut words = message.splitn(2, ' ');
    let name = words.next().unwrap_or("");
    let rest = words.next().unwrap_or("").trim();
    if name.eq_ignore_ascii_case("/goldnotice") {
        let gm = s.account.as_deref().is_some_and(|a| shared.db.lock().unwrap().account(a).is_some_and(|a| a.game_master));
        if !gm {
            eprintln!("[game] {}: /goldnotice sin ser Game Master", s.id);
        } else if !rest.is_empty() {
            eprintln!("[game] {}: aviso dorado: {rest}", s.id);
            global_notice(shared, game::notice_kind::GOLDEN_CENTER, rest);
        }
        return;
    }
    if !name.eq_ignore_ascii_case("/move") {
        eprintln!("[game] {}: comando desconocido {name:?}", s.id);
        return;
    }
    match shared.warps.find_by_name(rest) {
        Some(m) => warp_to(s, shared, m.index as u16),
        None => eprintln!("[game] {}: /move a un mapa que no está en la lista: {rest:?}", s.id),
    }
}

/// A notice to everyone in the world, on any map (OpenMU's `SendGlobalMessageAsync`).
fn global_notice(shared: &Shared, kind: u8, text: &str) {
    let packet = ServerMessage::Notice { kind, text: text.to_string() }.encode();
    let w = shared.world.lock().unwrap();
    for p in w.players.values() {
        let _ = p.out.send(packet.clone());
    }
}

/// `WhisperMessage`: to the connected character with that name; if none, `WhisperResult` 0.
fn whisper(s: &Session, shared: &Shared, target: &str, message: &str) {
    if !s.in_world {
        return;
    }
    let w = shared.world.lock().unwrap();
    let Some(me) = w.players.get(&s.id) else { return };
    match w.players.values().find(|p| p.name == target) {
        Some(p) => {
            eprintln!("[game] {}: {} susurra a {target}: {message}", s.id, me.name);
            let _ = p.out.send(ServerMessage::WhisperMessage { sender: me.name.clone(), message: message.to_string() }.encode());
        }
        None => s.send(ServerMessage::WhisperResult { success: false }),
    }
}

fn walk(s: &Session, shared: &Shared, source_x: u8, source_y: u8, rotation: u8, directions: Vec<u8>) {
    if !s.in_world {
        return;
    }
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get_mut(&s.id).filter(|p| p.ready) else { return };
    // Destination tile: the starting one plus each step.
    let (mut x, mut y) = (source_x as i32, source_y as i32);
    for &d in &directions {
        let (dx, dy) = game::DIRECTIONS[(d & 7) as usize];
        x += dx as i32;
        y += dy as i32;
    }
    me.x = x.clamp(0, 255) as u8;
    me.y = y.clamp(0, 255) as u8;
    me.rotation = rotation;
    // Moving ends sitting or leaning (the client does too: it walks).
    me.pose = 0;
    let map = me.map;
    let packet =
        ServerMessage::ObjectWalked { id: s.id, target_x: me.x, target_y: me.y, rotation, directions }.encode();
    for (id, p) in &w.players {
        if *id != s.id && p.ready && p.map == map {
            let _ = p.out.send(packet.clone());
        }
    }
}

/// Leaves the world (saves where it was and disappears for the others); `disconnect` also releases the account.
fn leave(s: &mut Session, shared: &Shared, disconnect: bool) {
    let mut w = shared.world.lock().unwrap();
    if s.in_world {
        s.in_world = false;
        if let Some(me) = w.players.remove(&s.id) {
            let gone = ServerMessage::MapObjectOutOfScope(vec![s.id]).encode();
            for p in w.players.values().filter(|p| me.ready && p.ready && p.map == me.map) {
                let _ = p.out.send(gone.clone());
            }
            shared.db.lock().unwrap().update_position(&me.account, &me.name, me.map, me.x, me.y, me.rotation);
            eprintln!("[game] {}: {} sale del mundo", s.id, me.name);
        }
    }
    if disconnect {
        if let Some(a) = s.account.take() {
            w.online.remove(&a);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canales_del_chat() {
        assert_eq!(channel("hola"), Channel::Normal);
        assert_eq!(channel("~al grupo"), Channel::Party);
        assert_eq!(channel("@al gremio"), Channel::Guild);
        assert_eq!(channel("@@a la alianza"), Channel::Alliance);
        assert_eq!(channel("$a los Gens"), Channel::Gens);
        assert_eq!(channel("/move Noria"), Channel::Command);
        assert_eq!(channel(""), Channel::Normal);
    }

    #[test]
    fn avisos_al_no_poder_mover() {
        assert_eq!(refused_message(Refused::Level { needed: 10 }).as_deref(), Some("You need to be level 10 in order to warp"));
        assert_eq!(refused_message(Refused::Money { needed: 2000 }).as_deref(), Some("You need 2000 in order to warp"));
        assert!(refused_message(Refused::NeedsWings).is_some_and(|t| t.contains("Icarus")));
        assert_eq!(refused_message(Refused::UnknownWarp), None);
        // Gates: the client's `GlobalText[350]`.
        assert_eq!(gate_refused_message(Refused::Level { needed: 20 }).as_deref(), Some("Only characters over level 20 can enter."));
        assert_eq!(gate_refused_message(Refused::TooFar), None);
    }
}