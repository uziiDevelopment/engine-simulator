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

pub mod config;
mod crank;
mod cylinder;
mod fuel;
mod geometry;
mod manifold;
pub mod material;
pub mod oil;
mod state;
mod thermo;
mod valve;

pub use config::*;
pub use crank::*;
pub use cylinder::*;
pub use fuel::*;
pub use geometry::*;
pub use manifold::*;
pub use material::*;
pub use oil::*;
pub use state::*;
pub use thermo::*;
pub use valve::*;

use bevy::prelude::*;
use std::f32::consts::TAU;

/// Bevy plugin: registers [`EngineCore`] and the per-frame stepping system.
pub struct EnginePlugin;

impl Plugin for EnginePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(EngineCore::new(0, 0))
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
pub fn engine_step(
    time: Res<Time>,
    mut core: ResMut<EngineCore>,
    audio_tx: Option<ResMut<crate::audio::AudioTx>>,
) {
    let frame_dt = time.delta_seconds().min(1.0 / 30.0) * core.time_scale;
    let substeps: usize = 80;
    let dt = frame_dt / substeps as f32;
    if dt <= 0.0 {
        return;
    }

    let mut last_torque = 0.0_f32;
    let mut total_fuel_burned = 0.0_f32;
    let mut total_work = 0.0_f32;
    let mut last_friction_heat_w = 0.0_f32;

    let mut audio_buffer = Vec::with_capacity(substeps);

    for _ in 0..substeps {
        let rpm = core.rpm();

        // ── Run-state machine ────────────────────────────────────────────
        // A seized engine refuses to spin regardless of player input.
        if core.engine_seized {
            core.run_state = RunState::Off;
            core.omega = 0.0;
            core.starter_active = false;
        }

        match core.run_state {
            RunState::Off => {
                if core.starter_active && !core.engine_seized {
                    core.run_state = RunState::Cranking;
                }
            }
            RunState::Cranking => {
                if rpm >= core.config.starter_disengage_rpm {
                    core.run_state = RunState::Running;
                } else if !core.starter_active && rpm < 30.0 {
                    core.run_state = RunState::Off;
                    core.omega = 0.0;
                }
            }
            RunState::Running => {
                if rpm < core.config.stall_rpm {
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
        let cranking = core.run_state == RunState::Cranking;
        let throttle_eff = core.config.effective_throttle(core.throttle, cranking);

        // ── Step every cylinder ──────────────────────────────────────────
        let angle_old = core.angle;
        let fourstroke_old = core.fourstroke_angle;
        let omega = core.omega;
        let angle_new = angle_old + omega * dt;
        let fourstroke_new = fourstroke_old + omega * dt;

        let mut total_torque_from_gas = 0.0_f32;
        let mut total_friction_torque = 0.0_f32;
        let mut total_friction_heat_w = 0.0_f32;
        let mut total_oil_consumed = 0.0_f32;
        let mut any_failed = false;
        let num_cyl = core.config.num_cylinders;
        let wear_time_scale = core.wear_time_scale;
        let EngineCore {
            ref config, ref fuel,
            ref mut cylinders,
            ref mut intake, ref mut exhaust,
            ref oil_config, ref oil,
            ..
        } = *core;

        manifold::throttle_flow_cfg(config, intake, throttle_eff, dt);
        manifold::exhaust_to_atmosphere_cfg(config, exhaust, dt);

        for i in 0..num_cyl {
            // Worn-out cylinders contribute reduced gas torque (lost compression
            // through worn rings/walls).  rod_damage at 1.0 = snapped, so the
            // cylinder simply can't push.
            let rod_dmg_before = cylinders[i].rod_damage;
            let wall_w_before = cylinders[i].wall_wear;
            let ring_w_before = cylinders[i].ring_wear;
            let compression_factor =
                (1.0 - 0.6 * wall_w_before - 0.4 * ring_w_before).max(0.0);
            let cyl_alive = (1.0 - rod_dmg_before).max(0.0);

            let (tau, fuel_burned) = cylinder::step_cylinder_cfg(
                config,
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
            let derated = tau * compression_factor * cyl_alive;
            total_torque_from_gas += derated;
            total_fuel_burned += fuel_burned;

            // Material-aware mechanical step (friction, wear, thermal, rod stress).
            let mech = cylinder::apply_mechanical_step_cfg(
                config,
                &mut cylinders[i],
                i,
                angle_old,
                angle_new,
                omega,
                dt,
                oil,
                oil_config,
                wear_time_scale,
            );
            total_friction_torque += mech.friction_torque;
            total_friction_heat_w += mech.friction_heat_w;
            total_oil_consumed += mech.oil_consumed;
            if mech.failed { any_failed = true; }
        }

        // Step the oil reservoir with the heat we just collected.
        core.oil.step(&core.oil_config, omega, total_friction_heat_w, 0.0, dt);
        if total_oil_consumed > 0.0 {
            core.oil.consume(total_oil_consumed);
        }
        last_friction_heat_w = total_friction_heat_w;

        // Latch a permanent seizure if anyone failed.
        if any_failed {
            core.engine_seized = true;
        }

        // ── Friction + starter + soft rev-limit ──────────────────────────
        let mut tau_total = total_torque_from_gas;
        tau_total -= core.config.friction_torque(core.omega);
        tau_total -= total_friction_torque; // material-aware ring/wall drag
        tau_total += core.config.starter_torque_at(rpm, core.starter_active);

        if rpm > core.config.redline_rpm {
            tau_total -= ((rpm - core.config.redline_rpm) / 200.0).min(2.0) * 60.0;
        }

        // ── Integrate the only true degree of freedom: the crank ─────────
        core.omega += tau_total / core.config.flywheel_inertia * dt;
        if core.omega < 0.0 {
            core.omega = 0.0;
        }
        let dtheta = core.omega * dt;
        core.angle = (core.angle + dtheta).rem_euclid(TAU);
        core.fourstroke_angle = (core.fourstroke_angle + dtheta).rem_euclid(2.0 * TAU);

        last_torque = tau_total;
        total_work += tau_total * core.omega * dt;

        // Push audio sample
        if core.audio_enabled {
            let exhaust_pressure = core.exhaust.pressure();
            // Atmospheric pressure is 101325.0 Pa. Scale it down for audio (-1.0 to 1.0)
            let audio_sample = (exhaust_pressure - 101325.0) * 0.00005;
            audio_buffer.push((dt, audio_sample));
        }
    }

    if let Some(tx) = audio_tx {
        if core.audio_enabled && !audio_buffer.is_empty() {
            if let Ok(mut buffer) = tx.buffer.try_lock() {
                buffer.extend(audio_buffer);
            }
        }
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
    core.friction_heat_smoothed += (last_friction_heat_w - core.friction_heat_smoothed) * 0.06;
}
