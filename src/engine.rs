//! Combustion-driven engine simulation.
//!
//! ## Architecture
//!
//! ```text
//! ┌───────────────┐                    ┌──────────────────┐
//! │ atmosphere    │── throttle plate ──┤ intake manifold  │
//! └───────────────┘                    └────────┬─────────┘
//!                                               │ intake valves
//!                       ┌──────────┬───────────────────────────┐
//!                       │ Cyl 1    │ Cyl 2 ... gas state:      │
//!                       │ Cyl 3    │   m, T, p, composition    │
//!                       │ Cyl 4    │   + Wiebe burn progress   │
//!                       └────┬─────┴───────────────────────────┘
//!                            │ exhaust valves
//!                       ┌────┴─────────────┐    ┌──────────────┐
//!                       │ exhaust manifold ├────┤ atmosphere   │
//!                       └──────────────────┘    └──────────────┘
//! ```
//!
//! The crankshaft has rotational inertia and is driven only by:
//!   * gas-pressure forces transmitted through the slider-crank mechanism,
//!   * the starter motor (engaged by the player),
//!   * mechanical friction.
//!
//! Each submodule owns one slice of the model:
//!
//! | module       | responsibility                                          |
//! |--------------|---------------------------------------------------------|
//! | `geometry`   | constants + slider-crank kinematics                     |
//! | `thermo`     | ideal-gas, choked-orifice flow, Wiebe burn function     |
//! | `fuel`       | fuel presets (LHV, AFR, burn rate, flame colour, ...)   |
//! | `valve`      | cam-driven valve lift, effective discharge area         |
//! | `cylinder`   | per-cylinder thermodynamic state + per-step update       |
//! | `manifold`   | intake / exhaust plenums + throttle + tailpipe          |
//! | `crank`      | rotational dynamics (friction, starter, redline)        |
//! | `state`      | the [`EngineCore`] resource glueing everything together |

mod crank;
mod cylinder;
mod fuel;
mod geometry;
mod manifold;
mod state;
mod thermo;
mod valve;

pub use crank::*;
pub use cylinder::*;
pub use fuel::*;
pub use geometry::*;
pub use manifold::*;
pub use state::*;
pub use thermo::*;
pub use valve::*;

use bevy::prelude::*;
use std::f32::consts::TAU;

/// Bevy plugin: registers [`EngineCore`] and the per-frame stepping system.
pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EngineCore::new(0))
            .add_systems(Update, (engine_input, engine_step).chain());
    }
}

/// Hold **E** to engage the starter.  When the engine is already running on its
/// own combustion, the starter is disconnected automatically.
fn engine_input(keys: Res<ButtonInput<KeyCode>>, mut core: ResMut<EngineCore>) {
    core.starter_active = keys.pressed(KeyCode::KeyE) && core.run_state != RunState::Running;
}

/// Step the entire engine forward by one frame, with `time_scale` applied for slow-mo.
///
/// Internally we sub-step at a few kHz so the gas dynamics stay stable even at
/// redline (where 1° of crank only takes ~22 µs).
pub fn engine_step(time: Res<Time>, mut core: ResMut<EngineCore>) {
    let frame_dt = time.delta_seconds().min(1.0 / 30.0) * core.time_scale;
    let substeps: usize = 80;
    let dt = frame_dt / substeps as f32;
    if dt <= 0.0 {
        return;
    }

    let mut last_torque = 0.0_f32;
    let mut total_fuel_burned = 0.0_f32;
    let mut total_work = 0.0_f32;

    for _ in 0..substeps {
        let rpm = core.rpm();

        // ── Run-state machine ────────────────────────────────────────────
        match core.run_state {
            RunState::Off => {
                if core.starter_active {
                    core.run_state = RunState::Cranking;
                }
            }
            RunState::Cranking => {
                if rpm >= STARTER_DISENGAGE_RPM {
                    core.run_state = RunState::Running;
                } else if !core.starter_active && rpm < 30.0 {
                    core.run_state = RunState::Off;
                    core.omega = 0.0;
                }
            }
            RunState::Running => {
                if rpm < STALL_RPM {
                    core.run_state = RunState::Off;
                    core.omega = 0.0;
                }
            }
        }

        let combustion_enabled = match core.run_state {
            RunState::Off => false,
            RunState::Cranking => rpm > 80.0,
            RunState::Running => true,
        };

        // ── Plumbing: throttle plate + tailpipe ──────────────────────────
        let throttle_eff = match core.run_state {
            RunState::Cranking => 0.10,
            _ => (0.015 + 0.985 * core.throttle).clamp(0.0, 1.0),
        };
        manifold::throttle_flow(&mut core.intake, throttle_eff, dt);
        manifold::exhaust_to_atmosphere(&mut core.exhaust, dt);

        // ── Step every cylinder ──────────────────────────────────────────
        let angle_old = core.angle;
        let fourstroke_old = core.fourstroke_angle;
        let omega = core.omega;
        let angle_new = angle_old + omega * dt;
        let fourstroke_new = fourstroke_old + omega * dt;

        let mut total_torque_from_gas = 0.0_f32;
        let EngineCore { ref fuel, ref mut cylinders, ref mut intake, ref mut exhaust, .. } = *core;
        for i in 0..NUM_CYL {
            let (tau, fuel_burned) = cylinder::step_cylinder(
                &mut cylinders[i],
                intake,
                exhaust,
                fuel,
                i,
                angle_old,
                angle_new,
                fourstroke_old,
                fourstroke_new,
                dt,
                combustion_enabled,
            );
            total_torque_from_gas += tau;
            total_fuel_burned += fuel_burned;
        }

        // ── Friction + starter + soft rev-limit ──────────────────────────
        let mut tau_total = total_torque_from_gas;
        tau_total -= friction_torque(core.omega);
        tau_total += starter_torque(rpm, core.starter_active);

        if rpm > REDLINE_RPM {
            tau_total -= ((rpm - REDLINE_RPM) / 200.0).min(2.0) * 60.0;
        }

        // ── Integrate the only true degree of freedom: the crank ─────────
        core.omega += tau_total / FLYWHEEL_INERTIA * dt;
        if core.omega < 0.0 {
            core.omega = 0.0;
        }
        let dtheta = core.omega * dt;
        core.angle = (core.angle + dtheta).rem_euclid(TAU);
        core.fourstroke_angle = (core.fourstroke_angle + dtheta).rem_euclid(2.0 * TAU);

        last_torque = tau_total;
        total_work += tau_total * core.omega * dt;
    }

    // ── Smooth telemetry for the UI ──────────────────────────────────────
    let alpha = 0.06;
    core.torque_smoothed += (last_torque - core.torque_smoothed) * alpha;
    core.power_smoothed += ((last_torque * core.omega) - core.power_smoothed) * alpha;
    core.map_smoothed += (core.intake.pressure() - core.map_smoothed) * alpha;
    core.exhaust_pressure_smoothed += (core.exhaust.pressure() - core.exhaust_pressure_smoothed) * alpha;
    core.exhaust_temp_smoothed += (core.exhaust.temperature - core.exhaust_temp_smoothed) * alpha;

    // Bulk AFR estimate from cylinder composition
    let total_air: f32 = core.cylinders.iter().map(|c| c.air_frac * c.mass).sum();
    let total_fuel: f32 = core.cylinders.iter().map(|c| c.fuel_frac * c.mass).sum();
    let afr = if total_fuel > 1e-12 { total_air / total_fuel } else { 0.0 };
    core.afr_smoothed += (afr - core.afr_smoothed) * 0.04;

    core.last_frame_fuel_kg = total_fuel_burned;
    core.last_frame_work_j = total_work;
}
