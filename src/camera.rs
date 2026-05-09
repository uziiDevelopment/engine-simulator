//! Studio-style orbit camera.
//!
//! Inputs update *target* values; the actually-rendered yaw / pitch / zoom /
//! focus are critically-damped toward the targets each frame so motion feels
//! weighted but never floaty.
//!
//! Controls:
//!   • RMB drag             — orbit
//!   • MMB drag (or ⇧ + RMB) — pan in screen space
//!   • Scroll               — exponential zoom
//!   • F                     — frame the engine

use std::f32::consts::{PI, TAU};

use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::engine::{ROD_LENGTH, VIS_SCALE};

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            .add_systems(Update, orbit_camera_system);
    }
}

#[derive(Component)]
pub struct OrbitCamera {
    // Smoothed values used to position the camera.
    pub yaw:      f32,
    pub pitch:    f32,
    pub distance: f32,
    pub focus:    Vec3,
    // Targets the smoothed values are damped toward.
    pub target_yaw:      f32,
    pub target_pitch:    f32,
    pub target_distance: f32,
    pub target_focus:    Vec3,
}

fn spawn_camera(mut commands: Commands) {
    let initial_focus = Vec3::new(0.0, ROD_LENGTH * 0.5 * VIS_SCALE, 0.0);
    let initial_yaw = 0.6;
    let initial_pitch = 0.35;
    let initial_distance = 9.0;

    commands.spawn((
        Camera3dBundle {
            transform: Transform::from_xyz(4.0, 3.0, 7.0).looking_at(initial_focus, Vec3::Y),
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
        if orbiting {
            orbit.target_yaw   -= motion.x * 0.0065;
            orbit.target_pitch += motion.y * 0.0065;
            orbit.target_pitch = orbit.target_pitch.clamp(-1.45, 1.45);
        }

        if panning {
            let cam_rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
                * Quat::from_axis_angle(Vec3::X, orbit.pitch);
            let right = cam_rot * Vec3::X;
            let up    = cam_rot * Vec3::Y;
            let pan_scale = orbit.distance * 0.0018;
            orbit.target_focus -= right * motion.x * pan_scale;
            orbit.target_focus += up    * motion.y * pan_scale;
        }

        if wheel.abs() > 0.0 && !pointer_over_egui {
            let factor = (-wheel * 0.12).exp();
            orbit.target_distance = (orbit.target_distance * factor).clamp(1.5, 50.0);
        }

        if frame_request {
            orbit.target_focus = Vec3::new(0.0, ROD_LENGTH * 0.5 * VIS_SCALE, 0.0);
            orbit.target_distance = 9.0;
            orbit.target_yaw   = 0.6;
            orbit.target_pitch = 0.35;
        }

        let smooth = 1.0 - (-dt * 16.0).exp();
        orbit.yaw      = lerp_angle(orbit.yaw, orbit.target_yaw, smooth);
        orbit.pitch    = lerp_f32(orbit.pitch, orbit.target_pitch, smooth);
        orbit.distance = lerp_f32(orbit.distance, orbit.target_distance, smooth);
        orbit.focus    = orbit.focus.lerp(orbit.target_focus, smooth);

        let rot = Quat::from_axis_angle(Vec3::Y, orbit.yaw)
            * Quat::from_axis_angle(Vec3::X, orbit.pitch);
        let offset = rot * Vec3::new(0.0, 0.0, orbit.distance);
        t.translation = orbit.focus + offset;
        t.look_at(orbit.focus, Vec3::Y);
    }
}

#[inline] fn lerp_f32(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

#[inline]
fn lerp_angle(a: f32, b: f32, t: f32) -> f32 {
    let mut diff = (b - a) % TAU;
    if diff >  PI { diff -= TAU; }
    if diff < -PI { diff += TAU; }
    a + diff * t
}
