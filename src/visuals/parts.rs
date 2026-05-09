//! Dynamic scene assembly: spawns engine visual entities based on current config.
//! Called on startup and whenever the engine config changes.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::{EngineCore, VIS_SCALE};

use super::{
    CombustionFlash, ConRod, Crankshaft, CylinderGasViz, EngineVisual, ManifoldKind, ManifoldViz,
    Piston, Valve, ValveKind,
};

/// Spawns static scene elements (floor, lights) that don't depend on engine config.
pub fn setup_static_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.07, 0.075, 0.09),
        perceptual_roughness: 0.95, ..default()
    });

    // ── Floor ──────────────────────────────────────────────────────────────
    commands.spawn(PbrBundle {
        mesh: meshes.add(Plane3d::default().mesh().size(60.0, 60.0)),
        material: floor_mat,
        transform: Transform::from_xyz(0.0, -2.5, 0.0),
        ..default()
    });

    // ── Lighting ───────────────────────────────────────────────────────────
    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 9000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1_500_000.0,
            color: Color::srgb(1.0, 0.55, 0.35),
            range: 25.0,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.5, -3.5),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 800_000.0,
            color: Color::srgb(0.5, 0.7, 1.0),
            range: 22.0,
            ..default()
        },
        transform: Transform::from_xyz(-3.5, 3.5, 4.0),
        ..default()
    });
}

/// Spawns all engine visual entities dynamically based on the current config.
/// Every entity gets the [`EngineVisual`] marker for easy bulk despawn.
pub fn spawn_engine_visuals(
    commands: &mut Commands,
    core: &EngineCore,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
) {
    let cfg = &core.config;
    let s = VIS_SCALE;
    let num_cyl = cfg.num_cylinders;
    let crank_pos = cfg.crank_positions(); // number of throw positions along X

    // ── Materials ──────────────────────────────────────────────────────────
    let crank_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.13, 0.13),
        metallic: 0.7, perceptual_roughness: 0.32, ..default()
    });
    let piston_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.45, 0.85),
        metallic: 0.55, perceptual_roughness: 0.35, ..default()
    });
    let rod_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.78, 0.82),
        metallic: 0.85, perceptual_roughness: 0.22, ..default()
    });
    let flywheel_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        metallic: 0.9, perceptual_roughness: 0.45, ..default()
    });
    let valve_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.84, 0.88),
        metallic: 0.95, perceptual_roughness: 0.18, ..default()
    });
    let head_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.20, 0.22),
        metallic: 0.4, perceptual_roughness: 0.65, ..default()
    });

    // ── Shared Meshes ──────────────────────────────────────────────────────
    let crank_radius = cfg.crank_radius();
    let rod_length = cfg.rod_length;
    let bore = cfg.bore;
    let cyl_spacing = cfg.cylinder_spacing;

    let main_journal_mesh  = meshes.add(Cylinder::new(0.026 * s, 0.045 * s));
    let crank_pin_mesh     = meshes.add(Cylinder::new(0.024 * s, 0.060 * s));
    let counterweight_mesh = meshes.add(Cuboid::new(0.025 * s, 0.11 * s, 0.06 * s));
    let piston_mesh        = meshes.add(Cylinder::new(bore * 0.49 * s, 0.075 * s));
    let rod_mesh           = meshes.add(Cuboid::new(0.020 * s, rod_length * s, 0.028 * s));
    let bore_mesh          = meshes.add(Cylinder::new(bore * 0.55 * s, 0.18 * s));
    let flywheel_mesh      = meshes.add(Cylinder::new(0.135 * s, 0.030 * s));
    let output_shaft_mesh  = meshes.add(Cylinder::new(0.022 * s, 0.10 * s));
    let pulley_mesh        = meshes.add(Cylinder::new(0.060 * s, 0.025 * s));
    let valve_head_mesh    = meshes.add(Cylinder::new(0.017 * s, 0.008 * s));
    let valve_stem_mesh    = meshes.add(Cylinder::new(0.004 * s, 0.06 * s));
    let flash_mesh         = meshes.add(Sphere::new(bore * 0.42 * s));

    let crank_axis_rot = Quat::from_rotation_z(PI / 2.0);

    // ── Crankshaft (parent; children rotate with it) ───────────────────────
    let crank_entity = commands.spawn((
        EngineVisual, Crankshaft, SpatialBundle::default(),
    )).id();

    // Main bearing journals (one more than crank_pos)
    for i in 0..=crank_pos {
        let x = (i as f32 - crank_pos as f32 * 0.5) * cyl_spacing * s;
        commands.spawn((EngineVisual, PbrBundle {
            mesh: main_journal_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, 0.0, 0.0).with_rotation(crank_axis_rot),
            ..default()
        })).set_parent(crank_entity);
    }

    // Crank throws — one per crank position.  For V/Flat, the throw serves
    // two cylinders (paired on same X).  We use the phase of the first
    // cylinder at each position.
    for pos in 0..crank_pos {
        let cyl_idx = match cfg.layout {
            crate::engine::EngineLayout::Inline => pos,
            _ => pos * 2, // first cylinder of the pair
        };
        let x = cfg.cyl_visual_x(cyl_idx);
        let phi = cfg.crank_phases[cyl_idx];
        let pin_y = phi.cos() * crank_radius * s;
        let pin_z = phi.sin() * crank_radius * s;

        commands.spawn((EngineVisual, PbrBundle {
            mesh: crank_pin_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, pin_y, pin_z).with_rotation(crank_axis_rot),
            ..default()
        })).set_parent(crank_entity);

        for &dx in &[-0.034 * s, 0.034 * s] {
            commands.spawn((EngineVisual, PbrBundle {
                mesh: counterweight_mesh.clone(),
                material: crank_mat.clone(),
                transform: Transform::from_xyz(x + dx, pin_y * 0.5, pin_z * 0.5)
                    .with_rotation(Quat::from_rotation_x(phi)),
                ..default()
            })).set_parent(crank_entity);
        }
    }

    // Front pulley + flywheel + output shaft
    let half_len = (crank_pos as f32 * 0.5 + 0.5) * cyl_spacing * s;
    let front_x = -half_len;
    let rear_x  =  half_len;

    commands.spawn((EngineVisual, PbrBundle {
        mesh: pulley_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(front_x - 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity);

    commands.spawn((EngineVisual, PbrBundle {
        mesh: flywheel_mesh.clone(), material: flywheel_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity);

    // Yellow timing mark on the flywheel rim
    commands.spawn((EngineVisual, PbrBundle {
        mesh: meshes.add(Cuboid::new(0.012 * s, 0.025 * s, 0.045 * s)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.85, 0.1),
            emissive: LinearRgba::new(0.4, 0.35, 0.0, 1.0),
            ..default()
        }),
        transform: Transform::from_xyz(rear_x + 0.04 * s, 0.105 * s, 0.0),
        ..default()
    })).set_parent(crank_entity);

    commands.spawn((EngineVisual, PbrBundle {
        mesh: output_shaft_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.10 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity);

    // ── Per-cylinder visuals (pistons, rods, bores, valves, flash) ─────────
    for i in 0..num_cyl {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        // Initial piston Y position (at rest, theta=0)
        let y_p = cfg.piston_y(0.0, cfg.crank_phases[i]) * s;
        // Tilted position: piston moves along the bank axis
        let pos = tilt_position(x, y_p, 0.0, tilt);

        // ── Piston ─────────────────────────────────────────────────────────
        commands.spawn((
            EngineVisual,
            Piston { idx: i, bank_tilt: tilt },
            PbrBundle {
                mesh: piston_mesh.clone(),
                material: piston_mat.clone(),
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        ));

        // ── Connecting rod ─────────────────────────────────────────────────
        commands.spawn((
            EngineVisual,
            ConRod { idx: i, base_x: x, bank_tilt: tilt },
            PbrBundle {
                mesh: rod_mesh.clone(),
                material: rod_mat.clone(),
                transform: Transform::from_translation(pos * 0.5),
                ..default()
            },
        ));

        // ── Translucent cylinder bore ──────────────────────────────────────
        let bore_y = rod_length * s + 0.04 * s;
        let bore_pos = tilt_position(x, bore_y, 0.0, tilt);

        let bore_mat_handle = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.6, 0.7, 0.18),
            emissive: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.7,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });

        commands.spawn((
            EngineVisual,
            CylinderGasViz { idx: i, bore_material: bore_mat_handle.clone(), bank_tilt: tilt },
            PbrBundle {
                mesh: bore_mesh.clone(),
                material: bore_mat_handle,
                transform: Transform::from_translation(bore_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        ));

        // ── Combustion flash sphere ────────────────────────────────────────
        let flash_y = rod_length * s + 0.115 * s;
        let flash_pos = tilt_position(x, flash_y, 0.0, tilt);

        let flash_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.5, 0.2, 0.0),
            emissive: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        commands.spawn((
            EngineVisual,
            CombustionFlash { cyl: i, material: flash_mat.clone() },
            PbrBundle {
                mesh: flash_mesh.clone(),
                material: flash_mat,
                transform: Transform::from_translation(flash_pos),
                ..default()
            },
        ));

        // ── Valves: intake (−Z side) and exhaust (+Z side) relative to bank ─
        let valve_seat_y = rod_length * s + 0.13 * s;
        for (kind, z_local) in [(ValveKind::Intake, -0.022 * s), (ValveKind::Exhaust, 0.022 * s)] {
            let stem_pos = tilt_position(x, valve_seat_y + 0.045 * s, z_local, tilt);
            let head_pos = tilt_position(x, valve_seat_y, z_local, tilt);

            // Valve stem (static)
            commands.spawn((EngineVisual, PbrBundle {
                mesh: valve_stem_mesh.clone(),
                material: valve_mat.clone(),
                transform: Transform::from_translation(stem_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            }));

            // Valve head (animated)
            commands.spawn((
                EngineVisual,
                Valve { cyl: i, kind, seat_y: valve_seat_y, bank_tilt: tilt },
                PbrBundle {
                    mesh: valve_head_mesh.clone(),
                    material: valve_mat.clone(),
                    transform: Transform::from_translation(head_pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                },
            ));
        }
    }

    // ── Cylinder head block(s) ─────────────────────────────────────────────
    // For inline: one head block.  For V/Flat: one per bank.
    let head_y = rod_length * s + 0.18 * s;
    let head_width = (crank_pos as f32 * cyl_spacing + 0.08) * s;

    match cfg.layout {
        crate::engine::EngineLayout::Inline => {
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.16 * s));
            commands.spawn((EngineVisual, PbrBundle {
                mesh: head_mesh,
                material: head_mat.clone(),
                transform: Transform::from_xyz(0.0, head_y, 0.0),
                ..default()
            }));
        }
        _ => {
            // Two heads, one per bank, tilted
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.14 * s));
            for bank in 0..2 {
                let tilt = if bank == 0 { cfg.bank_angle * 0.5 } else { -cfg.bank_angle * 0.5 };
                let pos = tilt_position(0.0, head_y, 0.0, tilt);
                commands.spawn((EngineVisual, PbrBundle {
                    mesh: head_mesh.clone(),
                    material: head_mat.clone(),
                    transform: Transform::from_translation(pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                }));
            }
        }
    }

    // ── Intake / exhaust manifold runners ──────────────────────────────────
    let intake_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.55, 0.95),
        emissive: LinearRgba::new(0.05, 0.10, 0.20, 1.0),
        metallic: 0.4, perceptual_roughness: 0.45, ..default()
    });
    let exhaust_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.45, 0.2, 0.18),
        emissive: LinearRgba::new(0.1, 0.05, 0.02, 1.0),
        metallic: 0.5, perceptual_roughness: 0.55, ..default()
    });

    let runner_len = head_width;
    let runner_mesh = meshes.add(Cylinder::new(0.030 * s, runner_len));

    match cfg.layout {
        crate::engine::EngineLayout::Inline => {
            // Intake on −Z, exhaust on +Z
            commands.spawn((
                EngineVisual,
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: intake_mat,
                    transform: Transform::from_xyz(0.0, head_y + 0.08 * s, -0.10 * s)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            ));
            commands.spawn((
                EngineVisual,
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh, material: exhaust_mat,
                    transform: Transform::from_xyz(0.0, head_y + 0.08 * s, 0.10 * s)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            ));
        }
        _ => {
            // V / Flat: intake in the valley (center), exhaust on outer sides
            let intake_pos = Vec3::new(0.0, head_y + 0.06 * s, 0.0);
            commands.spawn((
                EngineVisual,
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: intake_mat,
                    transform: Transform::from_translation(intake_pos)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            ));
            // Exhaust runners on outer side of each bank
            let exh_offset = 0.14 * s;
            let tilt_a = cfg.bank_angle * 0.5;
            let exh_pos_a = tilt_position(0.0, head_y + 0.06 * s, exh_offset, tilt_a);
            let exh_pos_b = tilt_position(0.0, head_y + 0.06 * s, -exh_offset, -tilt_a);

            let exhaust_mat_b = materials.add(StandardMaterial {
                base_color: Color::srgb(0.45, 0.2, 0.18),
                emissive: LinearRgba::new(0.1, 0.05, 0.02, 1.0),
                metallic: 0.5, perceptual_roughness: 0.55, ..default()
            });

            commands.spawn((
                EngineVisual,
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: exhaust_mat,
                    transform: Transform::from_translation(exh_pos_a)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            ));
            commands.spawn((
                EngineVisual,
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat_b.clone() },
                PbrBundle {
                    mesh: runner_mesh, material: exhaust_mat_b,
                    transform: Transform::from_translation(exh_pos_b)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            ));
        }
    }

    // ── Port tubes from head to runners ────────────────────────────────────
    let port_mesh = meshes.add(Cylinder::new(0.015 * s, 0.10 * s));
    for i in 0..num_cyl {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        for z_local in [-0.10 * s, 0.10 * s] {
            let pos = tilt_position(x, head_y + 0.04 * s, z_local, tilt);
            commands.spawn((EngineVisual, PbrBundle {
                mesh: port_mesh.clone(),
                material: head_mat.clone(),
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            }));
        }
    }
}

// ── Helper: compute world position given local (x, y_along_axis, z_local) and bank tilt ─
#[inline]
fn tilt_position(x: f32, y_local: f32, z_local: f32, tilt: f32) -> Vec3 {
    // tilt rotates the cylinder axis from +Y toward +Z (for positive tilt).
    // y_local is distance along the cylinder axis, z_local is lateral offset.
    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    Vec3::new(
        x,
        y_local * cos_t - z_local * sin_t,
        y_local * sin_t + z_local * cos_t,
    )
}
