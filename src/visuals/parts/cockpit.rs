//! Floating cockpit rig: pedal box (clutch + throttle pedals), shifter base,
//! H-pattern guide plate, and the shift lever + knob.

use bevy::prelude::*;
use std::f32::consts::FRAC_PI_4;

use crate::visuals::{
    Cockpit, EngineVisual, HPatternGuide, PedalControl, PedalKind,
    ShiftKnob, ShiftLever,
};

use super::BuildCtx;

pub fn spawn(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    ctx: &BuildCtx,
) {
    let s = ctx.s;
    let head_y = ctx.head_y;
    let head_width = ctx.head_width;

    let cx = -head_width * 0.5 - 0.20 * s;
    let cy = 0.0;
    let cz = head_width * 0.5 + 0.40 * s;
    let _ = cy;

    let cockpit = commands.spawn((
        EngineVisual, Cockpit, Name::new("Cockpit Rig"),
        SpatialBundle::from_transform(Transform::from_xyz(cx, 0.0, cz)),
    )).id();

    // Materials
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.18, 0.20),
        metallic: 0.2, perceptual_roughness: 0.85, ..default()
    });
    let pedal_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.55, 0.60),
        metallic: 0.8, perceptual_roughness: 0.35, ..default()
    });
    let lever_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        metallic: 0.55, perceptual_roughness: 0.55, ..default()
    });
    let knob_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.55, 0.05, 0.05),
        metallic: 0.3, perceptual_roughness: 0.45, ..default()
    });
    let plate_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.25, 0.28),
        metallic: 0.4, perceptual_roughness: 0.65, ..default()
    });

    // Floor + pedal box
    let floor_mesh = meshes.add(Cuboid::new(0.45 * s, 0.04 * s, 0.30 * s));
    commands.spawn((
        EngineVisual, Name::new("Cockpit Floor"),
        PbrBundle {
            mesh: floor_mesh,
            material: floor_mat.clone(),
            transform: Transform::from_xyz(0.0, head_y * 0.10, 0.0),
            ..default()
        },
    )).set_parent(cockpit);

    let box_mesh = meshes.add(Cuboid::new(0.40 * s, 0.18 * s, 0.06 * s));
    commands.spawn((
        EngineVisual, Name::new("Pedal Box"),
        PbrBundle {
            mesh: box_mesh,
            material: floor_mat.clone(),
            transform: Transform::from_xyz(0.0, head_y * 0.10 + 0.11 * s, -0.10 * s),
            ..default()
        },
    )).set_parent(cockpit);

    // Pedals — hinge entity at the pivot, visible cuboid offset down
    let pedal_len = 0.16 * s;
    let pedal_w = 0.06 * s;
    let pedal_t = 0.012 * s;
    let pedal_mesh = meshes.add(Cuboid::new(pedal_w, pedal_len, pedal_t));
    let pedal_base_y = head_y * 0.10 + 0.20 * s;
    let pedal_base_z = -0.06 * s;

    spawn_pedal(commands, &pedal_mesh, &pedal_mat, PedalKind::Clutch,
        cockpit, -0.10 * s, pedal_base_y, pedal_base_z, pedal_len);
    spawn_pedal(commands, &pedal_mesh, &pedal_mat, PedalKind::Throttle,
        cockpit, 0.10 * s, pedal_base_y, pedal_base_z, pedal_len);

    // Shifter base + H-pattern plate
    let shifter_x = 0.28 * s;
    let shifter_z = 0.05 * s;
    let base_mesh = meshes.add(Cuboid::new(0.10 * s, 0.04 * s, 0.10 * s));
    commands.spawn((
        EngineVisual, Name::new("Shifter Base"),
        PbrBundle {
            mesh: base_mesh,
            material: floor_mat,
            transform: Transform::from_xyz(shifter_x, head_y * 0.10 + 0.04 * s, shifter_z),
            ..default()
        },
    )).set_parent(cockpit);

    let plate_mesh = meshes.add(Cuboid::new(0.13 * s, 0.005 * s, 0.13 * s));
    commands.spawn((
        EngineVisual, HPatternGuide, Name::new("H-Pattern Guide"),
        PbrBundle {
            mesh: plate_mesh,
            material: plate_mat,
            transform: Transform::from_xyz(shifter_x, head_y * 0.10 + 0.07 * s, shifter_z),
            ..default()
        },
    )).set_parent(cockpit);

    // Lever — pivot at base, rod sticks straight up, knob on top
    let lever_pivot = commands.spawn((
        EngineVisual, ShiftLever, Name::new("Shift Lever"),
        SpatialBundle::from_transform(Transform::from_xyz(
            shifter_x,
            head_y * 0.10 + 0.07 * s,
            shifter_z,
        )),
    )).set_parent(cockpit).id();

    let rod_len = 0.22 * s;
    let rod_mesh = meshes.add(Cylinder::new(0.012 * s, rod_len));
    commands.spawn((
        EngineVisual, Name::new("Shift Lever Rod"),
        PbrBundle {
            mesh: rod_mesh,
            material: lever_mat,
            transform: Transform::from_xyz(0.0, rod_len * 0.5, 0.0),
            ..default()
        },
    )).set_parent(lever_pivot);

    let knob_mesh = meshes.add(Sphere::new(0.035 * s).mesh().ico(3).unwrap());
    commands.spawn((
        EngineVisual, ShiftKnob, Name::new("Shift Knob"),
        PbrBundle {
            mesh: knob_mesh,
            material: knob_mat,
            transform: Transform::from_xyz(0.0, rod_len, 0.0),
            ..default()
        },
    )).set_parent(lever_pivot);
}

fn spawn_pedal(
    commands: &mut Commands,
    pedal_mesh: &Handle<Mesh>,
    pedal_mat: &Handle<StandardMaterial>,
    kind: PedalKind,
    cockpit: Entity,
    x: f32,
    y: f32,
    z: f32,
    pedal_len: f32,
) {
    let name = match kind {
        PedalKind::Clutch => "Clutch",
        PedalKind::Throttle => "Throttle",
    };
    let hinge = commands.spawn((
        EngineVisual,
        PedalControl { kind, max_angle: FRAC_PI_4 },
        Name::new(format!("{} Pedal", name)),
        SpatialBundle::from_transform(Transform::from_xyz(x, y, z)),
    )).set_parent(cockpit).id();

    commands.spawn((
        EngineVisual,
        Name::new(format!("{} Pedal Pad", name)),
        PbrBundle {
            mesh: pedal_mesh.clone(),
            material: pedal_mat.clone(),
            transform: Transform::from_xyz(0.0, -pedal_len * 0.5, 0.0),
            ..default()
        },
    )).set_parent(hinge);
}
