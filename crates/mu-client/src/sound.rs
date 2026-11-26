//! Sound and music, like the original (`DSPlaySound.cpp`, `MiniAudioBackend.cpp`,
//! `SceneManager.cpp`).
//!
//! - **Effects** (`PlayBuffer`): the `Data/Audio/SoundFiles.json` table gives the file and the channels
//!   (1 to 4) of each `SOUND_*`. They play in 2D. Channels are used in turns. A sound with a single
//!   channel, bound to an object or looping does not restart while any of its channels is playing.
//! - **Ambience** per map (`PlayWorldAmbientSounds`): wind, dungeon, tower, water, desert...
//! - **Music** per map (`ManageBackgroundMusic`): one looping track; requesting another switches it.
//! - **Footsteps** (`PlayWalkSound`): `Data/Audio/SoundEvents.json` picks the sound by map and tile.
//!
//! Volume from 0 to 10 (default 5, like the original): `volumen_sonido=` and `volumen_musica=` in
//! `mu-launcher.txt`. The options windows read it with [`volumes`] and change it with [`SoundQueue::set_sfx_volume`] and
//! [`SoundQueue::set_music_volume`].
//!
//! The rest of the game requests sounds through [`SoundQueue`] (and the effects engine through `Ctx::play_sound`).

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
            .add_systems(Update, world_sounds)
            // At the end of the frame, once everything has been requested.
            .add_systems(PostUpdate, (take_fx_sounds, apply_volume, play_queue).chain().in_set(SoundSystems));
    }
}

/// Where the frame's requests are handled (in `PostUpdate`): anything that wants to override another
/// system's request (e.g. the credits music) goes before.
#[derive(SystemSet, Clone, PartialEq, Eq, Hash, Debug)]
pub struct SoundSystems;

/// Max channels per sound (`MAX_CHANNEL`).
const MAX_CHANNEL: usize = 4;
/// `DefaultVolumeLevel` / `MaxVolumeLevel`.
const DEFAULT_VOLUME: u8 = 5;
const MAX_VOLUME: u8 = 10;

/// How a sound is requested.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PlayKind {
    /// `PlayBuffer(id)`.
    Once,
    /// `PlayBuffer(id, object)`: does not restart while playing.
    Object,
    /// `PlayBuffer(id, NULL, true)`: loops until `stop`.
    Loop,
}

/// Sound requests of this frame.
#[derive(Resource, Default)]
pub struct SoundQueue {
    plays: Vec<(String, PlayKind)>,
    stops: Vec<String>,
    /// `PlayMp3`: track requested this frame (name in `Data/Music`, without `.mp3`).
    music: Option<String>,
    /// `StopMp3`: stop the music.
    stop_music: bool,
    /// New effects and music volume (from the options window).
    set_sfx: Option<u8>,
    set_music: Option<u8>,
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
    /// `PlayMp3(name)`: if already playing, nothing; if another is playing, switch.
    pub fn music(&mut self, name: &str) {
        self.music = Some(name.to_string());
    }
    /// `StopMp3`.
    pub fn stop_music(&mut self) {
        self.stop_music = true;
    }
    /// `SetEffectVolumeLevel`: effects volume (0 to 10). Saved in `mu-launcher.txt`.
    pub fn set_sfx_volume(&mut self, level: u8) {
        self.set_sfx = Some(level.min(MAX_VOLUME));
    }
    /// Music volume (0 to 10). Saved in `mu-launcher.txt`.
    pub fn set_music_volume(&mut self, level: u8) {
        self.set_music = Some(level.min(MAX_VOLUME));
    }
}

/// Effects and music volume (0 to 10), counting what was requested this frame.
pub fn volumes(audio: Option<&Audio>, queue: &SoundQueue) -> (u8, u8) {
    let (sfx, music) = audio.map_or((DEFAULT_VOLUME, DEFAULT_VOLUME), |a| (a.sfx_volume, a.music_volume));
    (queue.set_sfx.unwrap_or(sfx), queue.set_music.unwrap_or(music))
}

// --- Tables ----------------------------------------------------------------------------------------

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

/// A `SoundEvents.json` rule: the first one that matches picks the sound.
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
pub struct Audio {
    data: PathBuf,
    /// `SOUND_*` → (file relative to `Data`, channels).
    table: HashMap<String, (String, usize)>,
    sources: HashMap<String, Option<Handle<AudioSource>>>,
    channels: HashMap<String, Channels>,
    sfx_volume: u8,
    music_volume: u8,
    /// Playing track and its entity.
    music: Option<(String, Entity)>,
    /// Looping effects currently playing (to change their volume).
    loops: Vec<Entity>,
}

/// Marker for music entities (to change their volume).
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
    // Monster sounds live in their own data in MuMain (`MonsterSetup`): the one for the
    // invasion dragon (`Monster32`, the bull in the original table) goes here.
    table.entry("SOUND_MONSTER_BULLATTACK1".into()).or_insert(("Data/Sound/mBullAttack1.wav".into(), 2));
    // The Kalima waterfalls (`MapManager.cpp`: `LoadWaveFile(SOUND_KALIMA_WATER_FALL, ..., 3)`), missing from
    // the MuMain table. One channel: `PlayBuffer` (DSplaysound.cpp:422-454) never goes past channel 0,
    // so requesting it every tick while playing does not restart it.
    table.entry("SOUND_KALIMA_WATER_FALL".into()).or_insert(("Data/Sound/aKalimaWaterFall.wav".into(), 1));
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

/// `Data/Sound/w37/a.wav` → the real path inside `data` (case-insensitive).
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
                // Bevy's decoder crashes the game if it cannot handle the file: PCM only.
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

/// `RIFF....WAVE` with its `fmt ` chunk in PCM (1) or floating point (3).
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

/// What the effects engine requested during its ticks.
fn take_fx_sounds(mut world: ResMut<crate::fx::FxWorld>, mut queue: ResMut<SoundQueue>) {
    use crate::fx::core::FxSound;
    let mut seen: Vec<FxSound> = Vec::new();
    for s in world.sounds.drain(..) {
        // An object-bound or looping sound does not restart while it plays, but `play_queue` only
        // sees the players of earlier frames: one request per frame (several objects, or two ticks
        // in one frame, ask for the same one).
        if matches!(s, FxSound::Object(_) | FxSound::Loop(_)) {
            if seen.contains(&s) {
                continue;
            }
            seen.push(s);
        }
        match s {
            FxSound::Play(id) => queue.play(id),
            FxSound::Object(id) => queue.play_object(id),
            FxSound::Loop(id) => queue.play_loop(id),
            FxSound::Stop(id) => queue.stop(id),
        }
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
    if std::mem::take(&mut queue.stop_music) {
        stop_music(&mut commands, &mut audio, None);
    }
    if let Some(name) = queue.music.take() {
        play_music(&mut commands, &mut audio, &mut sources, &name);
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
        // Single channel, bound to an object or looping: if already playing, nothing.
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

/// On map change: effects and loops of the previous map are stopped (`StopInactiveAmbientSounds`).
pub fn reset(mut commands: Commands, audio: Option<ResMut<Audio>>) {
    let Some(mut audio) = audio else { return };
    for (_, ch) in audio.channels.drain() {
        for e in ch.entities.into_iter().flatten() {
            commands.entity(e).try_despawn();
        }
    }
    audio.loops.clear();
}

/// `PlayMp3(name)`: if that track is already playing, nothing; if another is playing, switch.
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

/// `StopMp3(name)` (or any if `None`).
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

/// Map ambience and music depending on where the character is (every frame, like the original).
fn world_sounds(
    mut commands: Commands,
    args: Res<Args>,
    audio: Option<ResMut<Audio>>,
    terrain: Option<Res<LoadedTerrain>>,
    players: Query<&Player>,
    mut queue: ResMut<SoundQueue>,
    mut sources: ResMut<Assets<AudioSource>>,
    mut rng: Local<u32>,
    leaves: Option<Res<crate::leaves::Leaves>>,
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

    // `PlayWorldAmbientSounds` (ZzzScene.cpp:2484-2560).
    match world {
        0 => {
            if tile == 4 {
                queue.stop("SOUND_WIND01");
                queue.stop("SOUND_RAIN01");
            } else {
                queue.play_loop("SOUND_WIND01");
                // `RainCurrent > 0`: the rain from leaves.rs (sent by the server with the weather).
                if leaves.as_ref().is_some_and(|l| l.rain_current > 0) {
                    queue.play_loop("SOUND_RAIN01");
                }
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
            // `rand_fps_check(512)` per tick: here per frame, at 25 ticks per second.
            if chance(512 * 60 / 25) {
                queue.play("SOUND_FOREST01");
            }
        }
        4 => queue.play_loop("SOUND_TOWER01"),
        7 => queue.play_loop("SOUND_WATER01"),
        8 => queue.play_loop("SOUND_DESERT01"),
        10 => queue.play_loop("SOUND_HEAVEN01"),
        // Raklion (boss room).
        58 => queue.play_loop("SOUND_WIND01"),
        // Loren Market: wind and rain always.
        79 => {
            queue.play_loop("SOUND_WIND01");
            queue.play_loop("SOUND_RAIN01");
        }
        // Karutan (`ASG_ADD_MAP_KARUTAN`).
        80 => queue.play_loop("SOUND_KARUTAN_DESERT_ENV"),
        81 => {
            if tile == 12 {
                queue.stop("SOUND_KARUTAN_DESERT_ENV");
                queue.play_loop("SOUND_KARUTAN_KARDAMAHAL_ENV");
            } else {
                queue.stop("SOUND_KARUTAN_KARDAMAHAL_ENV");
                queue.play_loop("SOUND_KARUTAN_DESERT_ENV");
            }
        }
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
        // The `PlayBGM` of each map (ZzzScene.cpp:2697-2754), in their default state (no
        // server events: Crywolf and Kanturu 3 before the battle, Illusion Temple waiting).
        31 => Some("huntingground"),
        33 => Some("Aida"),
        34 => Some("crywolf1st"),
        37 => Some("kanturu_1st"),
        38 => Some("kanturu_2nd"),
        39 => Some("KanturuTower"),
        41 => Some("BalgasBarrack"),
        42 => Some("BalgasRefuge"),
        45..=50 => Some("cursedtemplewait"),
        51 => Some("elbeland"),
        56 => Some("SwampOfCalmness"),
        57 => Some("Raklion"),
        62 => Some("Santa_Village"),
        63 => Some("PK_Field"),
        64 => Some("DuelArena"),
        65..=68 => Some("iDoppelganger"),
        69..=72 => Some("ImperialGuardianFort"),
        80 => Some("Karutan_A"),
        81 => Some("Karutan_B"),
        _ => None,
    };
    if let Some(track) = track {
        play_music(&mut commands, &mut audio, &mut sources, track);
    }
}

/// `PlayWalkSound`: the footstep by map, tile and safe zone.
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

/// Applies the requested volume to what is already playing and saves it.
fn apply_volume(
    audio: Option<ResMut<Audio>>,
    mut queue: ResMut<SoundQueue>,
    mut sinks: Query<(Entity, &mut AudioSink, Has<Music>)>,
    mut commands: Commands,
) {
    let Some(mut audio) = audio else {
        queue.set_sfx = None;
        queue.set_music = None;
        return;
    };
    if let Some(level) = queue.set_sfx.take()
        && level != audio.sfx_volume
    {
        audio.sfx_volume = level;
        crate::settings::save_value("volumen_sonido", &audio.sfx_volume.to_string());
        info!("volumen de los efectos: {}/10", audio.sfx_volume);
        for (e, mut sink, music) in &mut sinks {
            if !music && audio.loops.contains(&e) {
                sink.set_volume(volume(audio.sfx_volume));
            }
        }
    }
    if let Some(level) = queue.set_music.take()
        && level != audio.music_volume
    {
        audio.music_volume = level;
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
