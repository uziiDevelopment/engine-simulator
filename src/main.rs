//! Engine Crankshaft Simulator
//!
//! A combustion-driven inline-4 engine simulation.  The crankshaft is **not**
//! programmed to spin — it spins because compressed gas in each cylinder ignites,
//! pressure rises, force pushes the piston, and that force is carried through the
//! connecting rod to a tangential torque on the crank pin.  The crankshaft only
//! has rotational inertia; everything else is real thermodynamics.
//!
//! Top-level wiring only; physics lives in [`engine`], rendering in [`visuals`].

mod camera;
mod engine;
mod ui;
mod visuals;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Engine Simulator — combustion driven".into(),
                resolution: (1500., 920.).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(ClearColor(Color::srgb(0.04, 0.05, 0.07)))
        .insert_resource(AmbientLight { color: Color::WHITE, brightness: 70.0 })
        .add_plugins(engine::EnginePlugin)
        .add_plugins(visuals::VisualsPlugin)
        .add_plugins(camera::CameraPlugin)
        .add_plugins(ui::UiPlugin)
        .run();
}
