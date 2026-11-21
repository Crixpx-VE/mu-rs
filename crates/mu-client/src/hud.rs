//! Información en pantalla. De momento, el contador de fps (arriba a la izquierda).
//! `F3` lo muestra u oculta.

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .add_systems(Startup, spawn_fps)
            .add_systems(Update, (update_fps, toggle_fps));
    }
}

#[derive(Component)]
struct FpsText;

fn spawn_fps(mut commands: Commands) {
    commands.spawn((
        FpsText,
        Text::new("FPS --"),
        TextFont { font_size: bevy::text::FontSize::Px(16.0), ..default() },
        TextColor(Color::srgb(1.0, 0.95, 0.6)),
        TextShadow::default(),
        Node { position_type: PositionType::Absolute, top: Val::Px(6.0), left: Val::Px(8.0), ..default() },
    ));
}

/// Se refresca 4 veces por segundo con la media de los últimos fotogramas, para que se pueda leer.
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

fn toggle_fps(keys: Res<ButtonInput<KeyCode>>, mut q: Query<&mut Visibility, With<FpsText>>) {
    if keys.just_pressed(KeyCode::F3) {
        for mut v in &mut q {
            v.toggle_visible_hidden();
        }
    }
}
