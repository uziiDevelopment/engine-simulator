//! Throttle butterfly flap rotation — 0% throttle = closed, 100% = open.

use bevy::prelude::*;

use crate::engine::EngineCore;
use crate::visuals::ThrottleFlap;

pub fn animate_throttle(
    core: Res<EngineCore>,
    mut q_flap: Query<&mut Transform, With<ThrottleFlap>>,
) {
    let throttle = core.throttle_smoothed;
    let angle = throttle * std::f32::consts::FRAC_PI_2;
    for mut transform in &mut q_flap {
        transform.rotation = Quat::from_rotation_y(angle);
    }
}
