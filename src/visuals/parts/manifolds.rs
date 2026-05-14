//! Intake runner + throttle body + exhaust header manifold + port tubes.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::{EngineConfig, EngineLayout};
use crate::visuals::{
    EngineVisual, ExhaustOutlet, ManifoldKind, ManifoldViz, ThrottleFlap,
};

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
        EngineLayout::Inline => {
            let intake_pos = Vec3::new(0.0, head_y + 0.08 * s, -0.10 * s);
            commands.spawn((
                EngineVisual,
                Name::new("Intake Runner"),
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(),
                    material: intake_mat.clone(),
                    transform: Transform::from_translation(intake_pos)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(groups.manifolds);

            let throttle_body_pos = intake_pos + Vec3::new(-0.12 * s, 0.0, 0.0);
            spawn_throttle_body(commands, meshes, materials, throttle_body_pos, s);

            spawn_exhaust_manifold(
                commands, meshes, exhaust_mat.clone(), cfg, head_y, groups.manifolds, s,
                0, 0.0,
            );
        }
        _ => {
            let intake_pos = Vec3::new(0.0, head_y + 0.06 * s, 0.0);
            commands.spawn((
                EngineVisual,
                Name::new("Intake Runner"),
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(),
                    material: intake_mat.clone(),
                    transform: Transform::from_translation(intake_pos)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(groups.manifolds);

            let throttle_body_pos = intake_pos + Vec3::new(-0.12 * s, 0.0, 0.0);
            spawn_throttle_body(commands, meshes, materials, throttle_body_pos, s);

            let tilt_a = cfg.bank_angle * 0.5;
            let exhaust_mat_b = materials.add(StandardMaterial {
                base_color: Color::srgb(0.45, 0.2, 0.18),
                emissive: LinearRgba::new(0.1, 0.05, 0.02, 1.0),
                metallic: 0.5, perceptual_roughness: 0.55, ..default()
            });

            spawn_exhaust_manifold(
                commands, meshes, exhaust_mat.clone(), cfg, head_y, groups.manifolds, s,
                0, tilt_a,
            );
            spawn_exhaust_manifold(
                commands, meshes, exhaust_mat_b, cfg, head_y, groups.manifolds, s,
                1, -tilt_a,
            );
        }
    }

    // Per-cylinder short port tubes head ↔ runner — cosmetic only.
    let port_mesh = meshes.add(Cylinder::new(0.015 * s, 0.10 * s));
    for i in 0..cfg.num_cylinders {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        for z_local in [-0.10 * s, 0.10 * s] {
            let pos = tilt_position(x, head_y + 0.04 * s, z_local, tilt);
            let side = if z_local < 0.0 { "Intake" } else { "Exhaust" };
            commands.spawn((
                EngineVisual,
                Name::new(format!("Cyl {} {} Port", i + 1, side)),
                PbrBundle {
                    mesh: port_mesh.clone(),
                    material: head_mat.clone(),
                    transform: Transform::from_translation(pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                },
            )).set_parent(groups.manifolds);
        }
    }
}

// ── Throttle body with rotating flap ─────────────────────────────────────
fn spawn_throttle_body(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    pos: Vec3,
    s: f32,
) {
    let throttle_group = commands.spawn((
        EngineVisual,
        Name::new("Throttle Body"),
        SpatialBundle::from_transform(Transform::from_translation(pos)),
    )).id();

    let housing_mesh = meshes.add(Cuboid::new(0.04 * s, 0.08 * s, 0.08 * s));
    let housing_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.20, 0.20, 0.22),
        metallic: 0.6, perceptual_roughness: 0.5,
        ..default()
    });
    commands.spawn((
        EngineVisual, Name::new("Throttle Housing"),
        PbrBundle { mesh: housing_mesh, material: housing_mat, ..default() },
    )).set_parent(throttle_group);

    let flange_mesh = meshes.add(Cylinder::new(0.035 * s, 0.06 * s));
    let flange_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.30, 0.32),
        metallic: 0.7, perceptual_roughness: 0.4,
        ..default()
    });
    commands.spawn((
        EngineVisual, Name::new("Throttle Flange"),
        PbrBundle {
            mesh: flange_mesh,
            material: flange_mat,
            transform: Transform::from_rotation(Quat::from_rotation_z(PI / 2.0)),
            ..default()
        },
    )).set_parent(throttle_group);

    let flap_mesh = meshes.add(Cuboid::new(0.002 * s, 0.056 * s, 0.056 * s));
    let flap_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.80, 0.65, 0.25),
        metallic: 0.9, perceptual_roughness: 0.3,
        ..default()
    });
    commands.spawn((
        EngineVisual,
        Name::new("Throttle Flap"),
        ThrottleFlap,
        PbrBundle {
            mesh: flap_mesh,
            material: flap_mat,
            transform: Transform::from_rotation(Quat::from_rotation_z(PI / 6.0)),
            ..default()
        },
    )).set_parent(throttle_group);

    let shaft_mesh = meshes.add(Cylinder::new(0.004 * s, 0.10 * s));
    let shaft_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.40, 0.40, 0.42),
        metallic: 0.9, perceptual_roughness: 0.2,
        ..default()
    });
    commands.spawn((
        EngineVisual, Name::new("Throttle Shaft"),
        PbrBundle {
            mesh: shaft_mesh,
            material: shaft_mat,
            transform: Transform::from_rotation(Quat::from_rotation_x(PI / 2.0)),
            ..default()
        },
    )).set_parent(throttle_group);
}

// ── Exhaust header for one bank ──────────────────────────────────────────
fn spawn_exhaust_manifold(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    exhaust_mat: Handle<StandardMaterial>,
    cfg: &EngineConfig,
    head_y: f32,
    grp_manifolds: Entity,
    s: f32,
    bank: usize,
    tilt: f32,
) {
    // Cylinders on this bank, in X order
    let cyls: Vec<usize> = (0..cfg.num_cylinders)
        .filter(|&i| match cfg.layout {
            EngineLayout::Inline => true,
            EngineLayout::V | EngineLayout::Flat => i % 2 == bank,
            EngineLayout::W { .. } => i % 4 == bank,
        })
        .collect();
    if cyls.is_empty() { return; }

    let port_y_local = head_y + 0.04 * s;
    let port_z_local = 0.10 * s;
    let coll_y_local = head_y + 0.12 * s;
    let coll_z_local = 0.20 * s;
    let runner_radius = 0.026 * s;
    let collector_radius = 0.045 * s;

    let flange_mesh = meshes.add(Cuboid::new(0.060 * s, 0.014 * s, 0.080 * s));
    for &i in &cyls {
        let x = cfg.cyl_visual_x(i);
        let port_pos = tilt_position(x, port_y_local, port_z_local + 0.012 * s, tilt);
        let coll_pt = tilt_position(x, coll_y_local, coll_z_local, tilt);

        commands.spawn((
            EngineVisual,
            Name::new(format!("Exhaust Flange {}", i + 1)),
            PbrBundle {
                mesh: flange_mesh.clone(),
                material: exhaust_mat.clone(),
                transform: Transform::from_translation(port_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(grp_manifolds);

        let delta = coll_pt - port_pos;
        let len = delta.length().max(1e-4);
        let dir = delta / len;
        let center = (port_pos + coll_pt) * 0.5;
        let rotation = Quat::from_rotation_arc(Vec3::Y, dir);
        let runner_mesh = meshes.add(Cylinder::new(runner_radius, len));
        commands.spawn((
            EngineVisual,
            Name::new(format!("Exhaust Runner {}", i + 1)),
            ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
            PbrBundle {
                mesh: runner_mesh,
                material: exhaust_mat.clone(),
                transform: Transform::from_translation(center).with_rotation(rotation),
                ..default()
            },
        )).set_parent(grp_manifolds);
    }

    // Collector log along the bank's X axis
    let x_first = cfg.cyl_visual_x(cyls[0]);
    let x_last = cfg.cyl_visual_x(*cyls.last().unwrap());
    let outlet_sign: f32 = if bank == 1 { -1.0 } else { 1.0 };
    let overhang = 0.28 * s;
    let (x_min, x_max) = if outlet_sign > 0.0 {
        (x_first - 0.04 * s, x_last + overhang)
    } else {
        (x_first - overhang, x_last + 0.04 * s)
    };
    let coll_len = x_max - x_min;
    let coll_center_x = (x_min + x_max) * 0.5;
    let coll_center = tilt_position(coll_center_x, coll_y_local, coll_z_local, tilt);
    let collector_mesh = meshes.add(Cylinder::new(collector_radius, coll_len));
    let coll_rot = Quat::from_rotation_x(tilt) * Quat::from_rotation_z(PI / 2.0);
    commands.spawn((
        EngineVisual,
        Name::new(format!("Exhaust Collector {}", bank)),
        ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
        PbrBundle {
            mesh: collector_mesh,
            material: exhaust_mat.clone(),
            transform: Transform::from_translation(coll_center).with_rotation(coll_rot),
            ..default()
        },
    )).set_parent(grp_manifolds);

    let cap_mesh = meshes.add(Cylinder::new(collector_radius * 1.05, 0.020 * s));
    let cap_x = if outlet_sign > 0.0 { x_min } else { x_max };
    let cap_pos = tilt_position(cap_x, coll_y_local, coll_z_local, tilt);
    commands.spawn((
        EngineVisual,
        Name::new(format!("Exhaust End Cap {}", bank)),
        PbrBundle {
            mesh: cap_mesh,
            material: exhaust_mat.clone(),
            transform: Transform::from_translation(cap_pos).with_rotation(coll_rot),
            ..default()
        },
    )).set_parent(grp_manifolds);

    let enabled_turbos: Vec<usize> = cfg.turbos.iter().enumerate()
        .filter(|(_, t)| t.enabled)
        .map(|(i, _)| i)
        .collect();
    
    let bank_turbos: Vec<usize> = enabled_turbos.iter()
        .copied()
        .filter(|&i| {
            if cfg.layout == EngineLayout::Inline {
                bank == 0
            } else {
                i % 2 == bank
            }
        })
        .collect();

    for (bt_idx, &turbo_idx) in bank_turbos.iter().enumerate() {
        let frac = if bank_turbos.len() == 1 {
            1.0
        } else {
            bt_idx as f32 / (bank_turbos.len() as f32 - 1.0)
        };

        // For outlet_sign > 0, x_min is front, x_max is rear.
        // For outlet_sign < 0, x_max is front, x_min is rear.
        let outlet_x = if outlet_sign > 0.0 {
            x_min + (x_max - x_min) * (0.4 + 0.6 * frac)
        } else {
            x_max - (x_max - x_min) * (0.4 + 0.6 * frac)
        };

        let outlet_pos = tilt_position(outlet_x, coll_y_local, coll_z_local, tilt);
        let outlet_flange_mesh = meshes.add(Cuboid::new(0.020 * s, 0.10 * s, 0.10 * s));
        
        commands.spawn((
            EngineVisual,
            Name::new(format!("Exhaust Outlet Flange {}:{}", bank, turbo_idx)),
            PbrBundle {
                mesh: outlet_flange_mesh,
                material: exhaust_mat.clone(),
                transform: Transform::from_translation(outlet_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(grp_manifolds);

        commands.spawn((
            EngineVisual,
            ExhaustOutlet { turbo_idx: Some(turbo_idx), world_pos: outlet_pos },
            Name::new(format!("Exhaust Outlet Marker {}:{}", bank, turbo_idx)),
            SpatialBundle::from_transform(Transform::from_translation(outlet_pos)),
        )).set_parent(grp_manifolds);
    }
}
