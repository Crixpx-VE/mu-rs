//! Game Server: una tarea por conexión y un mundo compartido con los jugadores dentro.
//!
//! Flujo de una conexión (el del cliente S6):
//! 1. Saludo (`GameServerEntered`) con el id del jugador.
//! 2. Login: si la cuenta no existe se crea con esa contraseña (servidor privado); si existe,
//!    tiene que coincidir. Una cuenta no puede estar conectada dos veces.
//! 3. Lista de personajes, crear (nombre de 4 a 10 letras o números, sin repetir) y borrar (con
//!    la contraseña de la cuenta como código).
//! 4. Elegir personaje: su información (`CharacterInformation`) y entra al mundo en su mapa; ve a
//!    los demás jugadores de ese mapa y ellos a él.
//! 5. Caminar: se guarda la casilla de llegada y se reenvía a los demás del mapa.
//! 6. Mover de mapa (`M`): con la clave de `MapMoveChecksumKey`, el nivel y el zen (warp.rs), paga,
//!    desaparece para los del mapa viejo y le llega `MapChanged`; cuando el cliente ha cargado
//!    (`ClientReadyAfterMapChange`) se ven él y los del mapa nuevo.
//! 7. Salir (o cortarse): se guarda dónde estaba y desaparece para los demás.
//! 8. Chat (`PublicChatMessage`): le llega a todos los que lo ven, él incluido, con su nombre (no
//!    el que diga el paquete), como `ChatMessageNormalProcessor` de OpenMU. Los canales de grupo
//!    (`~`), gremio (`@`, `@@`) y Gens (`$`) no van a nadie porque aún no hay grupos, gremios ni
//!    Gens. Lo que empieza por `/` es un comando: solo `/move <mapa>` (como la ventana `M`).
//! 9. Susurro (`WhisperMessage`): al personaje conectado con ese nombre exacto (`strcmp`); si no
//!    hay, `WhisperResult` con 0 (el cliente enseña "No users").
//! 10. Si no puede mover de mapa, se lo dice con un aviso azul (`ServerMessage` tipo 1) con los
//!    textos de OpenMU (`WarpAction.cs`); el de Icarus es `GlobalText[263]` del cliente.
//! 11. Puertas a pie (`EnterGateRequest`): con las reglas de `Warps::enter_gate` va a la puerta de
//!    llegada como con `M` (sin zen). Si la puerta no es de su mapa no se le contesta (como
//!    `WarpGateHandlerPlugIn`); si no llega al nivel o está lejos, `MapChanged` sin cambio de mapa
//!    en su casilla (`MapChangeFailedAsync` de OpenMU) para que el cliente siga.
//! 12. Animaciones (`AnimationRequest`): se reenvían como `ObjectAnimation` a los demás que lo ven
//!    (`AnimationHandlerPlugIn` de OpenMU) y se guarda la postura (sentado, apoyado, curándose) para
//!    los que lo vean aparecer después; caminar la quita.
//!
//! De momento "ver" es estar en el mismo mapa (sin radio de visión).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
use crate::spawn::Spawns;
use crate::warp::{Refused, Warps};

/// Un jugador dentro del mundo.
struct InWorld {
    account: String,
    name: String,
    class: classes::ClassIndex,
    map: u8,
    x: u8,
    y: u8,
    rotation: u8,
    /// `CharacterPose` (`action::pose`): 0 de pie.
    pose: u8,
    /// Visible en su mapa. Tras `MapChanged` no lo está hasta que el cliente ha cargado el mapa
    /// nuevo (`ClientReadyAfterMapChange`, como `PlayerMapTransitions` de OpenMU).
    ready: bool,
    /// Paquetes para él (en claro; su tarea de escritura los cifra).
    out: UnboundedSender<Vec<u8>>,
}

impl InWorld {
    fn scope(&self, id: u16) -> ScopeCharacter {
        ScopeCharacter {
            id,
            x: self.x,
            y: self.y,
            class: game::class_number(self.class),
            name: self.name.clone(),
            target_x: self.x,
            target_y: self.y,
            rotation: self.rotation,
            // `CharacterHeroState::Normal` (3): sin PK (con 0 el cliente lo pinta como un NPC).
            hero_state: 3,
            pose: self.pose,
        }
    }
}

#[derive(Default)]
struct World {
    players: HashMap<u16, InWorld>,
    /// Cuentas conectadas ahora mismo.
    online: HashSet<String>,
    next_id: u16,
}

pub struct Shared {
    db: Mutex<Db>,
    world: Mutex<World>,
    spawns: Spawns,
    warps: Warps,
    pub idle_timeout: Duration,
}

impl Shared {
    pub fn new(db: Db, spawns: Spawns, warps: Warps) -> Self {
        Self { db: Mutex::new(db), world: Mutex::new(World::default()), spawns, warps, idle_timeout: Duration::from_secs(300) }
    }
}

pub async fn serve(listener: TcpListener, shared: Arc<Shared>) -> std::io::Result<()> {
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

/// Estado de una conexión.
struct Session {
    id: u16,
    account: Option<String>,
    /// Personaje dentro del mundo.
    in_world: bool,
    /// Última clave de mover de mapa (`m_dwMapMoveKeyValue`): la que se mandó al entrar y luego
    /// cada una de [`game::map_move_key`].
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
    // Escritura: cifra (C3/C4) y manda en orden.
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
        // Ids de 1 a 0x7FFF que no estén en uso.
        loop {
            w.next_id = if w.next_id >= 0x7FFF { 1 } else { w.next_id + 1 };
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

/// Atiende un mensaje. `false` = cerrar la conexión.
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
            // Sin las cartas de la tienda: todas las clases se pueden crear.
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
                        class: game::class_number(c.class),
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
            // `CharacterDeleteResult`: 1 borrado, 2 código incorrecto, 0 no se pudo.
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
                // A la lista de servidores: la cuenta queda libre.
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
    // La clave para mover de mapa (`GCSendMapMoveChecksum` / `GenerateSeedValue`).
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
    };
    let mut w = shared.world.lock().unwrap();
    show_each_other(&w, s.id, &me);
    eprintln!("[game] {}: {} entra en el mapa {} ({}, {})", s.id, c.name, c.map, c.x, c.y);
    w.players.insert(s.id, me);
    s.in_world = true;
}

/// `me` (que aún no está en `w.players`, o es `id`) ve a los del mapa y ellos a él.
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

/// `WarpCommandRequest` (la ventana `M`): primero la clave y luego [`warp_to`].
fn warp(s: &mut Session, shared: &Shared, key: u32, index: u16) {
    if !s.in_world {
        return;
    }
    // `CheckKeyValue` (KeyGenerater.cpp:51-63): tiene que ser la siguiente de la última.
    let expected = game::map_move_key(s.move_key);
    if key != expected {
        eprintln!("[game] {}: clave de mover incorrecta ({key:#010X}, esperaba {expected:#010X})", s.id);
        return;
    }
    s.move_key = expected;
    warp_to(s, shared, index);
}

/// Un aviso azul en el chat del jugador (`ShowBlueMessageAsync` de OpenMU).
fn blue_notice(s: &Session, text: impl Into<String>) {
    s.send(ServerMessage::Notice { kind: game::notice_kind::BLUE, text: text.into() });
}

/// Por qué no puede ir, como se lo dice al jugador; `None` = no se le dice nada (un destino o una
/// puerta que no existen son cosa del cliente o de los datos, no del jugador).
fn refused_message(why: Refused) -> Option<String> {
    match why {
        // `WarpAction.CheckRequirements` de OpenMU.
        Refused::Level { needed } => Some(format!("You need to be level {needed} in order to warp")),
        Refused::Money { needed } => Some(format!("You need {needed} in order to warp")),
        // `GlobalText[263]`: lo que dice el cliente original si se intenta sin alas (ZzzInterface.cpp:9301-9305).
        Refused::NeedsWings => Some("You can enter Icarus only with wings, dinorant, fenrirr".to_string()),
        Refused::UnknownWarp | Refused::BadGate | Refused::NotEntrance | Refused::TooFar => None,
    }
}

/// Lo mismo al cruzar una puerta a pie: el nivel con `GlobalText[350]`, lo que enseña el cliente
/// original en `CheckGate` (ZzzInterface.cpp:7095-7100) antes de pedirlo (OpenMU dice "Your level is
/// too low to enter this map."; solo llega si el cliente no lo ha mirado).
fn gate_refused_message(why: Refused) -> Option<String> {
    match why {
        Refused::Level { needed } => Some(format!("Only characters over level {needed} can enter.")),
        other => refused_message(other),
    }
}

/// Lo que se cuenta en el registro.
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

/// Mover al destino `index` de la lista (ventana `M` o `/move`): destino, nivel y zen; luego
/// paga, deja el mapa viejo y le dice al cliente a dónde va (`MapChanged`). Si no se puede, se lo
/// dice en el chat (ver [`refused_message`]).
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

/// Lleva al jugador a `landing`: lo guarda, desaparece para los del mapa viejo y le llega
/// `MapChanged`; se le vuelve a ver con `ClientReadyAfterMapChange` ([`map_ready`]).
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
    s.send(ServerMessage::MapChanged { is_map_change: true, map: landing.map as u16, x: landing.x, y: landing.y, rotation: landing.angle });
}

/// `EnterGateRequest` (el héroe pisa una puerta): con las reglas de `Warps::enter_gate`, a su
/// puerta de llegada. La puerta 0 es el teletransporte del mago, que aún no hay.
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
            // Una puerta que no es de su mapa solo se registra (`WarpGateHandlerPlugIn`).
            if matches!(why, Refused::NotEntrance | Refused::BadGate) {
                return;
            }
            if let Some(text) = gate_refused_message(why) {
                blue_notice(s, text);
            }
            // `MapChangeFailedAsync` → `SendMapChangedAsync(mapa, x, y, rotación, false)` (MapChangePlugIn.cs).
            s.send(ServerMessage::MapChanged { is_map_change: false, map: map as u16, x, y, rotation });
        }
    }
}

/// `AnimationRequest`: se gira, guarda la postura y lo ven los demás del mapa (él no).
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

/// `ClientReadyAfterMapChange`: ya ha cargado el mapa nuevo; se ven él y los de ese mapa.
fn map_ready(s: &Session, shared: &Shared) {
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get_mut(&s.id).filter(|p| !p.ready) else { return };
    me.ready = true;
    let Some(me) = w.players.get(&s.id) else { return };
    show_each_other(&w, s.id, me);
}

/// Canal de un mensaje de chat por su prefijo (`ChatMessageAction.GetMessageType` de OpenMU; el
/// cliente los pone en NewUIChatInputBox.cpp:478-490 y los quita en `ReceiveChat`).
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

/// `PublicChatMessage`: a todos los del mapa que están en él (él también, así se ve lo que dice).
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

/// Comandos del chat:
/// - `/move <mapa>` (el nombre o el subnombre de la lista de la ventana `M`, sin mirar mayúsculas,
///   como `GetMapIndexFromMovereq`), con las mismas reglas que la ventana.
/// - `/goldnotice <texto>`, solo Game Master: aviso dorado en el centro de la pantalla de todos
///   (`NoticeChatCommandPlugIn` de OpenMU → `MessageType.GoldenCenter`, `ReceiveNotice` tipo 0).
///
/// El resto se ignora.
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

/// Un aviso a todos los que están en el mundo, en cualquier mapa (`SendGlobalMessageAsync` de OpenMU).
fn global_notice(shared: &Shared, kind: u8, text: &str) {
    let packet = ServerMessage::Notice { kind, text: text.to_string() }.encode();
    let w = shared.world.lock().unwrap();
    for p in w.players.values() {
        let _ = p.out.send(packet.clone());
    }
}

/// `WhisperMessage`: al personaje conectado con ese nombre; si no hay, `WhisperResult` 0.
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
    // Casilla de llegada: la de salida más cada paso.
    let (mut x, mut y) = (source_x as i32, source_y as i32);
    for &d in &directions {
        let (dx, dy) = game::DIRECTIONS[(d & 7) as usize];
        x += dx as i32;
        y += dy as i32;
    }
    me.x = x.clamp(0, 255) as u8;
    me.y = y.clamp(0, 255) as u8;
    me.rotation = rotation;
    // Al moverse deja de estar sentado o apoyado (el cliente también: camina).
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

/// Sale del mundo (guarda dónde estaba y desaparece para los demás); `disconnect` libera además la cuenta.
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
        // Puertas: el `GlobalText[350]` del cliente.
        assert_eq!(gate_refused_message(Refused::Level { needed: 20 }).as_deref(), Some("Only characters over level 20 can enter."));
        assert_eq!(gate_refused_message(Refused::TooFar), None);
    }
}