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
            hero_state: 0,
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

/// `WarpCommandRequest` (la ventana `M`): clave, destino, nivel y zen; luego paga, deja el mapa
/// viejo y le dice al cliente a dónde va (`MapChanged`). Sin respuesta si no se puede (el
/// original avisa en el chat).
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
            let why = match why {
                Refused::UnknownWarp => "no existe".to_string(),
                Refused::Level { needed } => format!("necesita nivel {needed}"),
                Refused::Money { needed } => format!("necesita {needed} de zen (tiene {})", c.money),
                Refused::NeedsWings => "Icarus pide alas".to_string(),
                Refused::BadGate => "puerta inexistente".to_string(),
            };
            eprintln!("[game] {}: {name} no puede ir al destino {index}: {why}", s.id);
            return;
        }
    };
    let money = c.money - cost;
    {
        let mut db = shared.db.lock().unwrap();
        db.set_money(&account, &name, money);
        db.update_position(&account, &name, landing.map, landing.x, landing.y, landing.angle);
    }
    s.send(ServerMessage::InventoryMoneyUpdate { money });
    // Desaparece para los del mapa viejo.
    let old_map = me.map;
    let gone = ServerMessage::MapObjectOutOfScope(vec![s.id]).encode();
    for (_, p) in w.players.iter().filter(|(pid, p)| **pid != s.id && p.ready && p.map == old_map) {
        let _ = p.out.send(gone.clone());
    }
    let Some(me) = w.players.get_mut(&s.id) else { return };
    (me.map, me.x, me.y, me.rotation, me.ready) = (landing.map, landing.x, landing.y, landing.angle, false);
    s.send(ServerMessage::MapChanged { is_map_change: true, map: landing.map as u16, x: landing.x, y: landing.y, rotation: landing.angle });
    eprintln!("[game] {}: {name} va al destino {index}: mapa {} ({}, {}), paga {cost}", s.id, landing.map, landing.x, landing.y);
}

/// `ClientReadyAfterMapChange`: ya ha cargado el mapa nuevo; se ven él y los de ese mapa.
fn map_ready(s: &Session, shared: &Shared) {
    let mut w = shared.world.lock().unwrap();
    let Some(me) = w.players.get_mut(&s.id).filter(|p| !p.ready) else { return };
    me.ready = true;
    let Some(me) = w.players.get(&s.id) else { return };
    show_each_other(&w, s.id, me);
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
