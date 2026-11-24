//! The launcher window (egui): background art from the player's own client data, the update
//! progress, the server status, the resolution and the PLAY button.
//!
//! No game image is shipped with the launcher: the background (`Interface/LSBg01-04`, the original
//! loading screens) and the logo (`Logo/MU-logo.OZT`) are read from the `Data` folder once it has
//! been downloaded; until then a plain gradient is drawn.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontFamily, FontId, Mesh, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle,
    pos2, vec2,
};

use crate::update::{Server, Shared};

const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 520.0;
const GITHUB: &str = "https://github.com/Crixpx-VE/mu-rs";

/// Window modes of the original client (`Winmain.cpp:1148-1153`).
const RESOLUTIONS: [(u32, u32); 5] = [(640, 480), (800, 600), (1024, 768), (1280, 1024), (1600, 1200)];
/// The client's default (`settings::DEFAULT_RESOLUTION`).
const DEFAULT_RESOLUTION: (u32, u32) = (1024, 768);

// Palette: dark leather and gold.
const GOLD: Color32 = Color32::from_rgb(0xE8, 0xC1, 0x6A);
const GOLD_DARK: Color32 = Color32::from_rgb(0xA8, 0x7A, 0x2C);
const GOLD_EDGE: Color32 = Color32::from_rgba_premultiplied(0x8A, 0x6A, 0x2F, 0xC0);
const TEXT: Color32 = Color32::from_rgb(0xF2, 0xEC, 0xE0);
const TEXT_DIM: Color32 = Color32::from_rgb(0xB8, 0xA8, 0x8A);
const PANEL: Color32 = Color32::from_rgba_premultiplied(10, 8, 6, 215);
const GREEN: Color32 = Color32::from_rgb(0x5C, 0xD6, 0x7A);
const AMBER: Color32 = Color32::from_rgb(0xF0, 0xB4, 0x3C);
const RED: Color32 = Color32::from_rgb(0xE0, 0x55, 0x4A);
const GREY: Color32 = Color32::from_rgb(0x9A, 0x96, 0x90);

/// What happens with the game.
#[derive(Clone, Debug, PartialEq)]
enum Game {
    Idle,
    Running,
    Failed(String),
}

pub struct Launcher {
    dir: PathBuf,
    url: Option<String>,
    args: Vec<String>,
    status: Shared,
    game: Arc<Mutex<Game>>,
    background: Option<TextureHandle>,
    logo: Option<TextureHandle>,
    /// Art loading is retried until the data is there (the first run downloads it).
    art_tried_ready: bool,
    resolution: (u32, u32),
    /// Started with game options (the "Jugar - ..." shortcuts): play as soon as it is ready.
    auto_play: bool,
}

pub fn run(dir: PathBuf, url: Option<String>, args: Vec<String>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MU Rust")
            .with_inner_size([WIDTH, HEIGHT])
            .with_resizable(false)
            .with_maximize_button(false),
        centered: true,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            wgpu_setup: eframe::egui_wgpu::WgpuSetup::CreateNew(eframe::egui_wgpu::WgpuSetupCreateNew {
                native_adapter_selector: Some(Arc::new(pick_adapter)),
                ..Default::default()
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    eframe::run_native(
        "MU Rust",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            setup_visuals(&cc.egui_ctx);
            let status: Shared = Arc::default();
            {
                let (dir, url, args, status, ctx) = (dir.clone(), url.clone(), args.clone(), status.clone(), cc.egui_ctx.clone());
                std::thread::spawn(move || {
                    crate::update::run(&dir, url, &args, &status);
                    ctx.request_repaint();
                });
            }
            let resolution = saved_value(&dir, "resolucion")
                .and_then(|v| parse_resolution(&v))
                .unwrap_or(DEFAULT_RESOLUTION);
            let auto_play = !args.is_empty();
            let mut app = Launcher {
                dir,
                url,
                args,
                status,
                game: Arc::new(Mutex::new(Game::Idle)),
                background: None,
                logo: None,
                art_tried_ready: false,
                resolution,
                auto_play,
            };
            app.load_art(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    )
}

/// The best adapter that can draw to the window: a real GPU if there is one, otherwise the
/// software one (WARP on Windows), so the launcher also opens on machines without a GPU, which
/// wgpu's default request refuses.
fn pick_adapter(adapters: &[eframe::wgpu::Adapter], surface: Option<&eframe::wgpu::Surface<'_>>) -> Result<eframe::wgpu::Adapter, String> {
    use eframe::wgpu::DeviceType;
    let rank = |t: DeviceType| match t {
        DeviceType::DiscreteGpu => 0,
        DeviceType::IntegratedGpu => 1,
        DeviceType::VirtualGpu => 2,
        DeviceType::Other => 3,
        DeviceType::Cpu => 4,
    };
    adapters
        .iter()
        .filter(|a| surface.is_none_or(|s| a.is_surface_supported(s)))
        .min_by_key(|a| rank(a.get_info().device_type))
        .cloned()
        .ok_or_else(|| {
            let seen: Vec<String> = adapters
                .iter()
                .map(|a| {
                    let i = a.get_info();
                    let ok = surface.is_none_or(|s| a.is_surface_supported(s));
                    format!("{} ({:?}, {:?}, ventana: {ok})", i.name, i.backend, i.device_type)
                })
                .collect();
            format!("no hay ningún adaptador gráfico que pueda dibujar la ventana; vistos: [{}]", seen.join("; "))
        })
}

/// Segoe UI from Windows when available: cleaner than the built-in font.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windows = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| "C:\\Windows".into()).join("Fonts");
    let mut add = |key: &str, file: &str| -> bool {
        match std::fs::read(windows.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), Arc::new(egui::FontData::from_owned(bytes)));
                true
            }
            Err(_) => false,
        }
    };
    let regular = add("segoe", "segoeui.ttf");
    let bold = add("segoe-bold", "segoeuib.ttf");
    if regular && let Some(list) = fonts.families.get_mut(&FontFamily::Proportional) {
        list.insert(0, "segoe".into());
    }
    let mut bold_list = vec![];
    if bold {
        bold_list.push("segoe-bold".to_string());
    }
    bold_list.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("bold".into()), bold_list);
    ctx.set_fonts(fonts);
}

/// Dark widgets with gold accents (the resolution list).
fn setup_visuals(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    let field = Color32::from_rgb(0x24, 0x1E, 0x17);
    v.window_fill = Color32::from_rgb(0x18, 0x14, 0x10);
    v.panel_fill = v.window_fill;
    v.extreme_bg_color = field;
    v.window_stroke = Stroke::new(1.0_f32, GOLD_EDGE);
    v.selection.bg_fill = GOLD_DARK;
    v.selection.stroke = Stroke::new(1.0_f32, GOLD);
    for (w, fill) in [
        (&mut v.widgets.inactive, field),
        (&mut v.widgets.hovered, Color32::from_rgb(0x33, 0x2A, 0x1E)),
        (&mut v.widgets.active, Color32::from_rgb(0x40, 0x33, 0x22)),
        (&mut v.widgets.open, Color32::from_rgb(0x33, 0x2A, 0x1E)),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.bg_stroke = Stroke::new(1.0_f32, GOLD_EDGE);
        w.fg_stroke = Stroke::new(1.0_f32, TEXT);
        w.corner_radius = CornerRadius::same(6);
    }
    // Always dark, whatever the Windows theme is.
    ctx.set_theme(egui::Theme::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, v);
}

fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

fn regular(size: f32) -> FontId {
    FontId::proportional(size)
}

impl Launcher {
    /// Background and logo from the client data, if present.
    fn load_art(&mut self, ctx: &egui::Context) {
        let data = self.dir.join("Data");
        if self.background.is_none() {
            self.background = ["LSBg01.OZJ", "LSBg02.OZJ", "LSBg03.OZJ", "LSBg04.OZJ"]
                .iter()
                .find_map(|name| texture(ctx, &data, &format!("Interface/{name}"), "background"));
        }
        if self.logo.is_none() {
            self.logo = texture(ctx, &data, "Logo/MU-logo.OZT", "logo");
        }
    }

    fn play(&mut self, ctx: &egui::Context) {
        *self.game.lock().unwrap() = Game::Running;
        save_value(&self.dir, "resolucion", &format!("{}x{}", self.resolution.0, self.resolution.1));
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        let (dir, url, args, game, ctx) = (self.dir.clone(), self.url.clone(), self.args.clone(), self.game.clone(), ctx.clone());
        std::thread::spawn(move || {
            let (slot, console) = crate::game::log_slot(&dir);
            let result = crate::game::run_client(&dir, &args, &slot, console);
            if let Some(url) = &url {
                crate::game::send_log(&dir, url, &slot);
            }
            match result {
                // The game closed normally: so does the launcher.
                Ok(true) => std::process::exit(0),
                Ok(false) => *game.lock().unwrap() = Game::Failed(format!("El juego se cerró con un error. Mira {}.", slot.game)),
                Err(e) => *game.lock().unwrap() = Game::Failed(e),
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        });
    }
}

impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_millis(100));
        self.dev_screenshot(ctx);
        let status = self.status.lock().map(|s| s.clone()).unwrap_or_default();
        let game = self.game.lock().unwrap().clone();
        if status.ready && !self.art_tried_ready {
            self.art_tried_ready = true;
            self.load_art(ctx);
        }
        if status.ready && self.auto_play && game == Game::Idle {
            self.auto_play = false;
            self.play(ctx);
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let full = ui.max_rect();
            let painter = ui.painter().clone();
            self.draw_background(&painter, full);

            // Logo (or the name) at the top left.
            let top = full.min + vec2(36.0, 28.0);
            match &self.logo {
                Some(logo) => {
                    let size = logo.size_vec2();
                    let h = 120.0;
                    let w = size.x / size.y * h;
                    painter.image(logo.id(), Rect::from_min_size(top - vec2(12.0, 12.0), vec2(w, h)), uv_full(), Color32::WHITE);
                }
                None => {
                    painter.text(top, Align2::LEFT_TOP, "MU", bold(64.0), GOLD);
                    painter.text(top + vec2(2.0, 70.0), Align2::LEFT_TOP, "R U S T", bold(18.0), TEXT_DIM);
                }
            }

            // Server status at the top right.
            server_pill(&painter, pos2(full.max.x - 28.0, full.min.y + 30.0), status.server);

            // The bottom panel.
            let panel = Rect::from_min_max(pos2(full.min.x + 24.0, full.max.y - 168.0), pos2(full.max.x - 24.0, full.max.y - 40.0));
            painter.rect_filled(panel, CornerRadius::same(12), PANEL);
            painter.rect_stroke(panel, CornerRadius::same(12), Stroke::new(1.0_f32, GOLD_EDGE), StrokeKind::Inside);

            let left = Rect::from_min_max(panel.min + vec2(22.0, 18.0), pos2(panel.max.x - 250.0, panel.max.y - 18.0));
            let (title, detail) = match &game {
                Game::Idle => (status.title.clone(), status.detail.clone()),
                Game::Running => ("Jugando".into(), "El lanzador se cierra solo cuando cierres el juego.".into()),
                Game::Failed(e) => ("El juego se cerró".into(), e.clone()),
            };
            let title = if title.is_empty() { "Preparando...".to_string() } else { title };
            painter.text(left.min, Align2::LEFT_TOP, title, bold(20.0), TEXT);
            let detail = elide(ui, &detail, left.width(), regular(13.0));
            painter.text(left.min + vec2(0.0, 32.0), Align2::LEFT_TOP, detail, regular(13.0), TEXT_DIM);
            let bar = Rect::from_min_size(pos2(left.min.x, left.max.y - 26.0), vec2(left.width() - 52.0, 10.0));
            // Ready without having measured anything (no server, offline): a full bar, no sweep.
            let progress = if status.ready { Some(status.progress.unwrap_or(1.0)) } else { status.progress };
            progress_bar(&painter, bar, progress, ui.input(|i| i.time));
            if let Some(p) = progress {
                painter.text(pos2(left.max.x, bar.center().y), Align2::RIGHT_CENTER, format!("{:.0}%", p * 100.0), bold(14.0), GOLD);
            }

            // PLAY button and the resolution under it.
            let button = Rect::from_min_size(pos2(panel.max.x - 222.0, panel.min.y + 20.0), vec2(200.0, 56.0));
            let enabled = status.ready && game != Game::Running;
            let response = ui.allocate_rect(button, if enabled { Sense::click() } else { Sense::hover() });
            play_button(&painter, button, enabled, response.hovered(), response.is_pointer_button_down_on());
            if enabled && response.clicked() {
                *self.game.lock().unwrap() = Game::Idle;
                self.play(ctx);
            }
            if enabled {
                response.on_hover_cursor(egui::CursorIcon::PointingHand);
            }

            let combo = Rect::from_min_size(pos2(button.min.x, button.max.y + 14.0), vec2(button.width(), 24.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(combo), |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Resolución").color(TEXT_DIM).font(regular(13.0)));
                    egui::ComboBox::from_id_salt("resolution")
                        .width(110.0)
                        .selected_text(format!("{} × {}", self.resolution.0, self.resolution.1))
                        .show_ui(ui, |ui| {
                            for r in RESOLUTIONS {
                                ui.selectable_value(&mut self.resolution, r, format!("{} × {}", r.0, r.1));
                            }
                        });
                });
            });

            // Footer.
            let footer_y = full.max.y - 20.0;
            painter.text(
                pos2(full.min.x + 28.0, footer_y),
                Align2::LEFT_CENTER,
                format!("mu-rs {}  ·  Proyecto de fans, sin relación con Webzen", env!("CARGO_PKG_VERSION")),
                regular(11.5),
                GREY,
            );
            let link = Rect::from_min_size(pos2(full.max.x - 90.0, footer_y - 9.0), vec2(64.0, 18.0));
            let link_response = ui.allocate_rect(link, Sense::click());
            let link_color = if link_response.hovered() { GOLD } else { GREY };
            painter.text(link.right_center(), Align2::RIGHT_CENTER, "GitHub ↗", regular(11.5), link_color);
            if link_response.clicked() {
                ctx.open_url(egui::OpenUrl::new_tab(GITHUB));
            }
            link_response.on_hover_cursor(egui::CursorIcon::PointingHand);
        });
    }
}

impl Launcher {
    /// Development aid: with `MU_LAUNCHER_SCREENSHOT=<png>` the window saves a capture of itself
    /// after about three seconds and closes (to check the look on machines without a desktop).
    fn dev_screenshot(&self, ctx: &egui::Context) {
        let Some(path) = std::env::var_os("MU_LAUNCHER_SCREENSHOT") else { return };
        let time = ctx.input(|i| i.time);
        if time > 3.0 && !ctx.data(|d| d.get_temp::<bool>(egui::Id::new("shot")).unwrap_or(false)) {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("shot"), true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let shot = ctx.input(|i| {
            i.raw.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = shot {
            let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
            let _ = image::save_buffer(&path, &rgba, image.size[0] as u32, image.size[1] as u32, image::ExtendedColorType::Rgba8);
            std::process::exit(0);
        }
    }

    /// The art scaled to cover the window, darkened at the top and the bottom so text reads well.
    fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        match &self.background {
            Some(bg) => {
                let size = bg.size_vec2();
                let scale = (rect.width() / size.x).max(rect.height() / size.y);
                let shown = vec2(rect.width() / (size.x * scale), rect.height() / (size.y * scale));
                let uv = Rect::from_center_size(pos2(0.5, 0.5), shown);
                painter.image(bg.id(), rect, uv, Color32::WHITE);
            }
            None => gradient(painter, rect, Color32::from_rgb(0x2A, 0x20, 0x16), Color32::from_rgb(0x07, 0x06, 0x08)),
        }
        let shade = Color32::from_black_alpha(110);
        gradient(painter, Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + 110.0)), shade, Color32::TRANSPARENT);
        gradient(painter, Rect::from_min_max(pos2(rect.min.x, rect.max.y - 260.0), rect.max), Color32::TRANSPARENT, Color32::from_black_alpha(230));
    }
}

fn uv_full() -> Rect {
    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0))
}

/// A vertical gradient from `top` to `bottom`.
fn gradient(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// A horizontal gradient from `left` to `right`.
fn gradient_h(painter: &egui::Painter, rect: Rect, left: Color32, right: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(mesh);
}

/// Rounded track with a gold fill; without a known progress, a light sweeping back and forth.
fn progress_bar(painter: &egui::Painter, rect: Rect, progress: Option<f32>, time: f64) {
    let radius = CornerRadius::same((rect.height() / 2.0) as u8);
    painter.rect_filled(rect, radius, Color32::from_rgb(0x2A, 0x23, 0x1A));
    match progress {
        Some(p) => {
            let w = rect.width() * p.clamp(0.0, 1.0);
            if w > 1.0 {
                let fill = Rect::from_min_size(rect.min, vec2(w.max(rect.height()), rect.height()));
                painter.rect_filled(fill, radius, GOLD_DARK);
                gradient_h(painter, fill.shrink2(vec2(rect.height() / 2.0, 0.0)), GOLD_DARK, GOLD);
            }
        }
        None => {
            let t = ((time * 0.8).sin() * 0.5 + 0.5) as f32;
            let w = rect.width() * 0.25;
            let x = rect.min.x + (rect.width() - w) * t;
            painter.rect_filled(Rect::from_min_size(pos2(x, rect.min.y), vec2(w, rect.height())), radius, GOLD_DARK);
        }
    }
}

fn play_button(painter: &egui::Painter, rect: Rect, enabled: bool, hovered: bool, pressed: bool) {
    let radius = CornerRadius::same(10);
    let rect = if pressed { rect.translate(vec2(0.0, 1.0)) } else { rect };
    if enabled {
        let (top, bottom) = if hovered {
            (Color32::from_rgb(0xFF, 0xDD, 0x8A), Color32::from_rgb(0xC0, 0x8C, 0x34))
        } else {
            (Color32::from_rgb(0xF0, 0xCA, 0x72), Color32::from_rgb(0xA8, 0x74, 0x26))
        };
        painter.rect_filled(rect, radius, bottom);
        gradient(painter, rect.shrink2(vec2(0.0, 6.0)), top, bottom);
        painter.rect_stroke(rect, radius, Stroke::new(1.5_f32, Color32::from_rgb(0xFF, 0xEB, 0xB0)), StrokeKind::Inside);
        painter.text(rect.center(), Align2::CENTER_CENTER, "JUGAR", bold(24.0), Color32::from_rgb(0x2A, 0x18, 0x06));
    } else {
        painter.rect_filled(rect, radius, Color32::from_rgb(0x3A, 0x35, 0x2E));
        painter.rect_stroke(rect, radius, Stroke::new(1.0_f32, Color32::from_rgb(0x55, 0x4E, 0x44)), StrokeKind::Inside);
        painter.text(rect.center(), Align2::CENTER_CENTER, "JUGAR", bold(24.0), Color32::from_rgb(0x7A, 0x72, 0x66));
    }
}

/// Coloured dot and text, right-aligned at `right`.
fn server_pill(painter: &egui::Painter, right: Pos2, server: Server) {
    let (color, text) = match server {
        Server::Connecting => (AMBER, "Conectando..."),
        Server::Online => (GREEN, "Servidor en línea"),
        Server::Offline => (RED, "Sin conexión"),
        Server::None => (GREY, "Modo local"),
    };
    let font = regular(13.0);
    let galley = painter.layout_no_wrap(text.into(), font, TEXT);
    let size = vec2(galley.size().x + 38.0, 28.0);
    let rect = Rect::from_min_size(pos2(right.x - size.x, right.y - size.y / 2.0), size);
    painter.rect_filled(rect, CornerRadius::same(14), Color32::from_black_alpha(150));
    painter.rect_stroke(rect, CornerRadius::same(14), Stroke::new(1.0_f32, GOLD_EDGE), StrokeKind::Inside);
    painter.circle_filled(pos2(rect.min.x + 16.0, rect.center().y), 4.5, color);
    painter.galley(pos2(rect.min.x + 28.0, rect.center().y - galley.size().y / 2.0), galley, TEXT);
}

/// Cuts `text` with "…" so it fits in `width`.
fn elide(ui: &egui::Ui, text: &str, width: f32, font: FontId) -> String {
    let fits = |s: &str| ui.painter().layout_no_wrap(s.to_string(), font.clone(), TEXT).size().x <= width;
    if fits(text) {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let s: String = chars.iter().collect::<String>() + "…";
        if fits(&s) {
            return s;
        }
    }
    String::new()
}

fn texture(ctx: &egui::Context, data: &Path, name: &str, id: &str) -> Option<TextureHandle> {
    let t = mu_data::texture::load(data, name).ok()?;
    let image = egui::ColorImage::from_rgba_unmultiplied([t.width as usize, t.height as usize], &t.rgba);
    Some(ctx.load_texture(id, image, egui::TextureOptions::LINEAR))
}

/// `1024x768` → (1024, 768), the same format the client reads.
fn parse_resolution(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once(['x', 'X', '*'])?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Value of `key=` in `mu-launcher.txt`.
fn saved_value(dir: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(crate::update::CONFIG_FILE)).ok()?;
    let prefix = format!("{key}=");
    text.lines().find_map(|l| l.trim().strip_prefix(&prefix).map(|v| v.trim().to_string()))
}

/// Saves `key=value` in `mu-launcher.txt`, leaving the other lines alone (like the client's
/// `settings::save_value`).
fn save_value(dir: &Path, key: &str, value: &str) {
    let path = dir.join(crate::update::CONFIG_FILE);
    let prefix = format!("{key}=");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines: Vec<String> = text.lines().filter(|l| !l.trim().starts_with(&prefix)).map(str::to_string).collect();
    lines.push(format!("{prefix}{value}"));
    let _ = std::fs::write(&path, lines.join("\r\n") + "\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolutions() {
        assert_eq!(parse_resolution("1280x1024"), Some((1280, 1024)));
        assert_eq!(parse_resolution(" 800 X 600 "), Some((800, 600)));
        assert_eq!(parse_resolution("grande"), None);
        assert!(RESOLUTIONS.contains(&DEFAULT_RESOLUTION));
    }

    #[test]
    fn config_lines_are_kept() {
        let dir = std::env::temp_dir().join(format!("mu-launcher-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(crate::update::CONFIG_FILE), "# comentario\nservidor=1.2.3.4\nresolucion=800x600\n").unwrap();
        save_value(&dir, "resolucion", "1280x1024");
        assert_eq!(saved_value(&dir, "resolucion").as_deref(), Some("1280x1024"));
        assert_eq!(saved_value(&dir, "servidor").as_deref(), Some("1.2.3.4"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
