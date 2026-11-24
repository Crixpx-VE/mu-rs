//! On-screen info for testing: the fps counter (top left), only with
//! `--fps` (the original has none).

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_fps.run_if(|args: Res<crate::Args>| args.show_fps))
            .add_systems(Update, update_fps);
    }
}

#[derive(Component)]
struct FpsText;

fn spawn_fps(mut commands: Commands) {
    commands.spawn((
        FpsText,
        Text::new("FPS --"),
        // The UI is scaled to 480 high (see mainframe.rs): 8 here is ~18 px at 1080p.
        TextFont { font_size: bevy::text::FontSize::Px(8.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.6)),
        TextShadow { offset: Vec2::splat(0.6), color: Color::BLACK },
        Node { position_type: PositionType::Absolute, top: Val::Px(3.0), left: Val::Px(4.0), ..default() },
    ));
}

/// Refreshed 4 times per second with the average of the last frames, so it is readable.
fn update_fps(
    diagnostics: Res<DiagnosticsStore>,
    time: Res<Time>,
    mut since: Local<f32>,
    mut texts: Query<&mut Text, With<FpsText>>,
) {
    *since += time.delta_secs();
    if *since < 0.25 {
        return;
    }
    *since = 0.0;
    let fps = diagnostics.get(&FrameTimeDiagnosticsPlugin::FPS).and_then(|d| d.smoothed());
    let ms = diagnostics.get(&FrameTimeDiagnosticsPlugin::FRAME_TIME).and_then(|d| d.smoothed());
    for mut t in &mut texts {
        t.0 = match (fps, ms) {
            (Some(f), Some(ms)) => format!("FPS {f:.0}  ({ms:.1} ms)"),
            _ => "FPS --".into(),
        };
    }
}
