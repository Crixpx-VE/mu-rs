//! MU Online client in Rust.
//!
//! Usage:
//!   cargo run -p mu-client -- [options]
//!
//!   --data <path>            client Data folder (or the MU_DATA variable)
//!   --world <N>              map to load (1 = Lorencia, 2 = Dungeon, 3 = Devias, 4 = Noria...)
//!   --spawn x,y              tile where the character appears
//!   --walk-to x,y            walk order at startup (for testing without a mouse);
//!                            with --screenshot it is given 1 s before capturing
//!   --cam x,y,h,tx,ty,th     free camera at that position, looking at that point (in tiles)
//!   --screenshot <png>       takes a capture and quits
//!   --screenshot-delay <s>   seconds to wait before capturing (default 10)
//!   --zoom <tiles>           initial follow camera distance (10-17, default 13)
//!   --sin-vsync              no fps cap (for performance measurement)
//!   --invasion <1|3>         start with the dragon invasion (1 = red, 3 = golden)
//!   --resolucion <WxH>       window size for this run only (otherwise `resolucion=` from
//!                            mu-launcher.txt, or 1024x768)
//!   --fps                    frame counter
//!   --inventario-prueba      test inventory: items +0 to +15, excellent and one ancient,
//!                            to view the inventory (V or I) and the glow
//!   --monstruos-prueba       some of the map's monsters and NPCs next to the character, cycling actions
//!
//! Controls, as in the original: left click = walk (held, follows the mouse) · 1-0 = select
//! skill · right click = use it · Esc = system menu · U = windows menu.

mod account;
mod boids;
mod camera;
mod char_info;
mod char_stats;
mod character;
mod charselect;
mod chat;
mod chat_log;
mod clipboard;
mod buffs;
mod combat;
mod cursor;
mod effects;
mod fish;
mod fonts;
mod fx;
mod gate;
mod grass;
mod ground;
mod hero_dress;
mod hud;
mod invasion;
mod inventory;
mod item_draw;
mod item_view;
mod leaves;
mod logfile;
mod login;
mod login_ui;
mod mainframe;
mod market_window;
mod minimap;
mod net;
mod model_light;
mod monsters;
mod move_window;
mod names;
mod notices;
mod npc;
mod npc_dialogue;
mod objects;
mod operate;
mod options;
mod overlay;
mod remote;
mod settings;
mod shadow;
mod scene;
mod shop;
mod skills;
mod sound;
mod terrain;
mod vault;
mod water_terrain;

use std::path::PathBuf;

use bevy::prelude::*;

/// Command-line options.
#[derive(Resource, Clone)]
pub struct Args {
    pub data: PathBuf,
    pub world: u16,
    pub spawn: Option<[f32; 2]>,
    pub walk_to: Option<[f32; 2]>,
    /// If set, takes a capture at startup and quits (useful for testing unattended).
    pub screenshot: Option<PathBuf>,
    /// Seconds to wait before capturing.
    pub screenshot_delay: f32,
    /// Initial camera position: eye and look-at point, in MU tiles (x, y, height).
    pub cam: Option<[f32; 6]>,
    /// Initial follow camera distance (tiles).
    pub zoom: Option<f32>,
    /// No vertical sync: fps are not capped to the monitor refresh rate (for measuring).
    pub no_vsync: bool,
    /// Start with the dragon invasion active (1 = red, 3 = golden).
    pub invasion: u8,
    /// Window resolution for this run only (otherwise the saved one or 1024×768).
    pub resolution: Option<(u32, u32)>,
    /// Casts every skill one after another (for testing without a keyboard).
    pub skill_test: bool,
    /// `--world`: go straight into the game on that map, without title or login.
    pub direct: bool,
    /// `--ver-titulo`: the title screen stays half-way (to check it in a capture).
    pub hold_title: bool,
    /// `--fps`: frame counter at the top left.
    pub show_fps: bool,
    /// `--salir-tras <s>`: quits after `s` seconds (for measuring: the trace is written in full).
    pub quit_after: Option<f32>,
    /// `--inventario-prueba`: the test inventory (inventory.rs).
    pub test_inventory: bool,
    /// `--monstruos-prueba`: some of the map's monsters around the hero, cycling actions.
    pub monster_test: bool,
}

fn parse_floats<const N: usize>(s: Option<String>, what: &str) -> [f32; N] {
    let v: Vec<f32> = s
        .unwrap_or_else(|| panic!("falta el valor de {what}"))
        .split(',')
        .map(|n| n.trim().parse().unwrap_or_else(|_| panic!("{what}: número inválido")))
        .collect();
    v.try_into().unwrap_or_else(|_| panic!("{what} necesita {N} números"))
}

/// `Data` next to the executable (packaged build), then `./Data`, otherwise the references' one.
fn default_data_dir() -> PathBuf {
    let next_to_exe = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("Data")));
    [next_to_exe, Some(PathBuf::from("Data"))]
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
        .unwrap_or_else(|| PathBuf::from("../referencias/MuMain/src/bin/Data"))
}

impl Args {
    fn parse() -> Self {
        let mut args = Args {
            data: std::env::var_os("MU_DATA").map(PathBuf::from).unwrap_or_else(default_data_dir),
            world: 1,
            spawn: None,
            walk_to: None,
            screenshot: None,
            screenshot_delay: 10.0,
            cam: None,
            zoom: None,
            no_vsync: false,
            invasion: 0,
            resolution: None,
            skill_test: false,
            direct: false,
            hold_title: false,
            show_fps: false,
            quit_after: None,
            test_inventory: false,
            monster_test: false,
        };
        let mut it = std::env::args().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--data" => args.data = it.next().expect("--data <ruta>").into(),
                "--world" => {
                    args.world = it.next().expect("--world <N>").parse().expect("número");
                    args.direct = true;
                }
                "--spawn" => args.spawn = Some(parse_floats(it.next(), "--spawn")),
                "--walk-to" => args.walk_to = Some(parse_floats(it.next(), "--walk-to")),
                "--screenshot" => args.screenshot = Some(it.next().expect("--screenshot <png>").into()),
                "--screenshot-delay" => {
                    args.screenshot_delay = it.next().expect("--screenshot-delay <s>").parse().expect("número")
                }
                "--cam" => args.cam = Some(parse_floats(it.next(), "--cam")),
                "--zoom" => args.zoom = Some(it.next().expect("--zoom <casillas>").parse().expect("número")),
                "--sin-vsync" => args.no_vsync = true,
                "--probar-habilidades" => args.skill_test = true,
                "--ver-titulo" => args.hold_title = true,
                "--fps" => args.show_fps = true,
                "--inventario-prueba" => args.test_inventory = true,
                "--monstruos-prueba" => args.monster_test = true,
                "--salir-tras" => args.quit_after = it.next().and_then(|v| v.parse().ok()),
                "--resolucion" => {
                    args.resolution = it.next().as_deref().and_then(settings::parse_resolution);
                    if args.resolution.is_none() {
                        eprintln!("--resolucion: usa algo como 1024x768");
                    }
                }
                "--invasion" => args.invasion = it.next().and_then(|v| v.parse().ok()).unwrap_or(1),
                otro => eprintln!("argumento desconocido: {otro}"),
            }
        }
        args
    }
}

/// Background color of each map: bytes / 256. It is the one set right before each frame's `glClear`
/// (`RenderScene`, ZzzScene.cpp:2347-2396 in Main 5.2). Lorencia's green and Devias' blue from
/// `RenderMainScene` (2059-2066) are set after `glClear` and overwritten by the next frame before
/// clearing, so they never show: those maps have a black background. (MuMain, with its
/// `SetWorldClearColor`, merged them and does paint them, but the S6 client does not.)
pub fn world_clear_color(world: u16) -> Color {
    let rgb8 = |r: u8, g: u8, b: u8| Color::srgb(r as f32 / 256.0, g as f32 / 256.0, b as f32 / 256.0);
    // The original's `WorldActive`: our map number minus one.
    match world as i32 - 1 {
        10 => rgb8(3, 25, 44),                // Heaven: blue
        24..=29 | 36 => rgb8(30, 40, 40),     // Kalima: dark teal
        45..=50 => rgb8(9, 8, 33),            // Cursed Temple: dark purple
        51 => rgb8(178, 178, 178),            // Elbeland: gray
        65 => rgb8(148, 179, 223),            // Doppelganger: light blue
        _ => Color::BLACK,                    // everything else, Lorencia and Devias included: black
    }
}

fn main() {
    let args = Args::parse();
    let title = format!("MU Rust - World{}", args.world);
    let resolution = args.resolution.or_else(settings::saved_resolution).unwrap_or(settings::DEFAULT_RESOLUTION);
    let headless = args.screenshot.is_some();

    let mut app = App::new();
    app.insert_resource(ClearColor(world_clear_color(args.world)));
    if headless {
        // Capture mode: no window. The camera renders to an in-memory image
        // (works without a display, e.g. a disconnected remote session).
        app.add_plugins(
            DefaultPlugins
                .set(logfile::plugin())
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: bevy::window::ExitCondition::DontExit,
                    ..default()
                })
                .disable::<bevy::winit::WinitPlugin>(),
        )
        .add_plugins(bevy::app::ScheduleRunnerPlugin::run_loop(std::time::Duration::from_secs_f64(1.0 / 30.0)));
    } else {
        app.insert_resource(bevy::winit::WinitSettings::continuous()).add_plugins(DefaultPlugins.set(logfile::plugin()).set(
            WindowPlugin {
                primary_window: Some(Window {
                    title,
                    // Fixed size, like the original: the resolution comes from the settings, no stretching.
                    resolution: settings::window_resolution(resolution),
                    resizable: false,
                    enabled_buttons: bevy::window::EnabledButtons { maximize: false, ..default() },
                    present_mode: if args.no_vsync {
                        bevy::window::PresentMode::AutoNoVsync
                    } else {
                        bevy::window::PresentMode::AutoVsync
                    },
                    ..default()
                }),
                ..default()
            },
        ));
    }

    app.insert_resource(args)
        .add_plugins((
            terrain::TerrainPlugin,
            grass::GrassPlugin,
            effects::EffectsPlugin,
            fx::FxPlugin,
            boids::BoidsPlugin,
            invasion::InvasionPlugin,
            leaves::LeavesPlugin,
            hud::HudPlugin,
            mainframe::MainFramePlugin,
            settings::SettingsPlugin,
            objects::ObjectsPlugin,
            character::CharacterPlugin,
            camera::FlyCameraPlugin,
        ))
        .add_plugins(fonts::FontsPlugin)
        .add_systems(Update, |args: Res<Args>, time: Res<Time<Real>>, mut exit: MessageWriter<AppExit>| {
            if args.quit_after.is_some_and(|s| time.elapsed_secs() >= s) {
                exit.write(AppExit::Success);
            }
        })
        .add_plugins(overlay::OverlayPlugin)
        .add_plugins((model_light::ModelLightPlugin, logfile::LogFilePlugin, shadow::ShadowPlugin, sound::SoundPlugin, scene::ScenePlugin, login::LoginPlugin, charselect::CharSelectPlugin, login_ui::LoginUiPlugin, options::OptionsPlugin, cursor::CursorPlugin, operate::OperatePlugin))
        .add_plugins((net::NetPlugin, remote::RemotePlugin, fish::FishPlugin, water_terrain::WaterTerrainPlugin))
        .add_plugins(minimap::MiniMapPlugin)
        .add_plugins(move_window::MoveWindowPlugin)
        .add_plugins(gate::GatePlugin)
        .add_plugins(names::NamesPlugin)
        .add_plugins(chat::ChatPlugin)
        .add_plugins(char_info::CharInfoPlugin)
        .add_plugins((item_view::ItemViewPlugin, inventory::InventoryPlugin, hero_dress::HeroDressPlugin))
        .add_plugins(monsters::MonstersPlugin)
        .add_plugins(buffs::BuffsPlugin)
        .add_plugins(npc::NpcPlugin)
        .add_plugins(shop::ShopPlugin)
        .add_plugins((npc_dialogue::NpcDialoguePlugin, market_window::MarketWindowPlugin))
        .add_plugins(vault::VaultPlugin)
        .add_plugins(skills::SkillsPlugin)
        .add_plugins(combat::CombatPlugin)
        .add_plugins(ground::GroundPlugin)
        .run();
}
