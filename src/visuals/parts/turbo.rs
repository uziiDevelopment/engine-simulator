//! Turbocharger placement using the shared `turbo.glb` asset. The mount node
//! discovery + alignment lives in `animate::turbo`.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::VIS_SCALE;
use crate::visuals::{EngineVisual, TurboGlbRoot};

use super::BuildCtx;

pub fn spawn(commands: &mut Commands, asset_server: &AssetServer, ctx: &BuildCtx) {
    let cfg = &ctx.core.config;
    let enabled: Vec<usize> = cfg.turbos.iter()
        .enumerate()
        .filter(|(_, t)| t.enabled)
        .take(4)
        .map(|(i, _)| i)
        .collect();
    let total = enabled.len();
    for &idx in &enabled {
        spawn_single(commands, asset_server, ctx.head_y, ctx.head_width, idx, total);
    }
}

fn spawn_single(
    commands: &mut Commands,
    asset_server: &AssetServer,
    head_y: f32,
    head_width: f32,
    turbo_idx: usize,
    total_turbos: usize,
) {
    let s = VIS_SCALE;
    let (group_x, group_z, rotation_y) = match (total_turbos, turbo_idx) {
        (1, 0) => ((head_width * 0.5) + 0.45 * s, 0.0, 0.0_f32),
        (2, 0) => ((head_width * 0.5) + 0.40 * s, 0.25 * s, 0.0),
        (2, 1) => (-(head_width * 0.5) - 0.40 * s, 0.25 * s, PI),
        (3, 0) => ((head_width * 0.5) + 0.40 * s, 0.0, 0.0),
        (3, 1) => (-(head_width * 0.5) - 0.40 * s, 0.0, PI),
        (3, 2) => ((head_width * 0.5) + 0.40 * s, -0.30 * s, 0.0),
        (4, 0) => ((head_width * 0.5) + 0.35 * s, 0.25 * s, 0.0),
        (4, 1) => (-(head_width * 0.5) - 0.35 * s, 0.25 * s, PI),
        (4, 2) => ((head_width * 0.5) + 0.35 * s, -0.25 * s, 0.0),
        (4, 3) => (-(head_width * 0.5) - 0.35 * s, -0.25 * s, PI),
        _ => ((head_width * 0.5) + 0.45 * s + turbo_idx as f32 * 0.15 * s, 0.0, 0.0),
    };

    let group_y = head_y * 0.45;
    let turbo_scene: Handle<Scene> = asset_server.load("engine/turbo/turbo.glb#Scene0");
    const GLB_TO_SIM: f32 = 0.55;

    commands.spawn((
        EngineVisual,
        TurboGlbRoot { turbo_idx },
        Name::new(format!("Turbocharger {}", turbo_idx + 1)),
        SceneBundle {
            scene: turbo_scene,
            transform: Transform::from_xyz(group_x, group_y, group_z)
                .with_rotation(Quat::from_rotation_y(rotation_y))
                .with_scale(Vec3::splat(GLB_TO_SIM)),
            ..default()
        },
    ));
}
