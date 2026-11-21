//! Sonido y música, como el original (`DSPlaySound.cpp`, `MiniAudioBackend.cpp`,
//! `SceneManager.cpp`).
//!
//! - **Efectos** (`PlayBuffer`): la tabla `Data/Audio/SoundFiles.json` da el archivo y los canales
//!   (1 a 4) de cada `SOUND_*`. Suenan en 2D. Los canales se usan por turnos. Un sonido de un solo
//!   canal, ligado a un objeto o en bucle no vuelve a empezar mientras ya suena alguno de sus canales.
//! - **Ambiente** por mapa (`PlayWorldAmbientSounds`): viento, mazmorra, torre, agua, desierto...
//! - **Música** por mapa (`ManageBackgroundMusic`): una pista en bucle; pedir otra cambia la que suena.
//! - **Pasos** (`PlayWalkSound`): `Data/Audio/SoundEvents.json` elige el sonido según el mapa y la casilla.
//!
//! Volumen de 0 a 10 (por defecto 5, como el original): `volumen_sonido=` y `volumen_musica=` en
//! `mu-launcher.txt`. `F7` cambia el de los efectos y `F8` el de la música.
//!
//! El resto del juego pide sonidos con [`SoundQueue`] (y el motor de efectos con `Ctx::play_sound`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use mu_data::terrain::{SIZE, attr};
use serde::Deserialize;

use crate::Args;
use crate::character::Player;
use crate::terrain::LoadedTerrain;

pub struct SoundPlugin;

impl Plugin for SoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundQueue>()
            .add_systems(Startup, setup)
            .add_systems(Update, (change_volume, world_sounds))
            // Al final del fotograma, cuando ya se ha pedido todo.
            .add_systems(PostUpdate, (take_fx_sounds, play_queue).chain());
    }
}

/// Máximo de canales por sonido (`MAX_CHANNEL`).
const MAX_CHANNEL: usize = 4;
/// `DefaultVolumeLevel` / `MaxVolumeLevel`.
const DEFAULT_VOLUME: u8 = 5;
const MAX_VOLUME: u8 = 10;

/// Cómo se pide un sonido.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayKind {
    /// `PlayBuffer(id)`.
    Once,
    /// `PlayBuffer(id, objeto)`: no vuelve a empezar mientras suena.
    Object,
    /// `PlayBuffer(id, NULL, true)`: en bucle hasta `stop`.
    Loop,
}

/// Peticiones de sonido de este fotograma.
#[derive(Resource, Default)]
pub struct SoundQueue {
    plays: Vec<(String, PlayKind)>,
    stops: Vec<String>,
}

impl SoundQueue {
    pub fn play(&mut self, id: &str) {
        self.plays.push((id.to_string(), PlayKind::Once));
    }
    pub fn play_object(&mut self, id: &str) {
        self.plays.push((id.to_string(), PlayKind::Object));
    }
    pub fn play_loop(&mut self, id: &str) {
        self.plays.push((id.to_string(), PlayKind::Loop));
    }
    /// `StopBuffer(id)`.
    pub fn stop(&mut self, id: &str) {
        self.stops.push(id.to_string());
    }
}

// --- Tablas ----------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct SoundFiles {
    sounds: Vec<SoundFile>,
}

#[derive(Deserialize)]
struct SoundFile {
    id: String,
    file: String,
    #[serde(default)]
    channels: usize,
}

#[derive(Deserialize)]
struct SoundEvents {
    events: Vec<SoundEvent>,
}

#[derive(Deserialize)]
struct SoundEvent {
    event: String,
    rules: Vec<EventRule>,
}

/// Una regla de `SoundEvents.json`: la primera que encaja elige el sonido.
#[derive(Deserialize, Clone)]
struct EventRule {
    #[serde(default)]
    worlds: Option<Vec<i32>>,
    #[serde(default)]
    tiles: Option<Vec<u8>>,
    #[serde(default, rename = "safeZone")]
    safe_zone: Option<bool>,
    plays: Vec<String>,
}

struct Channels {
    entities: Vec<Option<Entity>>,
    next: usize,
}

#[derive(Resource)]
struct Audio {
    data: PathBuf,
    /// `SOUND_*` → (archivo relativo a `Data`, canales).
    table: HashMap<String, (String, usize)>,
    sources: HashMap<String, Option<Handle<AudioSource>>>,
    channels: HashMap<String, Channels>,
    sfx_volume: u8,
    music_volume: u8,
    /// Pista que suena y su entidad.
    music: Option<(String, Entity)>,
    /// Efectos en bucle que suenan (para cambiarles el volumen).
    loops: Vec<Entity>,
}

/// Marca de las entidades de música (para cambiarles el volumen).
#[derive(Component)]
struct Music;

fn setup(mut commands: Commands, args: Res<Args>) {
    let read = |name: &str| std::fs::read_to_string(args.data.join("Audio").join(name)).ok();
    let mut table: HashMap<String, (String, usize)> = read("SoundFiles.json")
        .and_then(|t| serde_json::from_str::<SoundFiles>(&t).map_err(|e| warn!("SoundFiles.json: {e}")).ok())
        .map(|f| f.sounds.into_iter().map(|s| (s.id, (s.file, s.channels))).collect())
        .unwrap_or_default();
    let footstep = read("SoundEvents.json")
        .and_then(|t| serde_json::from_str::<SoundEvents>(&t).map_err(|e| warn!("SoundEvents.json: {e}")).ok())
        .and_then(|e| e.events.into_iter().find(|e| e.event == "heroFootstep"))
        .map(|e| e.rules)
        .unwrap_or_default();
    let _ = FOOTSTEP.set(footstep);
    // Los sonidos de los monstruos están en sus propios datos en MuMain (`MonsterSetup`): el del
    // dragón de la invasión (`Monster32`, el toro de la tabla original) va aquí.
    table.entry("SOUND_MONSTER_BULLATTACK1".into()).or_insert(("Data/Sound/mBullAttack1.wav".into(), 2));
    let level = |key: &str| {
        crate::settings::saved_value(key).and_then(|v| v.parse::<u8>().ok()).filter(|v| *v <= MAX_VOLUME).unwrap_or(DEFAULT_VOLUME)
    };
    let audio = Audio {
        data: args.data.clone(),
        table,
        sources: HashMap::new(),
        channels: HashMap::new(),
        sfx_volume: level("volumen_sonido"),
        music_volume: level("volumen_musica"),
        music: None,
        loops: Vec::new(),
    };
    info!("sonido: {} pasos, {} efectos en la tabla, volumen {}/10, música {}/10", FOOTSTEP.get().map_or(0, Vec::len), audio.table.len(), audio.sfx_volume, audio.music_volume);
    commands.insert_resource(audio);
}

/// `Data/Sound/w37/a.wav` → la ruta real dentro de `data` (sin distinguir mayúsculas).
fn resolve(data: &Path, file: &str) -> Option<PathBuf> {
    let mut parts: Vec<&str> = file.split(['/', '\\']).filter(|p| !p.is_empty()).collect();
    if parts.first().is_some_and(|p| p.eq_ignore_ascii_case("data")) {
        parts.remove(0);
    }
    let mut path = data.to_path_buf();
    for p in parts {
        path = mu_data::fs::find(&path, p)?;
    }
    Some(path)
}

impl Audio {
    fn source(&mut self, id: &str, sources: &mut Assets<AudioSource>) -> Option<Handle<AudioSource>> {
        if let Some(s) = self.sources.get(id) {
            return s.clone();
        }
        let loaded = self.table.get(id).and_then(|(file, _)| {
            let path = resolve(&self.data, file);
            match path.as_ref().map(std::fs::read) {
                // El decodificador de Bevy cierra el juego si no puede con el archivo: solo PCM.
                Some(Ok(bytes)) if !wav_is_pcm(&bytes) => {
                    warn!("sonido {id}: {file} no es PCM (no se puede reproducir)");
                    None
                }
                Some(Ok(bytes)) => Some(sources.add(AudioSource { bytes: Arc::from(bytes) })),
                _ => {
                    warn!("sonido {id}: no está {file}");
                    None
                }
            }
        });
        self.sources.insert(id.to_string(), loaded.clone());
        loaded
    }

    fn music_path(&self, name: &str) -> Option<PathBuf> {
        resolve(&self.data, &format!("Music/{name}.mp3"))
    }
}

/// `RIFF....WAVE` con su bloque `fmt ` en PCM (1) o en coma flotante (3).
fn wav_is_pcm(b: &[u8]) -> bool {
    if b.len() < 12 || &b[0..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return false;
    }
    let mut i = 12;
    while i + 8 <= b.len() {
        let size = u32::from_le_bytes([b[i + 4], b[i + 5], b[i + 6], b[i + 7]]) as usize;
        if &b[i..i + 4] == b"fmt " {
            return i + 10 <= b.len() && matches!(u16::from_le_bytes([b[i + 8], b[i + 9]]), 1 | 3);
        }
        i += 8 + size + (size & 1);
    }
    false
}

fn volume(level: u8) -> Volume {
    Volume::Linear(level as f32 / MAX_VOLUME as f32)
}

/// Lo que pidió el motor de efectos en sus ticks.
fn take_fx_sounds(mut world: ResMut<crate::fx::FxWorld>, mut queue: ResMut<SoundQueue>) {
    for id in world.sounds.drain(..) {
        queue.play(id);
    }
}

fn play_queue(
    mut commands: Commands,
    audio: Option<ResMut<Audio>>,
    mut queue: ResMut<SoundQueue>,
    mut sources: ResMut<Assets<AudioSource>>,
    alive: Query<(), With<AudioPlayer>>,
) {
    let Some(mut audio) = audio else {
        queue.plays.clear();
        queue.stops.clear();
        return;
    };
    for id in std::mem::take(&mut queue.stops) {
        if let Some(ch) = audio.channels.get_mut(&id) {
            for e in ch.entities.iter_mut().filter_map(Option::take) {
                if alive.contains(e) {
                    commands.entity(e).despawn();
                }
            }
        }
    }
    let plays = std::mem::take(&mut queue.plays);
    if audio.sfx_volume == 0 {
        return;
    }
    for (id, kind) in plays {
        let Some(source) = audio.source(&id, &mut sources) else { continue };
        let count = audio.table.get(&id).map_or(MAX_CHANNEL, |t| if t.1 == 0 { MAX_CHANNEL } else { t.1.min(MAX_CHANNEL) });
        let sfx = audio.sfx_volume;
        let ch = audio.channels.entry(id.clone()).or_insert_with(|| Channels { entities: vec![None; count], next: 0 });
        let playing = |e: &Option<Entity>| e.is_some_and(|e| alive.contains(e));
        // Un canal, ligado a un objeto o en bucle: si ya suena, nada.
        if (count == 1 || kind != PlayKind::Once) && ch.entities.iter().any(playing) {
            continue;
        }
        let slot = ch.next;
        ch.next = (slot + 1) % count;
        if let Some(old) = ch.entities[slot].take()
            && alive.contains(old)
        {
            commands.entity(old).despawn();
        }
        let settings = if kind == PlayKind::Loop { PlaybackSettings::LOOP } else { PlaybackSettings::DESPAWN };
        let e = commands.spawn((Name::new(id.clone()), AudioPlayer(source), settings.with_volume(volume(sfx)))).id();
        ch.entities[slot] = Some(e);
        if kind == PlayKind::Loop {
            audio.loops.push(e);
        }
    }
    audio.loops.retain(|e| alive.contains(*e));
}

/// `PlayMp3(nombre)`: si ya suena esa pista, nada; si suena otra, se cambia.
fn play_music(commands: &mut Commands, audio: &mut Audio, sources: &mut Assets<AudioSource>, name: &str) {
    if audio.music.as_ref().is_some_and(|(n, _)| n == name) {
        return;
    }
    stop_music(commands, audio, None);
    if audio.music_volume == 0 {
        return;
    }
    let Some(path) = audio.music_path(name) else {
        warn!("música: no está Data/Music/{name}.mp3");
        audio.music = Some((name.to_string(), Entity::PLACEHOLDER));
        return;
    };
    let Ok(bytes) = std::fs::read(&path) else { return };
    let source = sources.add(AudioSource { bytes: Arc::from(bytes) });
    let e = commands
        .spawn((Name::new(format!("Música {name}")), Music, AudioPlayer(source), PlaybackSettings::LOOP.with_volume(volume(audio.music_volume))))
        .id();
    audio.music = Some((name.to_string(), e));
}

/// `StopMp3(nombre)` (o cualquiera si `None`).
fn stop_music(commands: &mut Commands, audio: &mut Audio, name: Option<&str>) {
    if let Some((n, e)) = &audio.music
        && name.is_none_or(|name| name == n)
    {
        if *e != Entity::PLACEHOLDER {
            commands.entity(*e).try_despawn();
        }
        audio.music = None;
    }
}

/// Ambiente y música del mapa según dónde está el personaje (cada fotograma, como el original).
fn world_sounds(
    mut commands: Commands,
    args: Res<Args>,
    audio: Option<ResMut<Audio>>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&Player>,
    mut queue: ResMut<SoundQueue>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut rng: Local<u32>,
) {
    let (Some(mut audio), Some(t), Ok(p)) = (audio, terrain, players.single()) else { return };
    let (x, y) = ((p.pos.x.max(0.0) as usize).min(SIZE - 1), (p.pos.y.max(0.0) as usize).min(SIZE - 1));
    let tile = t.0.mapping.layer1[y * SIZE + x];
    let safe = t.0.attribute(x, y) & attr::SAFE_ZONE != 0;
    let world = args.world as i32 - 1;
    let mut chance = |n: u32| {
        *rng = rng.wrapping_mul(1_103_515_245).wrapping_add(12345);
        (*rng >> 16) % n == 0
    };

    // `PlayWorldAmbientSounds`.
    match world {
        0 => {
            if tile == 4 {
                queue.stop("SOUND_WIND01");
            } else {
                queue.play_loop("SOUND_WIND01");
            }
        }
        1 => queue.play_loop("SOUND_DUNGEON01"),
        2 => {
            if tile == 3 || tile >= 10 {
                queue.stop("SOUND_WIND01");
            } else {
                queue.play_loop("SOUND_WIND01");
            }
        }
        3 => {
            queue.play_loop("SOUND_WIND01");
            // `rand_fps_check(512)` por tick: aquí por fotograma, a 25 ticks por segundo.
            if chance(512 * 60 / 25) {
                queue.play("SOUND_FOREST01");
            }
        }
        4 => queue.play_loop("SOUND_TOWER01"),
        7 => queue.play_loop("SOUND_WATER01"),
        8 => queue.play_loop("SOUND_DESERT01"),
        10 => queue.play_loop("SOUND_HEAVEN01"),
        _ => {}
    }

    // `ManageBackgroundMusic`.
    let devias_church = (205.0..=214.0).contains(&p.pos.x.floor()) && (13.0..=31.0).contains(&p.pos.y.floor());
    let track = match world {
        0 if safe => Some(if tile == 4 { "Pub" } else { "main_theme" }),
        2 if safe => Some(if devias_church { "Church" } else { "Devias" }),
        3 if safe => Some("Noria"),
        1 | 5 => Some("Dungeon"),
        7 => Some("atlans"),
        10 => Some("icarus"),
        8 => Some("tarkan"),
        4 => Some("lost_tower_a"),
        24..=29 | 36 => Some("kalima"),
        _ => None,
    };
    if let Some(track) = track {
        play_music(&mut commands, &mut audio, &mut sources, track);
    }
}

/// `PlayWalkSound`: el paso según el mapa, la casilla y la zona segura.
pub fn footstep(world: i32, tile: u8, safe_zone: bool) -> Option<String> {
    FOOTSTEP.get().and_then(|rules| {
        rules
            .iter()
            .find(|r| {
                r.worlds.as_ref().is_none_or(|w| w.contains(&world))
                    && r.tiles.as_ref().is_none_or(|t| t.contains(&tile))
                    && r.safe_zone.is_none_or(|s| s == safe_zone)
            })
            .and_then(|r| r.plays.first().cloned())
    })
}

static FOOTSTEP: std::sync::OnceLock<Vec<EventRule>> = std::sync::OnceLock::new();

/// `F7`: volumen de los efectos; `F8`: de la música (0, 2, 4, ..., 10 y vuelta a 0). Se guarda.
fn change_volume(
    keys: Res<ButtonInput<KeyCode>>,
    audio: Option<ResMut<Audio>>,
    mut sinks: Query<(Entity, &mut AudioSink, Has<Music>)>,
    mut commands: Commands,
) {
    let Some(mut audio) = audio else { return };
    let next = |v: u8| if v >= MAX_VOLUME { 0 } else { (v / 2 * 2 + 2).min(MAX_VOLUME) };
    if keys.just_pressed(KeyCode::F7) {
        audio.sfx_volume = next(audio.sfx_volume);
        crate::settings::save_value("volumen_sonido", &audio.sfx_volume.to_string());
        info!("volumen de los efectos: {}/10", audio.sfx_volume);
        for (e, mut sink, music) in &mut sinks {
            if !music && audio.loops.contains(&e) {
                sink.set_volume(volume(audio.sfx_volume));
            }
        }
    }
    if keys.just_pressed(KeyCode::F8) {
        audio.music_volume = next(audio.music_volume);
        crate::settings::save_value("volumen_musica", &audio.music_volume.to_string());
        info!("volumen de la música: {}/10", audio.music_volume);
        if audio.music_volume == 0 {
            stop_music(&mut commands, &mut audio, None);
        }
        for (_, mut sink, music) in &mut sinks {
            if music {
                sink.set_volume(volume(audio.music_volume));
            }
        }
    }
}
