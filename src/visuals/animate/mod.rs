//! Per-frame animation systems. Read [`EngineCore`], write transforms +
//! material handles. No simulation logic here.
//!
//! This is a conductor module: each engine part has its own animation
//! submodule. The plugin in `visuals.rs` schedules them as plain functions
//! (the submodules export `pub fn`), so we re-export each one here.

mod cockpit;
mod crank;
mod cylinders;
mod damage;
mod gearbox;
mod manifolds;
mod pistons;
mod throttle;
mod turbo;
mod valves;

use bevy::prelude::*;

pub use cockpit::{animate_clutch_pedal, animate_shift_lever, animate_throttle_pedal};
pub use crank::{animate_crank, animate_drivetrain};
pub use cylinders::animate_cylinder_gas;
pub use damage::{animate_damage, apply_clutch_material, apply_flywheel_material};
pub use gearbox::{
    animate_engagement_sleeves, animate_gear_cogs, animate_gearbox_shafts,
    animate_housing_damage,
};
pub use manifolds::animate_manifolds;
pub use pistons::{animate_pistons, animate_rods};
pub use throttle::animate_throttle;
pub use turbo::{align_turbo_to_outlet, animate_turbo, discover_turbo_wheels};
pub use valves::animate_valves;

/// Shared helper: world position from local (x along crank axis, y along bank
/// axis, z lateral) and the bank tilt angle.
#[inline]
pub(super) fn tilt_vec(x: f32, y_local: f32, z_local: f32, tilt: f32) -> Vec3 {
    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    Vec3::new(
        x,
        y_local * cos_t - z_local * sin_t,
        y_local * sin_t + z_local * cos_t,
    )
}
