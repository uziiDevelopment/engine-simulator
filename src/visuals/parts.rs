//! One-shot scene assembly: spawns every visual entity at startup.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::{
    cyl_x, BORE, CRANK_PHASES, CRANK_RADIUS, CYL_SPACING, NUM_CYL, ROD_LENGTH, VIS_SCALE,
};

use super::{
    CombustionFlash, ConRod, Crankshaft, CylinderGasViz, ManifoldKind, ManifoldViz, Piston,
    Valve, ValveKind,
};

pub fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let s = VIS_SCALE;

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
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.07, 0.075, 0.09),
        perceptual_roughness: 0.95, ..default()
    });

    // ── Meshes (shared) ────────────────────────────────────────────────────
    let main_journal_mesh   = meshes.add(Cylinder::new(0.026 * s, 0.045 * s));
    let crank_pin_mesh      = meshes.add(Cylinder::new(0.024 * s, 0.060 * s));
    let counterweight_mesh  = meshes.add(Cuboid::new(0.025 * s, 0.11 * s, 0.06 * s));
    let piston_mesh         = meshes.add(Cylinder::new(BORE * 0.49 * s, 0.075 * s));
    let rod_mesh            = meshes.add(Cuboid::new(0.020 * s, ROD_LENGTH * s, 0.028 * s));
    let bore_mesh           = meshes.add(Cylinder::new(BORE * 0.55 * s, 0.18 * s));
    let flywheel_mesh       = meshes.add(Cylinder::new(0.135 * s, 0.030 * s));
    let output_shaft_mesh   = meshes.add(Cylinder::new(0.022 * s, 0.10 * s));
    let pulley_mesh         = meshes.add(Cylinder::new(0.060 * s, 0.025 * s));
    let valve_head_mesh     = meshes.add(Cylinder::new(0.017 * s, 0.008 * s));
    let valve_stem_mesh     = meshes.add(Cylinder::new(0.004 * s, 0.06 * s));
    let head_mesh           = meshes.add(Cuboid::new(
        (NUM_CYL as f32 * CYL_SPACING + 0.08) * s,
        0.05 * s,
        0.16 * s,
    ));
    let flash_mesh          = meshes.add(Sphere::new(BORE * 0.42 * s));

    let crank_axis_rot = Quat::from_rotation_z(PI / 2.0);

    // ── Crankshaft (parent transform; children rotate together) ────────────
    let crank_entity = commands.spawn((Crankshaft, SpatialBundle::default())).id();

    // 5 main bearing journals
    for i in 0..=NUM_CYL {
        let x = (i as f32 - NUM_CYL as f32 * 0.5) * CYL_SPACING * s;
        commands.spawn(PbrBundle {
            mesh: main_journal_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, 0.0, 0.0).with_rotation(crank_axis_rot),
            ..default()
        }).set_parent(crank_entity);
    }

    // 4 throws (pin + 2 webs each)
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        let phi = CRANK_PHASES[i];
        let pin_y = phi.cos() * CRANK_RADIUS * s;
        let pin_z = phi.sin() * CRANK_RADIUS * s;

        commands.spawn(PbrBundle {
            mesh: crank_pin_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, pin_y, pin_z).with_rotation(crank_axis_rot),
            ..default()
        }).set_parent(crank_entity);

        for &dx in &[-0.034 * s, 0.034 * s] {
            commands.spawn(PbrBundle {
                mesh: counterweight_mesh.clone(),
                material: crank_mat.clone(),
                transform: Transform::from_xyz(x + dx, pin_y * 0.5, pin_z * 0.5)
                    .with_rotation(Quat::from_rotation_x(phi)),
                ..default()
            }).set_parent(crank_entity);
        }
    }

    // Front pulley + flywheel + output shaft
    let front_x = -2.5 * CYL_SPACING * s;
    let rear_x  =  2.5 * CYL_SPACING * s;

    commands.spawn(PbrBundle {
        mesh: pulley_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(front_x - 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    commands.spawn(PbrBundle {
        mesh: flywheel_mesh.clone(), material: flywheel_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    // Yellow timing mark on the flywheel rim
    commands.spawn(PbrBundle {
        mesh: meshes.add(Cuboid::new(0.012 * s, 0.025 * s, 0.045 * s)),
        material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.95, 0.85, 0.1),
            emissive: LinearRgba::new(0.4, 0.35, 0.0, 1.0),
            ..default()
        }),
        transform: Transform::from_xyz(rear_x + 0.04 * s, 0.105 * s, 0.0),
        ..default()
    }).set_parent(crank_entity);

    commands.spawn(PbrBundle {
        mesh: output_shaft_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.10 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    // ── Pistons ────────────────────────────────────────────────────────────
    for i in 0..NUM_CYL {
        commands.spawn((
            Piston { idx: i },
            PbrBundle {
                mesh: piston_mesh.clone(),
                material: piston_mat.clone(),
                transform: Transform::from_xyz(cyl_x(i), ROD_LENGTH * s, 0.0),
                ..default()
            },
        ));
    }

    // ── Connecting rods ────────────────────────────────────────────────────
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        commands.spawn((
            ConRod { idx: i, base_x: x },
            PbrBundle {
                mesh: rod_mesh.clone(),
                material: rod_mat.clone(),
                transform: Transform::from_xyz(x, ROD_LENGTH * 0.5 * s, 0.0),
                ..default()
            },
        ));
    }

    // ── Translucent cylinder bores (gas-coloured per cyl) ──────────────────
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        let bore_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(0.55, 0.6, 0.7, 0.18),
            emissive: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
            metallic: 0.2,
            perceptual_roughness: 0.7,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });

        let flash_mat = materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.5, 0.2, 0.0),
            emissive: LinearRgba::new(0.0, 0.0, 0.0, 1.0),
            alpha_mode: AlphaMode::Blend,
            unlit: true,
            cull_mode: None,
            ..default()
        });

        commands.spawn((
            CylinderGasViz { idx: i, bore_material: bore_mat.clone() },
            PbrBundle {
                mesh: bore_mesh.clone(),
                material: bore_mat,
                transform: Transform::from_xyz(x, ROD_LENGTH * s + 0.04 * s, 0.0),
                ..default()
            },
        ));

        // Combustion flash sphere — sits at the top of the cylinder, brightens
        // and tints from the fuel's flame colour during ignition.
        commands.spawn((
            CombustionFlash { cyl: i, material: flash_mat.clone() },
            PbrBundle {
                mesh: flash_mesh.clone(),
                material: flash_mat,
                transform: Transform::from_xyz(x, ROD_LENGTH * s + 0.115 * s, 0.0),
                ..default()
            },
        ));
    }

    // ── Cylinder head (single block above all four bores) ─────────────────
    let head_y = ROD_LENGTH * s + 0.18 * s;
    commands.spawn(PbrBundle {
        mesh: head_mesh.clone(),
        material: head_mat.clone(),
        transform: Transform::from_xyz(0.0, head_y, 0.0),
        ..default()
    });

    // ── Valves: 2 per cylinder (intake on −Z, exhaust on +Z) ──────────────
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        let valve_seat_y = ROD_LENGTH * s + 0.13 * s;

        for (kind, z_off) in [(ValveKind::Intake, -0.022 * s), (ValveKind::Exhaust, 0.022 * s)] {
            // Stem
            commands.spawn(PbrBundle {
                mesh: valve_stem_mesh.clone(),
                material: valve_mat.clone(),
                transform: Transform::from_xyz(x, valve_seat_y + 0.045 * s, z_off),
                ..default()
            });
            // Head (animated)
            commands.spawn((
                Valve { cyl: i, kind, seat_y: valve_seat_y },
                PbrBundle {
                    mesh: valve_head_mesh.clone(),
                    material: valve_mat.clone(),
                    transform: Transform::from_xyz(x, valve_seat_y, z_off),
                    ..default()
                },
            ));
        }
    }

    // ── Intake / exhaust runner tubes (one tube per side) ─────────────────
    let runner_len = (NUM_CYL as f32 * CYL_SPACING + 0.06) * s;

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

    let runner_mesh = meshes.add(Cylinder::new(0.030 * s, runner_len));
    commands.spawn((
        ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
        PbrBundle {
            mesh: runner_mesh.clone(),
            material: intake_mat,
            transform: Transform::from_xyz(0.0, head_y + 0.08 * s, -0.10 * s)
                .with_rotation(Quat::from_rotation_z(PI / 2.0)),
            ..default()
        },
    ));
    commands.spawn((
        ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
        PbrBundle {
            mesh: runner_mesh.clone(),
            material: exhaust_mat,
            transform: Transform::from_xyz(0.0, head_y + 0.08 * s, 0.10 * s)
                .with_rotation(Quat::from_rotation_z(PI / 2.0)),
            ..default()
        },
    ));

    // Short port tubes from head to runners
    let port_mesh = meshes.add(Cylinder::new(0.015 * s, 0.10 * s));
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        for z_off in [-0.10 * s, 0.10 * s] {
            commands.spawn(PbrBundle {
                mesh: port_mesh.clone(),
                material: head_mat.clone(),
                transform: Transform::from_xyz(x, head_y + 0.04 * s, z_off),
                ..default()
            });
        }
    }

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
