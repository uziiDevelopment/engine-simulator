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

pub mod bearing;
pub mod config;
mod crank;
mod cylinder;
pub mod dyno;
mod fuel;
mod geometry;
mod manifold;
pub mod material;
pub mod oil;
mod state;
mod thermo;
mod valve;

pub use bearing::*;
pub use config::*;
pub use crank::*;
pub use cylinder::*;
pub use dyno::*;
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
            .insert_resource(DynoState::default())
            .add_systems(Update, (engine_input, engine_step, dyno::dyno_system).chain());
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
    dyno: Res<DynoState>,
    audio_tx: Option<ResMut<crate::audio::AudioTx>>,
) {
    let frame_dt = time.delta_seconds().min(1.0 / 30.0) * core.time_scale;
    let substeps: usize = 80;
    let dt = frame_dt / substeps as f32;
    if dt <= 0.0 {
        return;
    }

    let mut torque_sum = 0.0_f32;  // accumulated across substeps for per-frame average
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
        let mut total_block_heat_to_oil_w = 0.0_f32;
        let mut total_oil_consumed = 0.0_f32;
        let mut any_seized = false;
        let mut new_seizure_reason = String::new();
        let mut substep_knock = 0.0_f32;
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
                omega,
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
            total_block_heat_to_oil_w += mech.block_heat_to_oil_w;
            total_oil_consumed += mech.oil_consumed;
            
            // If the rod is snapped, it flails around and adds massive mechanical drag.
            if rod_dmg_before >= 1.0 {
                total_friction_torque += 150.0;
            }

            if mech.seized {
                any_seized = true;
                if new_seizure_reason.is_empty() {
                    new_seizure_reason = format!("Cylinder {} melted/galled", i + 1);
                }
            }
            if mech.bearing_seized {
                any_seized = true;
                if new_seizure_reason.is_empty() {
                    new_seizure_reason = format!("Cylinder {} bearing seized", i + 1);
                }
            }
        }

        // ── Step rod bearings (one per cylinder) ───────────────────────────
        {
            let EngineCore {
                ref config, ref mut rod_bearings,
                ref oil, ref oil_config, ref cylinders, ..
            } = *core;
            for i in 0..num_cyl {
                if i >= rod_bearings.len() { break; }
                // Rod bearing load = gas force + inertia on the crank pin.
                let p_cyl = cylinders[i].last_pressure;
                let piston_area = config.piston_area();
                let gas_force = (p_cyl - P_ATM).abs() * piston_area;
                let piston_mass = config.materials.piston.density * piston_area * 0.04;
                let crank_r = config.crank_radius();
                let phase = config.crank_phases[i];
                let theta_mid = 0.5 * (angle_old + angle_new);
                let inertia_force = (piston_mass * omega * omega * crank_r
                    * (theta_mid + phase).cos()).abs();
                let rod_load = gas_force + inertia_force;

                let brg_result = bearing::step_bearing(
                    &config.materials.rod_bearing,
                    &mut rod_bearings[i],
                    rod_load,
                    omega,
                    oil,
                    oil_config,
                    dt,
                    wear_time_scale,
                );
                total_friction_torque += brg_result.friction_torque;
                total_friction_heat_w += brg_result.heat_to_oil_w;
                substep_knock += brg_result.knock_impulse;
                if brg_result.seized {
                    any_seized = true;
                    if new_seizure_reason.is_empty() {
                        new_seizure_reason = format!("Rod bearing {} spun/wiped", i + 1);
                    }
                }
            }
        }

        // ── Step main bearings ────────────────────────────────────────
        {
            let EngineCore {
                ref config, ref mut main_bearings,
                ref oil, ref oil_config, ..
            } = *core;
            // Aggregate crank load shared across main bearings.
            // Rough estimate: total gas torque / crank radius, split evenly.
            let n_mains = main_bearings.len().max(1);
            let crank_r = config.crank_radius().max(0.01);
            let total_crank_load = (total_torque_from_gas.abs() / crank_r)
                + (config.materials.piston.density * config.piston_area() * 0.04
                   * num_cyl as f32 * omega * omega * crank_r);
            let load_per_main = total_crank_load / n_mains as f32;

            for (i, brg) in main_bearings.iter_mut().enumerate() {
                let result = bearing::step_bearing(
                    &config.materials.main_bearing,
                    brg,
                    load_per_main,
                    omega,
                    oil,
                    oil_config,
                    dt,
                    wear_time_scale,
                );
                total_friction_torque += result.friction_torque;
                total_friction_heat_w += result.heat_to_oil_w;
                substep_knock += result.knock_impulse;
                if result.seized {
                    any_seized = true;
                    if new_seizure_reason.is_empty() {
                        new_seizure_reason = format!("Main bearing {} spun/wiped", i + 1);
                    }
                }
            }
        }

        // ── Step cam bearings ─────────────────────────────────────────
        {
            let EngineCore {
                ref config, ref mut cam_bearings,
                ref oil, ref oil_config, ..
            } = *core;
            // Cam runs at half crank speed; load is light (valve springs).
            let cam_omega = omega * 0.5;
            let cam_load = 200.0 * num_cyl as f32; // ~200 N per valve spring

            for (i, brg) in cam_bearings.iter_mut().enumerate() {
                let result = bearing::step_bearing(
                    &config.materials.cam_bearing,
                    brg,
                    cam_load,
                    cam_omega,
                    oil,
                    oil_config,
                    dt,
                    wear_time_scale,
                );
                total_friction_torque += result.friction_torque;
                total_friction_heat_w += result.heat_to_oil_w;
                if result.seized {
                    any_seized = true;
                    if new_seizure_reason.is_empty() {
                        new_seizure_reason = format!("Cam bearing {} spun/wiped", i + 1);
                    }
                }
            }
        }
        // Step the oil reservoir with the heat we just collected.
        // Split-borrow: take refs to the two disjoint fields explicitly.
        let EngineCore { ref oil_config, ref mut oil, .. } = *core;
        oil.step(oil_config, omega, total_friction_heat_w, total_block_heat_to_oil_w, dt);
        if total_oil_consumed > 0.0 {
            oil.consume(total_oil_consumed);
        }
        last_friction_heat_w = total_friction_heat_w;

        // Latch a permanent seizure if anyone locked up thermally.
        if any_seized && !core.engine_seized {
            core.engine_seized = true;
            core.seizure_reason = new_seizure_reason;
        }

        // ── Friction + starter + soft rev-limit ──────────────────────────
        let mut tau_total = total_torque_from_gas;
        tau_total -= core.config.friction_torque(core.omega);
        tau_total -= total_friction_torque; // material-aware ring/wall drag
        tau_total += core.config.starter_torque_at(rpm, core.starter_active);

        if rpm > core.config.redline_rpm {
            tau_total -= ((rpm - core.config.redline_rpm) / 200.0).min(2.0) * 60.0;
        }

        // The pure engine output torque (flywheel torque)
        let engine_flywheel_torque = tau_total;

        // ── Dyno absorption brake ────────────────────────────────────────
        if dyno.active {
            // The PID applies positive torque to brake the engine.
            // Since engine spins with omega > 0, we subtract it.
            tau_total -= dyno.absorption_torque;
        }

        // ── Integrate the only true degree of freedom: the crank ─────────
        core.omega += tau_total / core.config.flywheel_inertia * dt;
        if core.omega < 0.0 {
            core.omega = 0.0;
        }
        let dtheta = core.omega * dt;
        core.angle = (core.angle + dtheta).rem_euclid(TAU);
        core.fourstroke_angle = (core.fourstroke_angle + dtheta).rem_euclid(2.0 * TAU);

        torque_sum += engine_flywheel_torque;
        total_work += tau_total * core.omega * dt;

        // Push audio sample — exhaust pressure + bearing knock impulse spikes.
        if core.audio_enabled {
            let exhaust_pressure = core.exhaust.pressure();
            let base_sample = (exhaust_pressure - 101325.0) * 0.00005;
            // Knock is injected as a sharp transient additive to the pressure wave.
            // Scale chosen so a knock_impulse of 1.0 produces a clearly audible
            // thud without completely overwhelming the exhaust note.
            let knock_sample = substep_knock.clamp(0.0, 1.0) * 0.4;
            audio_buffer.push((dt, base_sample + knock_sample));
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
    // Average torque across all substeps (not just the last one) so the EMA
    // input is the mean over a full frame of crank rotation, not one snapshot.
    let avg_frame_torque = torque_sum / substeps as f32;
    let alpha = 0.06;
    core.torque_smoothed += (avg_frame_torque - core.torque_smoothed) * alpha;
    core.power_smoothed += ((avg_frame_torque * core.omega) - core.power_smoothed) * alpha;
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
