//! Crankshaft hierarchy: modular GLB throws, front pulley, flywheel, clutch
//! plate, and the dummy output shaft stub that the gearbox extends from.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::EngineLayout;
use crate::visuals::{Clutch, Crankshaft, EngineVisual, Flywheel};

use super::BuildCtx;

const MODEL_PIN_RADIUS: f32 = 4.72;

/// Spawns the crank hierarchy. Returns the root crank entity (so callers could
/// attach further children, though right now none do).
pub fn spawn(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    asset_server: &AssetServer,
    ctx: &BuildCtx,
) -> Entity {
    let cfg = &ctx.core.config;
    let s = ctx.s;
    let num_cyl = cfg.num_cylinders;
    let cyl_spacing = cfg.cylinder_spacing;
    let crank_radius = cfg.crank_radius();

    // Shared materials / meshes local to the crank subassembly
    let crank_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.13, 0.13),
        metallic: 0.7, perceptual_roughness: 0.32, ..default()
    });
    let pulley_mesh = meshes.add(Cylinder::new(0.060 * s, 0.025 * s));
    let output_shaft_mesh = meshes.add(Cylinder::new(0.022 * s, 0.10 * s));
    let crank_axis_rot = Quat::from_rotation_z(PI / 2.0);

    // Root crank entity — its rotation is driven by animate_crank.
    let crank_entity = commands.spawn((
        EngineVisual, Crankshaft, SpatialBundle::default(), Name::new("Crankshaft"),
    )).id();

    // Modular GLB throws strung along the X axis.
    let radial_scale = (crank_radius * s) / MODEL_PIN_RADIUS;
    let length_scale = ctx.length_scale;
    let base_orient = Quat::from_rotation_y(PI / 2.0);
    let crank_scene: Handle<Scene> = asset_server.load("engine/crank/modular_crank.glb#Scene0");

    let pin_count = match cfg.layout {
        EngineLayout::Inline | EngineLayout::Flat => num_cyl,
        EngineLayout::V => num_cyl / 2,
        EngineLayout::W { .. } => num_cyl / 2,
    };

    for pos in 0..pin_count {
        let cyl_idx = match cfg.layout {
            EngineLayout::Inline | EngineLayout::Flat => pos,
            EngineLayout::V => pos * 2,
            EngineLayout::W { .. } => pos * 2,
        };
        let x = cfg.cyl_visual_x(cyl_idx);
        let phi = cfg.crank_phases[cyl_idx];
        let combined_rot = Quat::from_rotation_x(phi) * base_orient;

        commands.spawn((
            EngineVisual,
            Name::new(format!("Crank Module {}", pos + 1)),
            SceneBundle {
                scene: crank_scene.clone(),
                transform: Transform::from_xyz(x, 0.0, 0.0)
                    .with_rotation(combined_rot)
                    .with_scale(Vec3::new(radial_scale, radial_scale, length_scale)),
                ..default()
            },
        )).set_parent(crank_entity);
    }

    // Front pulley
    commands.spawn((
        EngineVisual,
        Name::new("Front Pulley"),
        PbrBundle {
            mesh: pulley_mesh,
            material: crank_mat.clone(),
            transform: Transform::from_xyz(ctx.front_x - 0.04 * s, 0.0, 0.0)
                .with_rotation(crank_axis_rot),
            ..default()
        },
    )).set_parent(crank_entity);

    // Flywheel (GLB)
    let flywheel_scene = asset_server.load("engine/crank/flywheel.glb#Scene0");
    commands.spawn((
        EngineVisual,
        Flywheel,
        Name::new("Flywheel"),
        SceneBundle {
            scene: flywheel_scene,
            transform: Transform::from_xyz(ctx.rear_x + 0.04 * s, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(-PI / 2.0))
                .with_scale(Vec3::splat(0.01)),
            ..default()
        },
    )).set_parent(crank_entity);

    // Clutch plate (GLB)
    let clutch_scene = asset_server.load("engine/crank/clutch_plate.glb#Scene0");
    commands.spawn((
        EngineVisual,
        Clutch,
        Name::new("Clutch Plate"),
        SceneBundle {
            scene: clutch_scene,
            transform: Transform::from_xyz(ctx.rear_x + 0.08 * s, 0.0, 0.0)
                .with_rotation(Quat::from_rotation_y(-PI / 2.0))
                .with_scale(Vec3::splat(0.01)),
            ..default()
        },
    )).set_parent(crank_entity);

    // Output shaft stub (decorative — the gearbox mainshaft picks up here).
    commands.spawn((
        EngineVisual,
        Name::new("Output Shaft"),
        PbrBundle {
            mesh: output_shaft_mesh,
            material: crank_mat,
            transform: Transform::from_xyz(ctx.rear_x + 0.10 * s, 0.0, 0.0)
                .with_rotation(crank_axis_rot),
            ..default()
        },
    )).set_parent(crank_entity);

    let _ = cyl_spacing; // kept for future per-throw spacing tweaks
    crank_entity
}
