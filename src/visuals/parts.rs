//! Dynamic scene assembly: spawns engine visual entities based on current config.
//! Called on startup and whenever the engine config changes.

use bevy::prelude::*;
use std::f32::consts::PI;

use crate::engine::{EngineCore, VIS_SCALE};

use super::{
    ConRod, Crankshaft, CylinderGasViz, DamageSource, DamageVisual, EngineVisual,
    ManifoldKind, ManifoldViz, Piston, Valve, ValveKind,
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
    asset_server: &AssetServer,
) {
    let cfg = &core.config;
    let s = VIS_SCALE;
    let num_cyl = cfg.num_cylinders;

    // ── Materials ──────────────────────────────────────────────────────────
    // Most parts use a unique material handle so the damage animator can tint
    // each one independently from its cylinder's wear / temperature.  Shared
    // handles are kept only for cosmetic parts that aren't damage-tracked
    // (heads, valves, runners).
    let crank_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.13, 0.13),
        metallic: 0.7, perceptual_roughness: 0.32, ..default()
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

    let piston_mesh        = meshes.add(Cylinder::new(bore * 0.49 * s, 0.075 * s));
    let rod_mesh           = meshes.add(Cuboid::new(0.020 * s, rod_length * s, 0.028 * s));
    let bore_mesh          = meshes.add(Cylinder::new(bore * 0.55 * s, 0.18 * s));
    let flywheel_mesh      = meshes.add(Cylinder::new(0.135 * s, 0.030 * s));
    let output_shaft_mesh  = meshes.add(Cylinder::new(0.022 * s, 0.10 * s));
    let pulley_mesh        = meshes.add(Cylinder::new(0.060 * s, 0.025 * s));
    let valve_head_mesh    = meshes.add(Cylinder::new(0.017 * s, 0.008 * s));
    let valve_stem_mesh    = meshes.add(Cylinder::new(0.004 * s, 0.06 * s));
    // Piston ring: a thin annular cylinder hugging the bore.
    let ring_mesh          = meshes.add(Cylinder::new(bore * 0.50 * s, 0.010 * s));
    // Block slice — a chunky cuboid wrapping the bore, colours with damage.
    let block_slice_mesh   = meshes.add(Cuboid::new(0.092 * s, 0.18 * s, 0.092 * s));

    let crank_axis_rot = Quat::from_rotation_z(PI / 2.0);

    // ── Crankshaft (parent; children rotate with it) ───────────────────────
    let crank_entity = commands.spawn((
        EngineVisual, Crankshaft, SpatialBundle::default(), Name::new("Crankshaft"),
    )).id();

    // ── Modular GLB crank pieces ────────────────────────────────────────────
    // Each modular_crank.glb represents one crank throw.  We string them along
    // the X axis (crankshaft longitudinal axis), one per pin position.
    // Named nodes in the GLB:
    //   - `connecting_rod_attachment`: where the connecting rod big-end sits
    //   - `crank_joining_surface`:    face where adjacent modules meet
    //
    // The model's joining axis comes out of Blender along Y → ends up along -Z
    // in glTF/Bevy.  We apply a base orientation to map it to the simulator's
    // crankshaft axis (+X).  The pin offset (Blender Z) maps to +Y in Bevy.
    const MODEL_PIN_RADIUS: f32 = 4.72;  // Distance from center to pin in model units (scaled by node)
    const MODEL_LENGTH: f32 = 10.49;     // Total longitudinal length of one module throw (scaled)
    const MODEL_PIN_OFFSET_X: f32 = 13.12; // Longitudinal: distance from GLB origin to pin center (scaled)
    const MODEL_PIN_OFFSET_Z: f32 = 2.97;  // Lateral: distance from GLB origin to pin center (scaled)

    let radial_scale = (crank_radius * s) / MODEL_PIN_RADIUS;
    let length_scale = (cyl_spacing * s) / MODEL_LENGTH;

    // Base orientation: rotate model so its longitudinal axis (-Z) aligns with +X.
    let base_orient = Quat::from_rotation_y(std::f32::consts::PI / 2.0);

    let crank_scene: Handle<Scene> = asset_server.load("engine/crank/modular_crank.glb#Scene0");

    // Number of crank throw positions along the X axis.
    let pin_count = match cfg.layout {
        crate::engine::EngineLayout::Inline | crate::engine::EngineLayout::Flat => num_cyl,
        crate::engine::EngineLayout::V => num_cyl / 2,
        crate::engine::EngineLayout::W { .. } => num_cyl / 2,
    };

    for pos in 0..pin_count {
        let cyl_idx = match cfg.layout {
            crate::engine::EngineLayout::Inline | crate::engine::EngineLayout::Flat => pos,
            crate::engine::EngineLayout::V => pos * 2,
            crate::engine::EngineLayout::W { .. } => pos * 2,
        };
        let x = cfg.cyl_visual_x(cyl_idx);
        let phi = cfg.crank_phases[cyl_idx];

        // Combined rotation: first orient the model (base_orient), then apply
        // the crank phase rotation around X so `connecting_rod_attachment`
        // lands at the correct angular position in the Y-Z plane.
        // We add PI to the phase to align the GLB pins with the piston TDC/BDC positions.
        let combined_rot = Quat::from_rotation_x(phi + std::f32::consts::PI) * base_orient;

        commands.spawn((
            EngineVisual,
            Name::new(format!("Crank Module {}", pos + 1)),
            SceneBundle {
                scene: crank_scene.clone(),
                transform: Transform::from_xyz(x + MODEL_PIN_OFFSET_X * length_scale, 0.0, -MODEL_PIN_OFFSET_Z * radial_scale)
                    .with_rotation(combined_rot)
                    .with_scale(Vec3::new(length_scale, radial_scale, radial_scale)),
                ..default()
            },
        )).set_parent(crank_entity);
    }

    let journal_count = match cfg.layout {
        crate::engine::EngineLayout::Inline | crate::engine::EngineLayout::Flat => num_cyl + 1,
        crate::engine::EngineLayout::V => (num_cyl / 2) + 1,
        crate::engine::EngineLayout::W { .. } => (num_cyl / 2) + 1,
    };

    // Calculate exact ends of the crankshaft modular assembly to snap pulley/flywheel to them
    // The module spans from Z=-178 to Z=-83 in GLB. 
    // Relative to pin at Z=-118.75, Front is at -59.25 and Rear is at +35.75.
    let front_x = cfg.cyl_visual_x(0) - 6.54 * length_scale;
    let rear_x  = cfg.cyl_visual_x(num_cyl - 1) + 3.95 * length_scale;

    commands.spawn((EngineVisual, Name::new("Front Pulley"), PbrBundle {
        mesh: pulley_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(front_x, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity);

    let flywheel = commands.spawn((EngineVisual, Name::new("Flywheel"), PbrBundle {
        mesh: flywheel_mesh.clone(), material: flywheel_mat.clone(),
        transform: Transform::from_xyz(rear_x, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity).id();

    // Removed yellow timing mark as it looked glitchy/unintended

    commands.spawn((EngineVisual, Name::new("Output Shaft"), PbrBundle {
        mesh: output_shaft_mesh.clone(), material: crank_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.10 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    })).set_parent(crank_entity);

    // ── Group entities ───────────────────────────────────────────────────────
    let grp_pistons = commands.spawn((EngineVisual, Name::new("Pistons"), SpatialBundle::default())).id();
    let grp_rings = commands.spawn((EngineVisual, Name::new("Piston Rings"), SpatialBundle::default())).id();
    let grp_rods = commands.spawn((EngineVisual, Name::new("Connecting Rods"), SpatialBundle::default())).id();
    let grp_bores = commands.spawn((EngineVisual, Name::new("Cylinder Bores"), SpatialBundle::default())).id();
    let grp_block = commands.spawn((EngineVisual, Name::new("Block Slices"), SpatialBundle::default())).id();
    let grp_valves = commands.spawn((EngineVisual, Name::new("Valves"), SpatialBundle::default())).id();
    let grp_heads = commands.spawn((EngineVisual, Name::new("Cylinder Heads"), SpatialBundle::default())).id();
    let grp_manifolds = commands.spawn((EngineVisual, Name::new("Manifolds"), SpatialBundle::default())).id();

    // ── Per-cylinder visuals (pistons, rods, bores, valves, flash) ─────────
    for i in 0..num_cyl {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        // Initial piston Y position (at rest, theta=0)
        let y_p = cfg.piston_y(0.0, i) * s;
        // Tilted position: piston moves along the bank axis
        let pos = tilt_position(x, y_p, 0.0, tilt);

        // ── Piston (unique material — drives off piston_temp / ring_wear) ──
        let piston_base = Color::srgb(0.18, 0.45, 0.85);
        let piston_emissive = LinearRgba::BLACK;
        let piston_mat_unique = materials.add(StandardMaterial {
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
                material: piston_mat_unique.clone(),
                base_color: piston_base,
                base_emissive: piston_emissive,
            },
            PbrBundle {
                mesh: piston_mesh.clone(),
                material: piston_mat_unique,
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(grp_pistons);

        // ── Piston ring stack (3 thin rings around the piston crown) ───────
        let ring_base = Color::srgb(0.85, 0.85, 0.88);
        let ring_emissive = LinearRgba::BLACK;
        for ring_idx in 0..3 {
            let ring_mat = materials.add(StandardMaterial {
                base_color: ring_base,
                metallic: 0.9, perceptual_roughness: 0.2,
                ..default()
            });
            // Stack three rings just below the piston crown.
            let ring_offset_y = 0.030 * s - 0.014 * s * ring_idx as f32;
            let ring_pos = tilt_position(x, y_p + ring_offset_y, 0.0, tilt);
            commands.spawn((
                EngineVisual,
                Name::new(format!("Piston Ring {}-{}", i + 1, ring_idx + 1)),
                Piston { idx: i, bank_tilt: tilt }, // animate alongside piston
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
            )).set_parent(grp_rings);
        }

        // ── Connecting rod (unique material — drives off rod_damage) ───────
        let rod_base = Color::srgb(0.78, 0.78, 0.82);
        let rod_emissive = LinearRgba::BLACK;
        let rod_mat_unique = materials.add(StandardMaterial {
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
                material: rod_mat_unique.clone(),
                base_color: rod_base,
                base_emissive: rod_emissive,
            },
            PbrBundle {
                mesh: rod_mesh.clone(),
                material: rod_mat_unique,
                transform: Transform::from_translation(pos * 0.5),
                ..default()
            },
        )).set_parent(grp_rods);

        // ── Translucent cylinder bore (gas pressure viz, unchanged) ────────
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
            Name::new(format!("Bore {}", i + 1)),
            CylinderGasViz { idx: i, bore_material: bore_mat_handle.clone(), bank_tilt: tilt },
            PbrBundle {
                mesh: bore_mesh.clone(),
                material: bore_mat_handle,
                transform: Transform::from_translation(bore_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(grp_bores);

        // ── Solid block slice wrapping the bore — drives off wall_wear / block_temp ──
        // A translucent shell so the gas viz inside still reads.  In normal
        // viewing it's near-invisible; in damage view it lights up the FEA gradient.
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
            PbrBundle {
                mesh: block_slice_mesh.clone(),
                material: block_mat,
                transform: Transform::from_translation(bore_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            },
        )).set_parent(grp_block);

        // (Combustion flash sphere replaced by particle bursts — see particles.rs)

        // ── Valves: intake (−Z side) and exhaust (+Z side) relative to bank ─
        let valve_seat_y = rod_length * s + 0.13 * s;
        for (kind, z_local) in [(ValveKind::Intake, -0.022 * s), (ValveKind::Exhaust, 0.022 * s)] {
            let stem_pos = tilt_position(x, valve_seat_y + 0.045 * s, z_local, tilt);
            let head_pos = tilt_position(x, valve_seat_y, z_local, tilt);

            let kind_name = match kind {
                ValveKind::Intake => "Intake",
                ValveKind::Exhaust => "Exhaust",
            };

            // Valve stem (static)
            commands.spawn((EngineVisual, Name::new(format!("{} Stem {}", kind_name, i + 1)), PbrBundle {
                mesh: valve_stem_mesh.clone(),
                material: valve_mat.clone(),
                transform: Transform::from_translation(stem_pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            })).set_parent(grp_valves);

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
            )).set_parent(grp_valves);
        }
    }

    // ── Cylinder head block(s) ─────────────────────────────────────────────
    // For inline: one head block.  For V/Flat: one per bank.
    let head_y = rod_length * s + 0.18 * s;
    let head_width = (journal_count as f32 * cyl_spacing + 0.08) * s;

    match cfg.layout {
        crate::engine::EngineLayout::Inline => {
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.16 * s));
            commands.spawn((EngineVisual, Name::new("Inline Head"), PbrBundle {
                mesh: head_mesh,
                material: head_mat.clone(),
                transform: Transform::from_xyz(0.0, head_y, 0.0),
                ..default()
            })).set_parent(grp_heads);
        }
        crate::engine::EngineLayout::W { .. } => {
            // Four heads, one per bank (A, B, C, D)
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.12 * s));
            for bank in 0..4usize {
                let tilt = cfg.cyl_bank_tilt(bank);
                let pos = tilt_position(0.0, head_y, 0.0, tilt);
                let bank_name = ["A", "B", "C", "D"][bank];
                commands.spawn((EngineVisual, Name::new(format!("Bank {} Head", bank_name)), PbrBundle {
                    mesh: head_mesh.clone(),
                    material: head_mat.clone(),
                    transform: Transform::from_translation(pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                })).set_parent(grp_heads);
            }
        }
        _ => {
            // Two heads, one per bank, tilted
            let head_mesh = meshes.add(Cuboid::new(head_width, 0.05 * s, 0.14 * s));
            for bank in 0..2 {
                let tilt = if bank == 0 { cfg.bank_angle * 0.5 } else { -cfg.bank_angle * 0.5 };
                let pos = tilt_position(0.0, head_y, 0.0, tilt);
                let bank_name = if bank == 0 { "A" } else { "B" };
                commands.spawn((EngineVisual, Name::new(format!("Bank {} Head", bank_name)), PbrBundle {
                    mesh: head_mesh.clone(),
                    material: head_mat.clone(),
                    transform: Transform::from_translation(pos)
                        .with_rotation(Quat::from_rotation_x(tilt)),
                    ..default()
                })).set_parent(grp_heads);
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
                Name::new("Intake Runner"),
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: intake_mat,
                    transform: Transform::from_xyz(0.0, head_y + 0.08 * s, -0.10 * s)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(grp_manifolds);
            commands.spawn((
                EngineVisual,
                Name::new("Exhaust Runner"),
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh, material: exhaust_mat,
                    transform: Transform::from_xyz(0.0, head_y + 0.08 * s, 0.10 * s)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(grp_manifolds);
        }
        _ => {
            // V / Flat: intake in the valley (center), exhaust on outer sides
            let intake_pos = Vec3::new(0.0, head_y + 0.06 * s, 0.0);
            commands.spawn((
                EngineVisual,
                Name::new("Intake Runner"),
                ManifoldViz { kind: ManifoldKind::Intake, material: intake_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: intake_mat,
                    transform: Transform::from_translation(intake_pos)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(grp_manifolds);
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
                Name::new("Bank A Exhaust Runner"),
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat.clone() },
                PbrBundle {
                    mesh: runner_mesh.clone(), material: exhaust_mat,
                    transform: Transform::from_translation(exh_pos_a)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(grp_manifolds);
            commands.spawn((
                EngineVisual,
                Name::new("Bank B Exhaust Runner"),
                ManifoldViz { kind: ManifoldKind::Exhaust, material: exhaust_mat_b.clone() },
                PbrBundle {
                    mesh: runner_mesh, material: exhaust_mat_b,
                    transform: Transform::from_translation(exh_pos_b)
                        .with_rotation(Quat::from_rotation_z(PI / 2.0)),
                    ..default()
                },
            )).set_parent(grp_manifolds);
        }
    }

    // ── Port tubes from head to runners ────────────────────────────────────
    let port_mesh = meshes.add(Cylinder::new(0.015 * s, 0.10 * s));
    for i in 0..num_cyl {
        let x = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        for z_local in [-0.10 * s, 0.10 * s] {
            let pos = tilt_position(x, head_y + 0.04 * s, z_local, tilt);
            let side = if z_local < 0.0 { "Intake" } else { "Exhaust" };
            commands.spawn((EngineVisual, Name::new(format!("Cyl {} {} Port", i + 1, side)), PbrBundle {
                mesh: port_mesh.clone(),
                material: head_mat.clone(),
                transform: Transform::from_translation(pos)
                    .with_rotation(Quat::from_rotation_x(tilt)),
                ..default()
            })).set_parent(grp_manifolds);
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
