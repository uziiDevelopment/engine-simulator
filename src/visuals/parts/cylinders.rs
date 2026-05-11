//! Per-cylinder mechanicals: piston, three rings, connecting rod, translucent
//! bore, and the block slice around the bore. All drive off `DamageVisual` so
//! each one tints independently with its cylinder's wear / temperature.

use bevy::prelude::*;
use bevy_mod_picking::prelude::Pickable;

use crate::visuals::{
    ConRod, CylinderGasViz, DamageSource, DamageVisual, EngineVisual, Piston,
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
    let bore = cfg.bore;
    let rod_length = cfg.rod_length;

    // Shared meshes
    let piston_mesh      = meshes.add(Cylinder::new(bore * 0.49 * s, 0.075 * s));
    let rod_mesh         = meshes.add(Cuboid::new(0.020 * s, 0.028 * s, rod_length * s));
    let bore_mesh        = meshes.add(Cylinder::new(bore * 0.55 * s, 0.18 * s));
    let ring_mesh        = meshes.add(Cylinder::new(bore * 0.50 * s, 0.010 * s));
    let block_slice_mesh = meshes.add(Cuboid::new(0.092 * s, 0.18 * s, 0.092 * s));

    for i in 0..cfg.num_cylinders {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        let y_p = cfg.piston_y(0.0, i) * s;
        let pos = tilt_position(x, y_p, 0.0, tilt);

        // ── Piston ─────────────────────────────────────────────────────────
        let piston_base = Color::srgb(0.18, 0.45, 0.85);
        let piston_emissive = LinearRgba::BLACK;
        let piston_mat = materials.add(StandardMaterial {
            base_color: piston_base,
            metallic: 0.55, perceptual_roughness: 0.35,
            ..default()
        });
        commands.spawn((
            EngineVisual,
            Name::new(format!("Piston {}", i + 1)),
            Piston { idx: i, bank_tilt: tilt },
            DamageVisual {
                source: DamageSource::Piston(i),
                material: piston_mat.clone(),
                base_color: piston_base,
                base_emissive: piston_emissive,
            },
            PbrBundle {
                mesh: piston_mesh.clone(),
                material: piston_mat,
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(groups.pistons);

        // ── Three piston rings stacked just below the crown ───────────────
        let ring_base = Color::srgb(0.85, 0.85, 0.88);
        let ring_emissive = LinearRgba::BLACK;
        for ring_idx in 0..3 {
            let ring_mat = materials.add(StandardMaterial {
                base_color: ring_base,
                metallic: 0.9, perceptual_roughness: 0.2,
                ..default()
            });
            let ring_offset_y = 0.030 * s - 0.014 * s * ring_idx as f32;
            let ring_pos = tilt_position(x, y_p + ring_offset_y, 0.0, tilt);
            commands.spawn((
                EngineVisual,
                Name::new(format!("Piston Ring {}-{}", i + 1, ring_idx + 1)),
                Piston { idx: i, bank_tilt: tilt },
                DamageVisual {
                    source: DamageSource::PistonRing(i),
                    material: ring_mat.clone(),
                    base_color: ring_base,
                    base_emissive: ring_emissive,
                },
                PbrBundle {
                    mesh: ring_mesh.clone(),
                    material: ring_mat,
                    transform: Transform::from_translation(ring_pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                },
            )).set_parent(groups.rings);
        }

        // ── Connecting rod (placeholder mesh — animate_rods reshapes once GLB markers load) ──
        let rod_base = Color::srgb(0.78, 0.78, 0.82);
        let rod_emissive = LinearRgba::BLACK;
        let rod_mat = materials.add(StandardMaterial {
            base_color: rod_base,
            metallic: 0.85, perceptual_roughness: 0.22,
            ..default()
        });
        commands.spawn((
            EngineVisual,
            Name::new(format!("Connecting Rod {}", i + 1)),
            ConRod { idx: i, base_x: x, bank_tilt: tilt },
            DamageVisual {
                source: DamageSource::Rod(i),
                material: rod_mat.clone(),
                base_color: rod_base,
                base_emissive: rod_emissive,
            },
            PbrBundle {
                mesh: rod_mesh.clone(),
                material: rod_mat,
                transform: Transform::from_translation(pos * 0.5),
                ..default()
            },
        )).set_parent(groups.rods);

        // ── Translucent bore — animated by animate_cylinder_gas ──────────
        let bore_y = rod_length * s + 0.04 * s;
        let bore_pos = tilt_position(x, bore_y, 0.0, tilt);
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
        commands.spawn((
            EngineVisual,
            Name::new(format!("Bore {}", i + 1)),
            CylinderGasViz { idx: i, bore_material: bore_mat.clone(), bank_tilt: tilt },
            Pickable::IGNORE,
            PbrBundle {
                mesh: bore_mesh.clone(),
                material: bore_mat,
                transform: Transform::from_translation(bore_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(groups.bores);

        // ── Block slice — tints with wall_wear / block_temp under damage view ─
        let block_base = Color::srgba(0.30, 0.32, 0.36, 0.10);
        let block_emissive = LinearRgba::BLACK;
        let block_mat = materials.add(StandardMaterial {
            base_color: block_base,
            metallic: 0.3, perceptual_roughness: 0.6,
            alpha_mode: AlphaMode::Blend,
            double_sided: true,
            cull_mode: None,
            ..default()
        });
        commands.spawn((
            EngineVisual,
            Name::new(format!("Block Slice {}", i + 1)),
            DamageVisual {
                source: DamageSource::BlockSlice(i),
                material: block_mat.clone(),
                base_color: block_base,
                base_emissive: block_emissive,
            },
            Pickable::IGNORE,
            PbrBundle {
                mesh: block_slice_mesh.clone(),
                material: block_mat,
                transform: Transform::from_translation(bore_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(groups.block);
    }
}
