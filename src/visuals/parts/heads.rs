//! Cylinder head blocks — one for inline, one per bank for V/Flat, four for W.

use bevy::prelude::*;

use crate::engine::EngineLayout;
use crate::visuals::EngineVisual;

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
    let head_y = ctx.head_y;
    let head_width = ctx.head_width;

    let head_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.20, 0.22),
        metallic: 0.4, perceptual_roughness: 0.65, ..default()
    });

    match cfg.layout {
        EngineLayout::Inline => {
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.16 * s));
            commands.spawn((EngineVisual, Name::new("Inline Head"), PbrBundle {
                mesh: head_mesh,
                material: head_mat,
                transform: Transform::from_xyz(0.0, head_y, 0.0),
                ..default()
            })).set_parent(groups.heads);
        }
        EngineLayout::W { .. } => {
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.12 * s));
            for bank in 0..4usize {
                let tilt = cfg.cyl_bank_tilt(bank);
                let pos = tilt_position(0.0, head_y, 0.0, tilt);
                let bank_name = ["A", "B", "C", "D"][bank];
                commands.spawn((
                    EngineVisual,
                    Name::new(format!("Bank {} Head", bank_name)),
                    PbrBundle {
                        mesh: head_mesh.clone(),
                        material: head_mat.clone(),
                        transform: Transform::from_translation(pos)
                            .with_rotation(Quat::from_rotation_x(tilt)),
                        ..default()
                    },
                )).set_parent(groups.heads);
            }
        }
        _ => {
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.14 * s));
            for bank in 0..2 {
                let tilt = if bank == 0 { cfg.bank_angle * 0.5 } else { -cfg.bank_angle * 0.5 };
                let pos = tilt_position(0.0, head_y, 0.0, tilt);
                let bank_name = if bank == 0 { "A" } else { "B" };
                commands.spawn((
                    EngineVisual,
                    Name::new(format!("Bank {} Head", bank_name)),
                    PbrBundle {
                        mesh: head_mesh.clone(),
                        material: head_mat.clone(),
                        transform: Transform::from_translation(pos)
                            .with_rotation(Quat::from_rotation_x(tilt)),
                        ..default()
                    },
                )).set_parent(groups.heads);
            }
        }
    }
}
