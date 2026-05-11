//! Per-cylinder intake + exhaust valves: stem (static) + head (animated lift).

use bevy::prelude::*;

use crate::visuals::{EngineVisual, Valve, ValveKind};

use super::{tilt_position, BuildCtx, Groups};

pub fn spawn(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    ctx: &BuildCtx,
    groups: &Groups,
) {
    let cfg = &ctx.core.config;
    let s = ctx.s;
    let rod_length = cfg.rod_length;

    let valve_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.84, 0.88),
        metallic: 0.95, perceptual_roughness: 0.18, ..default()
    });
    let valve_head_mesh = meshes.add(Cylinder::new(0.017 * s, 0.008 * s));
    let valve_stem_mesh = meshes.add(Cylinder::new(0.004 * s, 0.06 * s));

    for i in 0..cfg.num_cylinders {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        let valve_seat_y = rod_length * s + 0.13 * s;

        for (kind, z_local) in [(ValveKind::Intake, -0.022 * s), (ValveKind::Exhaust, 0.022 * s)] {
            let stem_pos = tilt_position(x, valve_seat_y + 0.045 * s, z_local, tilt);
            let head_pos = tilt_position(x, valve_seat_y, z_local, tilt);

            let kind_name = match kind {
                ValveKind::Intake => "Intake",
                ValveKind::Exhaust => "Exhaust",
            };

            commands.spawn((
                EngineVisual,
                Name::new(format!("{} Stem {}", kind_name, i + 1)),
                PbrBundle {
                    mesh: valve_stem_mesh.clone(),
                    material: valve_mat.clone(),
                    transform: Transform::from_translation(stem_pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                },
            )).set_parent(groups.valves);

            commands.spawn((
                EngineVisual,
                Name::new(format!("{} Head {}", kind_name, i + 1)),
                Valve { cyl: i, kind, seat_y: valve_seat_y, z_local, bank_tilt: tilt },
                PbrBundle {
                    mesh: valve_head_mesh.clone(),
                    material: valve_mat.clone(),
                    transform: Transform::from_translation(head_pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                },
            )).set_parent(groups.valves);
        }
    }
}
