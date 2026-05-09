// Inline-4 four-stroke engine simulator with realistic slider-crank kinematics
// and torque-based rotational dynamics.
//
// Geometry:  86mm bore, 86mm stroke, 145mm rod  (~2.0L K20-style)
// Firing order: 1-3-4-2  (cyl 1&4 share crank pin at 0°, cyl 2&3 at 180°)
// Each cylinder fires once every 720° of crank rotation.

use bevy::prelude::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy_egui::{egui, EguiContexts, EguiPlugin};
use std::f32::consts::{PI, TAU};

// ───────────────────────────── Engine geometry (meters) ────────────────────
const CRANK_RADIUS: f32 = 0.043;            // half of 86 mm stroke
const ROD_LENGTH:   f32 = 0.145;            // connecting-rod length
const BORE:         f32 = 0.086;            // cylinder bore
const NUM_CYL:      usize = 4;
const CYL_SPACING:  f32 = 0.10;             // spacing between cylinders along crank axis
const VIS_SCALE:    f32 = 8.0;              // visual scale factor (1 m → 8 units)

// Crank-pin angular phase per cylinder.  Inline-4: 1&4 at 0°, 2&3 at 180°.
const CRANK_PHASES: [f32; NUM_CYL] = [0.0, PI, PI, 0.0];

// Firing offsets within the 720° four-stroke cycle (firing order 1-3-4-2).
//   cyl 1 → 0°, cyl 3 → 180°, cyl 4 → 360°, cyl 2 → 540°
const FIRING_OFFSETS_DEG: [f32; NUM_CYL] = [0.0, 540.0, 180.0, 360.0];

// ───────────────────────────── Engine physics ──────────────────────────────
const FLYWHEEL_INERTIA:   f32 = 0.18;       // kg·m² (rotational inertia of crank+flywheel)
const FRICTION_BASE:      f32 = 4.5;        // Nm constant (Coulomb) friction
const FRICTION_VISCOUS:   f32 = 0.035;      // Nm·s/rad
const FRICTION_WINDAGE:   f32 = 0.00009;    // Nm·s²/rad²
const PUMPING_LOSS_PER_CYL: f32 = 0.6;      // Nm drag during non-power strokes
const PEAK_TORQUE_PER_CYL:f32 = 120.0;      // Nm peak combustion torque per cylinder
const STARTER_TORQUE:     f32 = 70.0;       // Nm starter motor stall torque
const STARTER_DISENGAGE_RPM: f32 = 550.0;
const COMBUSTION_RPM_MIN: f32 = 180.0;      // below this, no combustion
const CRANKING_THROTTLE:  f32 = 0.40;       // fuel during cranking (open-loop)
const IDLE_BASELINE:      f32 = 0.18;       // baseline fuel at zero slider
const IDLE_RPM:           f32 = 850.0;
const STALL_RPM:          f32 = 250.0;
const REDLINE_RPM:        f32 = 7500.0;

// ───────────────────────────── Components & resources ──────────────────────
#[derive(Component)] struct Crankshaft;
#[derive(Component)] struct Piston { idx: usize }
#[derive(Component)] struct ConRod { idx: usize, base_x: f32 }

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum EngineState { Off, Cranking, Running }

#[derive(Resource)]
struct Engine {
    angle: f32,                 // crank angle [0, 2π)
    fourstroke_angle: f32,      // crank angle within 4-stroke cycle [0, 4π)
    omega: f32,                 // rad/s
    state: EngineState,
    starter_active: bool,
    throttle: f32,              // 0..=1 user input
    time_scale: f32,            // 1.0 = real-time, <1 = slow-mo
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            angle: 0.0,
            fourstroke_angle: 0.0,
            omega: 0.0,
            state: EngineState::Off,
            starter_active: false,
            throttle: 0.0,
            time_scale: 1.0,
        }
    }
}

#[derive(Component)]
struct OrbitCamera {
    // Smoothed values actually used to position the camera each frame.
    yaw: f32,
    pitch: f32,
    distance: f32,
    focus: Vec3,
    // Targets the smoothed values are damped toward.
    target_yaw: f32,
    target_pitch: f32,
    target_distance: f32,
    target_focus: Vec3,
}

// ─────────────────────────────────── main ──────────────────────────────────
fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "Engine Crankshaft Simulator".into(),
                resolution: (1400., 880.).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin)
        .insert_resource(Engine::default())
        .insert_resource(ClearColor(Color::srgb(0.05, 0.06, 0.08)))
        .insert_resource(AmbientLight { color: Color::WHITE, brightness: 80.0 })
        .add_systems(Startup, setup)
        .add_systems(Update, (
            ui_panel,
            engine_input,
            engine_physics,
            update_crank_transform,
            update_piston_transforms,
            update_rod_transforms,
            orbit_camera_system,
        ).chain())
        .run();
}

// ─────────────────────────────── Scene setup ───────────────────────────────
fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let s = VIS_SCALE;

    // ── Materials ─────────────────────────────────────────────────────────
    let crank_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.13, 0.13),
        metallic: 0.7,
        perceptual_roughness: 0.32,
        ..default()
    });
    let piston_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.18, 0.45, 0.85),
        metallic: 0.55,
        perceptual_roughness: 0.35,
        ..default()
    });
    let rod_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.78, 0.78, 0.82),
        metallic: 0.85,
        perceptual_roughness: 0.22,
        ..default()
    });
    let block_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(0.55, 0.6, 0.7, 0.12),
        metallic: 0.2,
        perceptual_roughness: 0.7,
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        ..default()
    });
    let flywheel_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.10, 0.10, 0.12),
        metallic: 0.9,
        perceptual_roughness: 0.45,
        ..default()
    });
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.085, 0.10),
        perceptual_roughness: 0.95,
        ..default()
    });

    // ── Meshes ────────────────────────────────────────────────────────────
    let main_journal_mesh   = meshes.add(Cylinder::new(0.026 * s, 0.045 * s));
    let crank_pin_mesh      = meshes.add(Cylinder::new(0.024 * s, 0.060 * s));
    let counterweight_mesh  = meshes.add(Cuboid::new(0.025 * s, 0.11 * s, 0.06 * s));
    let piston_mesh         = meshes.add(Cylinder::new(BORE * 0.49 * s, 0.075 * s));
    let rod_mesh            = meshes.add(Cuboid::new(0.020 * s, ROD_LENGTH * s, 0.028 * s));
    let cylinder_block_mesh = meshes.add(Cylinder::new(BORE * 0.55 * s, 0.18 * s));
    let flywheel_mesh       = meshes.add(Cylinder::new(0.135 * s, 0.030 * s));
    let output_shaft_mesh   = meshes.add(Cylinder::new(0.022 * s, 0.10 * s));
    let pulley_mesh         = meshes.add(Cylinder::new(0.060 * s, 0.025 * s));

    let cyl_x = |i: usize| (i as f32 - 1.5) * CYL_SPACING * s;
    let crank_axis_rot = Quat::from_rotation_z(PI / 2.0);

    // ── Crankshaft (parent transform; children rotate with it) ────────────
    let crank_entity = commands.spawn((
        Crankshaft,
        SpatialBundle::default(),
    )).id();

    // Five main bearing journals (between & at ends of the four crank throws)
    for i in 0..=NUM_CYL {
        let x = (i as f32 - NUM_CYL as f32 * 0.5) * CYL_SPACING * s;
        commands.spawn(PbrBundle {
            mesh: main_journal_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, 0.0, 0.0).with_rotation(crank_axis_rot),
            ..default()
        }).set_parent(crank_entity);
    }

    // Crank pins + crank webs (counterweights) for each cylinder
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        let phi = CRANK_PHASES[i];
        let pin_y = phi.cos() * CRANK_RADIUS * s;
        let pin_z = phi.sin() * CRANK_RADIUS * s;

        // Crank pin — offset journal that the rod big-end rides on
        commands.spawn(PbrBundle {
            mesh: crank_pin_mesh.clone(),
            material: crank_mat.clone(),
            transform: Transform::from_xyz(x, pin_y, pin_z).with_rotation(crank_axis_rot),
            ..default()
        }).set_parent(crank_entity);

        // Two crank webs flanking the pin (also act as counterweights)
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

    // Front pulley (intake side, -X end) and flywheel (transmission side, +X end)
    let front_x  = -2.5 * CYL_SPACING * s;
    let rear_x   =  2.5 * CYL_SPACING * s;

    commands.spawn(PbrBundle {
        mesh: pulley_mesh.clone(),
        material: crank_mat.clone(),
        transform: Transform::from_xyz(front_x - 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    commands.spawn(PbrBundle {
        mesh: flywheel_mesh.clone(),
        material: flywheel_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.04 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    // Mark on flywheel so rotation is visible
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
        mesh: output_shaft_mesh.clone(),
        material: crank_mat.clone(),
        transform: Transform::from_xyz(rear_x + 0.10 * s, 0.0, 0.0).with_rotation(crank_axis_rot),
        ..default()
    }).set_parent(crank_entity);

    // ── Pistons ───────────────────────────────────────────────────────────
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        commands.spawn((
            Piston { idx: i },
            PbrBundle {
                mesh: piston_mesh.clone(),
                material: piston_mat.clone(),
                transform: Transform::from_xyz(x, ROD_LENGTH * s, 0.0),
                ..default()
            },
        ));
    }

    // ── Connecting rods ───────────────────────────────────────────────────
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

    // ── Translucent cylinder bores so the pistons are visible inside them ─
    for i in 0..NUM_CYL {
        let x = cyl_x(i);
        commands.spawn(PbrBundle {
            mesh: cylinder_block_mesh.clone(),
            material: block_mat.clone(),
            transform: Transform::from_xyz(x, ROD_LENGTH * s + 0.04 * s, 0.0),
            ..default()
        });
    }

    // ── Floor ─────────────────────────────────────────────────────────────
    commands.spawn(PbrBundle {
        mesh: meshes.add(Plane3d::default().mesh().size(40.0, 40.0)),
        material: floor_mat,
        transform: Transform::from_xyz(0.0, -2.5, 0.0),
        ..default()
    });

    // ── Lighting ──────────────────────────────────────────────────────────
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
            shadows_enabled: false,
            range: 20.0,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.0, -3.0),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 800_000.0,
            color: Color::srgb(0.5, 0.7, 1.0),
            shadows_enabled: false,
            range: 18.0,
            ..default()
        },
        transform: Transform::from_xyz(-3.0, 3.0, 4.0),
        ..default()
    });

    // ── Camera ────────────────────────────────────────────────────────────
    let initial_focus = Vec3::new(0.0, ROD_LENGTH * 0.5 * VIS_SCALE, 0.0);
    let initial_yaw = 0.6;
    let initial_pitch = 0.35;
    let initial_distance = 8.0;
    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(4.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
            ..default()
        },
        OrbitCamera {
            yaw: initial_yaw,
            pitch: initial_pitch,
            distance: initial_distance,
            focus: initial_focus,
            target_yaw: initial_yaw,
            target_pitch: initial_pitch,
            target_distance: initial_distance,
            target_focus: initial_focus,
        },
    ));
}

// ─────────────────────────────── Input ─────────────────────────────────────
fn engine_input(keys: Res<ButtonInput<KeyCode>>, mut engine: ResMut<Engine>) {
    // Hold E to engage starter motor (while engine is not yet running on its own).
    engine.starter_active =
        keys.pressed(KeyCode::KeyE) && engine.state != EngineState::Running;
}

// ─────────────────────────── Physics integration ───────────────────────────
fn engine_physics(time: Res<Time>, mut engine: ResMut<Engine>) {
    let frame_dt = time.delta_seconds().min(1.0 / 30.0) * engine.time_scale;
    let substeps = 8;
    let dt = frame_dt / substeps as f32;

    for _ in 0..substeps {
        let rpm = engine.omega.abs() * 60.0 / TAU;

        // ── State machine ────────────────────────────────────────────────
        match engine.state {
            EngineState::Off => {
                if engine.starter_active {
                    engine.state = EngineState::Cranking;
                }
            }
            EngineState::Cranking => {
                if rpm >= STARTER_DISENGAGE_RPM {
                    engine.state = EngineState::Running;
                } else if !engine.starter_active && rpm < 30.0 {
                    engine.state = EngineState::Off;
                    engine.omega = 0.0;
                }
            }
            EngineState::Running => {
                if rpm < STALL_RPM {
                    engine.state = EngineState::Off;
                    engine.omega = 0.0;
                }
            }
        }

        // ── Effective fuel / throttle ────────────────────────────────────
        // The slider drives wide-open throttle; an idle controller injects
        // a baseline (+ small correction) so the engine settles near idle
        // when the slider is at zero.
        let effective_throttle: f32 = match engine.state {
            EngineState::Running => {
                let idle_correction = ((IDLE_RPM - rpm) / 800.0).clamp(0.0, 0.18);
                (IDLE_BASELINE + idle_correction + (1.0 - IDLE_BASELINE) * engine.throttle)
                    .clamp(0.0, 1.0)
            }
            EngineState::Cranking => CRANKING_THROTTLE,
            EngineState::Off => 0.0,
        };

        // ── Combustion torque (sum of all four cylinders) ────────────────
        let mut torque = 0.0_f32;
        let combustion_active =
            engine.state == EngineState::Running ||
            (engine.state == EngineState::Cranking && rpm > COMBUSTION_RPM_MIN);

        for i in 0..NUM_CYL {
            let firing_at = FIRING_OFFSETS_DEG[i].to_radians();
            // Phase within this cylinder's own 720° cycle, measured from
            // its firing TDC.  Power stroke = first 180°.
            let phase = (engine.fourstroke_angle - firing_at).rem_euclid(2.0 * TAU);
            if phase < PI {
                if combustion_active {
                    // Parabolic torque pulse, peak at 90° ATDC.
                    let t = (phase - PI * 0.5) / (PI * 0.5);
                    let pulse = (1.0 - t * t).max(0.0);
                    torque += pulse * PEAK_TORQUE_PER_CYL * effective_throttle;
                }
            } else {
                // Pumping/compression losses on the non-power strokes.
                torque -= PUMPING_LOSS_PER_CYL;
            }
        }

        // ── Internal friction (Coulomb + viscous + windage) ──────────────
        let omega = engine.omega;
        if omega > 0.0 {
            let friction = FRICTION_BASE
                + FRICTION_VISCOUS * omega
                + FRICTION_WINDAGE * omega * omega;
            torque -= friction;
        }

        // ── Starter motor (torque falls linearly to zero at disengage) ───
        if engine.starter_active && rpm < STARTER_DISENGAGE_RPM {
            let starter_factor = (1.0 - rpm / STARTER_DISENGAGE_RPM).max(0.0);
            torque += STARTER_TORQUE * starter_factor;
        }

        // ── Soft rev limiter at redline ──────────────────────────────────
        if rpm > REDLINE_RPM {
            let over = (rpm - REDLINE_RPM) / 200.0;
            torque -= 40.0 * over.min(1.5);
        }

        // ── Integrate angular dynamics: I·dω/dt = τ ──────────────────────
        engine.omega += torque / FLYWHEEL_INERTIA * dt;
        if engine.omega < 0.0 { engine.omega = 0.0; }

        let dtheta = engine.omega * dt;
        engine.angle = (engine.angle + dtheta).rem_euclid(TAU);
        engine.fourstroke_angle = (engine.fourstroke_angle + dtheta).rem_euclid(2.0 * TAU);
    }
}

// ───────────────────────── Visual transform updates ────────────────────────
fn update_crank_transform(engine: Res<Engine>, mut q: Query<&mut Transform, With<Crankshaft>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_x(engine.angle);
    }
}

// Slider-crank kinematics: cylinder axis = +Y, crank axis = X.
// Crank pin position:    p_pin   = (x_cyl, R·cos(θ+φ), R·sin(θ+φ))
// Piston-center y:       y_p = R·cos(θ+φ) + √(L² − R²·sin²(θ+φ))
fn update_piston_transforms(engine: Res<Engine>, mut q: Query<(&Piston, &mut Transform)>) {
    let r = CRANK_RADIUS * VIS_SCALE;
    let l = ROD_LENGTH   * VIS_SCALE;
    for (p, mut t) in &mut q {
        let theta = engine.angle + CRANK_PHASES[p.idx];
        let s = theta.sin();
        let y_p = r * theta.cos() + (l * l - r * r * s * s).sqrt();
        t.translation.y = y_p;
        t.rotation = Quat::IDENTITY;
    }
}

fn update_rod_transforms(engine: Res<Engine>, mut q: Query<(&ConRod, &mut Transform)>) {
    let r = CRANK_RADIUS * VIS_SCALE;
    let l = ROD_LENGTH   * VIS_SCALE;
    for (rod, mut t) in &mut q {
        let theta = engine.angle + CRANK_PHASES[rod.idx];
        let pin   = Vec3::new(rod.base_x, r * theta.cos(), r * theta.sin());
        let y_p   = r * theta.cos() + (l * l - r * r * theta.sin().powi(2)).sqrt();
        let small = Vec3::new(rod.base_x, y_p, 0.0);

        let mid   = (pin + small) * 0.5;
        let dir   = (small - pin).normalize_or_zero();
        t.translation = mid;
        t.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
    }
}

// ──────────────────────────────── UI panel ─────────────────────────────────
fn ui_panel(mut contexts: EguiContexts, mut engine: ResMut<Engine>) {
    let rpm = engine.omega.abs() * 60.0 / TAU;
    let state_text = match engine.state {
        EngineState::Off      => "OFF",
        EngineState::Cranking => "CRANKING",
        EngineState::Running  => "RUNNING",
    };
    let state_color = match engine.state {
        EngineState::Off      => egui::Color32::from_rgb(180, 60, 60),
        EngineState::Cranking => egui::Color32::from_rgb(220, 180, 60),
        EngineState::Running  => egui::Color32::from_rgb(80, 200, 100),
    };

    egui::Window::new("Engine")
        .anchor(egui::Align2::LEFT_TOP, [12.0, 12.0])
        .resizable(false)
        .default_width(280.0)
        .show(contexts.ctx_mut(), |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("State:").strong());
                ui.colored_label(state_color, egui::RichText::new(state_text).strong());
            });
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("RPM:").strong());
                ui.label(egui::RichText::new(format!("{:>5.0}", rpm))
                    .monospace()
                    .size(18.0));
            });

            // RPM bar
            let bar_max = 8500.0;
            let frac = (rpm / bar_max).clamp(0.0, 1.0);
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 14.0),
                egui::Sense::hover(),
            );
            let painter = ui.painter();
            painter.rect_filled(rect, 3.0, egui::Color32::from_gray(40));
            let bar_color = if rpm > REDLINE_RPM {
                egui::Color32::from_rgb(230, 70, 70)
            } else if rpm > REDLINE_RPM - 1500.0 {
                egui::Color32::from_rgb(230, 180, 70)
            } else {
                egui::Color32::from_rgb(80, 180, 220)
            };
            let mut filled = rect;
            filled.set_width(rect.width() * frac);
            painter.rect_filled(filled, 3.0, bar_color);
            let redline_x = rect.left() + rect.width() * (REDLINE_RPM / bar_max);
            painter.line_segment(
                [egui::pos2(redline_x, rect.top()), egui::pos2(redline_x, rect.bottom())],
                egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 80, 80)),
            );

            ui.add_space(10.0);
            ui.label(egui::RichText::new("Throttle").strong());
            ui.spacing_mut().slider_width = ui.available_width() - 60.0;
            ui.add(
                egui::Slider::new(&mut engine.throttle, 0.0..=1.0)
                    .show_value(true)
                    .custom_formatter(|v, _| format!("{:>3.0}%", v * 100.0))
                    .custom_parser(|s| s.trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)),
            );

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Time Scale  (slow-mo)").strong());
            ui.add(
                egui::Slider::new(&mut engine.time_scale, 0.02..=1.0)
                    .show_value(true)
                    .logarithmic(true)
                    .custom_formatter(|v, _| {
                        if v >= 0.995 { "1.00× (real-time)".to_string() }
                        else { format!("{:.2}×", v) }
                    })
                    .custom_parser(|s| {
                        s.trim_end_matches('×').trim().parse::<f64>().ok()
                    }),
            );
            ui.horizontal(|ui| {
                if ui.small_button("1×").clicked()    { engine.time_scale = 1.0; }
                if ui.small_button("½×").clicked()    { engine.time_scale = 0.5; }
                if ui.small_button("¼×").clicked()    { engine.time_scale = 0.25; }
                if ui.small_button("⅒×").clicked()    { engine.time_scale = 0.10; }
                if ui.small_button("1/20×").clicked() { engine.time_scale = 0.05; }
            });

            ui.add_space(10.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Controls").strong());
            ui.label("• Hold  E         — starter motor");
            ui.label("• Drag  RMB       — orbit");
            ui.label("• Drag  MMB / ⇧RMB — pan");
            ui.label("• Scroll          — zoom");
            ui.label("• Press F         — frame engine");

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Specs").strong().small());
            ui.label(egui::RichText::new(format!(
                "Inline-4 • bore {:.0}mm • stroke {:.0}mm",
                BORE * 1000.0, CRANK_RADIUS * 2000.0
            )).small());
            ui.label(egui::RichText::new(format!(
                "rod {:.0}mm • firing 1-3-4-2 • redline {:.0} rpm",
                ROD_LENGTH * 1000.0, REDLINE_RPM
            )).small());
        });
}

// ─────────────────────────── Orbit camera control ──────────────────────────
//
// Studio-style camera:
//   • Right-mouse drag       — orbit
//   • Middle-mouse drag      — pan (also Shift + RMB)
//   • Mouse wheel            — zoom (cursor-anchored)
//   • F                       — frame the engine
//
// Inputs update *target* values; the actual camera state is critically-damped
// toward the targets each frame so motion feels weighted but never floaty.
fn orbit_camera_system(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<(&mut OrbitCamera, &mut Transform), With<Camera>>,
    mut motion_events: EventReader<MouseMotion>,
    mut wheel_events: EventReader<MouseWheel>,
    buttons: Res<ButtonInput<MouseButton>>,
    mut egui_ctx: EguiContexts,
) {
    let pointer_over_egui = egui_ctx.ctx_mut().is_pointer_over_area();

    let mut motion = Vec2::ZERO;
    for ev in motion_events.read() { motion += ev.delta; }
    let mut wheel = 0.0;
    for ev in wheel_events.read() { wheel += ev.y; }

    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let rmb = buttons.pressed(MouseButton::Right) && !pointer_over_egui;
    let mmb = buttons.pressed(MouseButton::Middle) && !pointer_over_egui;
    let orbiting = rmb && !shift;
    let panning  = mmb || (rmb && shift);

    let frame_request = keys.just_pressed(KeyCode::KeyF);
    let dt = time.delta_seconds().max(1e-4);

    for (mut orbit, mut t) in &mut q {
        // ── Orbit ────────────────────────────────────────────────────────
        if orbiting {
            orbit.target_yaw   -= motion.x * 0.0065;
            orbit.target_pitch += motion.y * 0.0065;
            orbit.target_pitch = orbit.target_pitch.clamp(-1.45, 1.45);
        }

        // ── Pan in screen space (scaled by zoom distance) ────────────────
        if panning {
            let cam_rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
                * Quat::from_axis_angle(Vec3::X, orbit.pitch);
            let right = cam_rot * Vec3::X;
            let up    = cam_rot * Vec3::Y;
            let pan_scale = orbit.distance * 0.0018;
            orbit.target_focus -= right * motion.x * pan_scale;
            orbit.target_focus += up    * motion.y * pan_scale;
        }

        // ── Zoom (exponential, smooth) ───────────────────────────────────
        if wheel.abs() > 0.0 && !pointer_over_egui {
            let factor = (-wheel * 0.12).exp();
            orbit.target_distance = (orbit.target_distance * factor).clamp(1.5, 50.0);
        }

        // ── F to frame the engine ────────────────────────────────────────
        if frame_request {
            orbit.target_focus = Vec3::new(0.0, ROD_LENGTH * 0.5 * VIS_SCALE, 0.0);
            orbit.target_distance = 8.0;
            orbit.target_yaw   = 0.6;
            orbit.target_pitch = 0.35;
        }

        // ── Critically-damped exponential smoothing toward targets ───────
        // Higher rate → snappier; ~16/s gives a solid, weighted feel.
        let smooth = 1.0 - (-dt * 16.0).exp();
        orbit.yaw      = lerp_angle(orbit.yaw, orbit.target_yaw, smooth);
        orbit.pitch    = lerp_f32(orbit.pitch, orbit.target_pitch, smooth);
        orbit.distance = lerp_f32(orbit.distance, orbit.target_distance, smooth);
        orbit.focus    = orbit.focus.lerp(orbit.target_focus, smooth);

        // ── Compose the transform ────────────────────────────────────────
        let rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
            * Quat::from_axis_angle(Vec3::X, orbit.pitch);
        let offset = rot * Vec3::new(0.0, 0.0, orbit.distance);
        t.translation = orbit.focus + offset;
        t.look_at(orbit.focus, Vec3::Y);
    }
}

#[inline] fn lerp_f32(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }
#[inline] fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    // Shortest-arc lerp so wrapping yaw past ±π doesn't unwind the long way.
    let mut diff = (b - a) % TAU;
    if diff >  PI { diff -= TAU; }
    if diff < -PI { diff += TAU; }
    a + diff * t
}
