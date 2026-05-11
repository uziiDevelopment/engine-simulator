//! 6-speed manual gearbox: transparent housing, two shafts, six visible gear
//! pairs sized to the ratio array, reverse idler, and four sliding sleeves.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::visuals::{
    EngagementSleeve, EngineVisual, GearCog, GearboxHousing, Layshaft, Mainshaft,
    ReverseIdler, ShaftKind,
};

use super::BuildCtx;

pub fn spawn(
    commands: &mut Commands,
    meshes: &mut ResMut<Assets<Mesh>>,
    materials: &mut ResMut<Assets<StandardMaterial>>,
    ctx: &BuildCtx,
) {
    let s = ctx.s;
    let gx0 = ctx.rear_x + 0.30 * s;

    // Materials
    let housing_base = Color::srgba(0.55, 0.6, 0.65, 0.18);
    let housing_mat = materials.add(StandardMaterial {
        base_color: housing_base,
        metallic: 0.3, perceptual_roughness: 0.4,
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let cog_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.85, 0.82, 0.55),
        metallic: 0.85, perceptual_roughness: 0.30, ..default()
    });
    let shaft_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.65, 0.65, 0.70),
        metallic: 0.9, perceptual_roughness: 0.25, ..default()
    });
    let sleeve_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.30, 0.42, 0.55),
        metallic: 0.8, perceptual_roughness: 0.35, ..default()
    });
    let idler_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.72, 0.55, 0.30),
        metallic: 0.8, perceptual_roughness: 0.4, ..default()
    });

    // Housing
    let box_w = 0.60 * s;
    let box_h = 0.45 * s;
    let box_d = 0.35 * s;
    let housing_mesh = meshes.add(Cuboid::new(box_w, box_h, box_d));
    commands.spawn((
        EngineVisual,
        GearboxHousing { material: housing_mat.clone(), base_color: housing_base },
        Name::new("Gearbox Housing"),
        PbrBundle {
            mesh: housing_mesh,
            material: housing_mat,
            transform: Transform::from_xyz(gx0 + box_w * 0.5 - 0.05 * s, 0.0, 0.0),
            ..default()
        },
    ));

    // Layout: mainshaft along X at y=0; layshaft below.
    let main_y = 0.0_f32;
    let lay_y = -0.13 * s;
    let n_pairs = 6;
    let pair_span = box_w * 0.78;
    let pair_start = gx0 + 0.04 * s;
    let pair_dx = pair_span / (n_pairs as f32);

    let centre_dist = main_y - lay_y;
    let ratios = [3.6f32, 2.1, 1.5, 1.15, 0.95, 0.75];
    let cog_thickness = 0.05 * s;

    let shaft_radius = 0.020 * s;
    let shaft_len = box_w * 0.95;
    let shaft_rot = Quat::from_rotation_z(PI / 2.0);
    let shaft_center_x = gx0 + box_w * 0.5 - 0.05 * s;
    let mainshaft_mesh = meshes.add(Cylinder::new(shaft_radius, shaft_len));
    let layshaft_mesh = meshes.add(Cylinder::new(shaft_radius * 0.9, shaft_len));

    let mainshaft = commands.spawn((
        EngineVisual, Mainshaft, Name::new("Mainshaft"),
        PbrBundle {
            mesh: mainshaft_mesh,
            material: shaft_mat.clone(),
            transform: Transform::from_xyz(shaft_center_x, main_y, 0.0)
                .with_rotation(shaft_rot),
            ..default()
        },
    )).id();
    let layshaft = commands.spawn((
        EngineVisual, Layshaft, Name::new("Layshaft"),
        PbrBundle {
            mesh: layshaft_mesh,
            material: shaft_mat,
            transform: Transform::from_xyz(shaft_center_x, lay_y, 0.0)
                .with_rotation(shaft_rot),
            ..default()
        },
    )).id();

    // Six forward gear pairs
    for (i, &ratio) in ratios.iter().enumerate() {
        let r_lay = centre_dist / (1.0 + ratio);
        let r_main = centre_dist - r_lay;
        let x = pair_start + (i as f32 + 0.5) * pair_dx;
        let local_x = x - shaft_center_x;

        let lay_cog_mesh = meshes.add(Cylinder::new(r_lay, cog_thickness));
        commands.spawn((
            EngineVisual,
            GearCog { shaft: ShaftKind::Layshaft, gear: Some(i as u8) },
            Name::new(format!("Layshaft Cog {}", i + 1)),
            PbrBundle {
                mesh: lay_cog_mesh,
                material: cog_mat.clone(),
                transform: Transform::from_xyz(0.0, local_x, 0.0),
                ..default()
            },
        )).set_parent(layshaft);

        let main_cog_mesh = meshes.add(Cylinder::new(r_main, cog_thickness));
        commands.spawn((
            EngineVisual,
            GearCog { shaft: ShaftKind::Mainshaft, gear: Some(i as u8) },
            Name::new(format!("Mainshaft Cog {}", i + 1)),
            PbrBundle {
                mesh: main_cog_mesh,
                material: cog_mat.clone(),
                transform: Transform::from_xyz(0.0, local_x, 0.0),
                ..default()
            },
        )).set_parent(mainshaft);
    }

    // Reverse idler — off to the side at the +X end
    let idler_r = centre_dist * 0.35;
    let idler_mesh = meshes.add(Cylinder::new(idler_r, cog_thickness));
    let idler_x = pair_start + (n_pairs as f32 + 0.45) * pair_dx;
    let idler_y = (main_y + lay_y) * 0.5;
    let idler_z = 0.10 * s;
    commands.spawn((
        EngineVisual,
        ReverseIdler,
        GearCog { shaft: ShaftKind::Layshaft, gear: None },
        Name::new("Reverse Idler"),
        PbrBundle {
            mesh: idler_mesh,
            material: idler_mat,
            transform: Transform::from_xyz(idler_x, idler_y, idler_z),
            ..default()
        },
    ));

    // Engagement sleeves: three forward (1-2, 3-4, 5-6) + one reverse
    let sleeve_radius = 0.038 * s;
    let sleeve_len = 0.06 * s;
    let sleeve_mesh = meshes.add(Cylinder::new(sleeve_radius, sleeve_len));
    for hub_idx in 0..3u8 {
        let a = (hub_idx as usize) * 2;
        let b = a + 1;
        let xa = pair_start + (a as f32 + 0.5) * pair_dx;
        let xb = pair_start + (b as f32 + 0.5) * pair_dx;
        let neutral_x_world = (xa + xb) * 0.5;
        let neutral_x_local = neutral_x_world - shaft_center_x;
        let engage_offset = ((xb - xa) * 0.5 - cog_thickness * 0.6).max(0.01 * s);
        commands.spawn((
            EngineVisual,
            EngagementSleeve { hub_idx, neutral_x: neutral_x_local, engage_offset },
            Name::new(format!("Sleeve {}-{}", a + 1, b + 1)),
            PbrBundle {
                mesh: sleeve_mesh.clone(),
                material: sleeve_mat.clone(),
                transform: Transform::from_xyz(0.0, neutral_x_local, 0.0),
                ..default()
            },
        )).set_parent(mainshaft);
    }
    {
        let r_neutral_world = pair_start + n_pairs as f32 * pair_dx;
        let r_neutral_local = r_neutral_world - shaft_center_x;
        commands.spawn((
            EngineVisual,
            EngagementSleeve {
                hub_idx: 3,
                neutral_x: r_neutral_local,
                engage_offset: pair_dx * 0.5,
            },
            Name::new("Sleeve R"),
            PbrBundle {
                mesh: sleeve_mesh,
                material: sleeve_mat,
                transform: Transform::from_xyz(0.0, r_neutral_local, 0.0),
                ..default()
            },
        )).set_parent(mainshaft);
    }
}
