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
pub mod cooling;
mod cylinder;
pub mod dyno;
mod fuel;
pub mod gearbox;
mod geometry;
mod manifold;
pub mod material;
pub mod oil;
mod state;
mod thermo;
pub mod turbo;
mod valve;

pub use bearing::*;
pub use config::*;
pub use cooling::*;
pub use crank::*;
pub use cylinder::*;
pub use dyno::*;
pub use fuel::*;
pub use gearbox::*;
pub use geometry::*;
pub use manifold::*;
pub use material::*;
pub use oil::*;
pub use state::*;
pub use thermo::*;
pub use turbo::*;
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
fn engine_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut core: ResMut<EngineCore>,
) {
    core.starter_active = keys.pressed(KeyCode::KeyE) && core.run_state != RunState::Running;

    // Throttle bound to W: hold W → ramps toward 100%, release → ramps back
    // toward 0%. Press rate is slower than release so the pedal feels weighty
    // going down but lifts crisply.
    let w_held = keys.pressed(KeyCode::KeyW);
    let dt_input = time.delta_seconds();
    const THROTTLE_PRESS_RATE: f32 = 1.6;   // 0 → 1 in ~0.62 s
    const THROTTLE_RELEASE_RATE: f32 = 4.0; // 1 → 0 in ~0.25 s

    // Holding Shift: dip clutch fully and snap throttle to 0 (safe shift
    // gesture, overrides W). Releasing Shift re-engages the clutch; the
    // throttle then resumes ramping toward whatever W is doing.
    let shift_held = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let shift_released =
        keys.just_released(KeyCode::ShiftLeft) || keys.just_released(KeyCode::ShiftRight);
    if shift_held {
        core.clutch_engagement = 0.0;
        core.throttle = 0.0;
    } else {
        if shift_released {
            core.clutch_engagement = 1.0;
        }
        let target = if w_held { 1.0 } else { 0.0 };
        let rate = if w_held { THROTTLE_PRESS_RATE } else { THROTTLE_RELEASE_RATE };
        let max_step = rate * dt_input;
        let delta = (target - core.throttle).clamp(-max_step, max_step);
        core.throttle = (core.throttle + delta).clamp(0.0, 1.0);
    }

    // ── Number / letter shortcuts: snap lever directly to that gate ──────
    use crate::engine::gearbox::{
        constrain_lever, gate_for_position, snapped_lever_for, GearSelector,
    };
    let new_sel = if keys.just_pressed(KeyCode::Digit0) || keys.just_pressed(KeyCode::KeyN) {
        Some(GearSelector::Neutral)
    } else if keys.just_pressed(KeyCode::Digit1) { Some(GearSelector::Gear(1)) }
    else if keys.just_pressed(KeyCode::Digit2) { Some(GearSelector::Gear(2)) }
    else if keys.just_pressed(KeyCode::Digit3) { Some(GearSelector::Gear(3)) }
    else if keys.just_pressed(KeyCode::Digit4) { Some(GearSelector::Gear(4)) }
    else if keys.just_pressed(KeyCode::Digit5) { Some(GearSelector::Gear(5)) }
    else if keys.just_pressed(KeyCode::Digit6) { Some(GearSelector::Gear(6)) }
    else if keys.just_pressed(KeyCode::KeyR) { Some(GearSelector::Reverse) }
    else { None };

    if let Some(sel) = new_sel {
        try_select_gear(&mut *core, sel);
        core.gearbox.lever_pos = snapped_lever_for(sel);
        if matches!(sel, GearSelector::Reverse) {
            core.gearbox.reverse_armed = true;
        } else if !matches!(sel, GearSelector::Neutral) {
            core.gearbox.reverse_armed = false;
        }
    }

    // ── Arrow-key shifter: fully discrete H-pattern moves ───────────────
    //
    // Each tap snaps the lever to the next gate position. Real-car semantics:
    //   • Up/Down on rail      → push lever into top/bottom gate of current column
    //   • Up/Down while in gate → pull lever back to neutral on the same column
    //   • Left/Right on rail   → step one column over
    //   • Left/Right in gate   → ignored (gate plate blocks it)
    let _ = time.delta_seconds();
    let cur = core.gearbox.lever_pos;
    let on_rail = cur.y.abs() < 0.35;
    let col_idx: usize = if cur.x < -0.5 { 0 } else if cur.x > 0.5 { 2 } else { 1 };
    let col_centres = [-1.0_f32, 0.0, 1.0];
    let mut next_lever: Option<Vec2> = None;

    if keys.just_pressed(KeyCode::ArrowUp) {
        if on_rail {
            if matches!(core.gearbox.selector, GearSelector::Neutral)
                && col_idx == 0
                && core.gearbox.reverse_armed
            {
                next_lever = Some(Vec2::new(-1.4, 1.3));
            } else {
                if matches!(core.gearbox.selector, GearSelector::Neutral) && col_idx == 1 {
                    core.gearbox.reverse_armed = true;
                }
                next_lever = Some(Vec2::new(col_centres[col_idx], 1.0));
            }
        } else if cur.y < 0.0 {
            next_lever = Some(Vec2::new(cur.x, 0.0));
        }
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        if on_rail {
            next_lever = Some(Vec2::new(col_centres[col_idx], -1.0));
        } else if cur.y > 0.0 {
            next_lever = Some(Vec2::new(cur.x, 0.0));
        }
    }
    if keys.just_pressed(KeyCode::ArrowLeft) && on_rail {
        let new_col = col_idx.saturating_sub(1);
        next_lever = Some(Vec2::new(col_centres[new_col], 0.0));
    }
    if keys.just_pressed(KeyCode::ArrowRight) && on_rail {
        let new_col = (col_idx + 1).min(2);
        next_lever = Some(Vec2::new(col_centres[new_col], 0.0));
    }

    if let Some(next) = next_lever {
        core.gearbox.lever_pos = next;
        let sel = gate_for_position(next, core.gearbox.reverse_armed);
        if sel != core.gearbox.selector {
            try_select_gear(&mut *core, sel);
            if !matches!(sel, GearSelector::Neutral | GearSelector::Reverse) {
                core.gearbox.reverse_armed = false;
            }
        }
    }
    // Keep constrain_lever referenced from this module (still used by mouse path
    // via re-export); silence the unused-import warning when keyboard path drops it.
    let _ = constrain_lever;
}

/// Apply a gear selection from any input source, running the no-clutch grind
/// check before committing.
pub fn try_select_gear(core: &mut EngineCore, sel: crate::engine::gearbox::GearSelector) {
    if core.gearbox.selector == sel { return; }
    // No-clutch grind: substantial engagement + non-zero slip at the input
    // shaft makes the change a destructive hard-engage.
    if core.clutch_engagement > 0.2 && sel != crate::engine::gearbox::GearSelector::Neutral {
        let cfg_ratio = core.gearbox_config.total_ratio(sel);
        if let Some(r) = cfg_ratio {
            let target_input_omega = core.gearbox.vehicle_omega * r;
            let slip = core.omega - target_input_omega;
            if slip.abs() > 30.0 {
                let impulse = crate::engine::gearbox::apply_grind_shock(&mut core.gearbox, slip);
                // Spike both shafts away from each other for a brief visual jolt
                core.omega = (core.omega - impulse.signum() * impulse.min(50.0)).max(0.0);
            }
        }
    }
    core.gearbox.selector = sel;
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

    let mut audio_buffer = Vec::<crate::audio::AudioSample>::with_capacity(substeps);

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
        let mut total_block_heat_to_coolant_w = 0.0_f32;
        let mut total_oil_consumed = 0.0_f32;
        let mut any_seized = false;
        let mut new_seizure_reason = String::new();
        let mut substep_knock = 0.0_f32;
        let num_cyl = core.config.num_cylinders;
        let wear_time_scale = core.wear_time_scale;
        // Snapshot coolant values before the split-borrow (plain f32 copies).
        let coolant_temp = core.coolant.temperature;
        let coolant_transfer_coeff =
            core.coolant.block_transfer_factor(&core.coolant_config, omega);

        let EngineCore {
            ref config, ref fuel,
            ref mut cylinders,
            ref mut intake, ref mut exhaust,
            ref mut turbos,
            ref oil_config, ref oil,
            ..
        } = *core;

        if config.turbo_enabled() {
            // Turbo path: each turbo's turbine pulls from exhaust + spins compressor.
            // Multiple turbos work in parallel, sharing exhaust flow and merging boost.
            let throttle_area = config.throttle_area(throttle_eff);

            // Count enabled turbos for exhaust flow sharing
            let enabled_count = config.turbos.iter()
                .enumerate()
                .filter(|(i, cfg)| cfg.enabled && *i < turbos.len())
                .count() as f32;

            // Each turbo sees an equal fraction of the exhaust flow
            let exhaust_fraction = if enabled_count > 0.0 { 1.0 / enabled_count } else { 1.0 };

            // Step all enabled turbos, collecting exhaust mass flow
            let mut total_exhaust_flow = 0.0_f32;
            for (i, turbo) in turbos.iter_mut().enumerate() {
                if i < config.turbos.len() && config.turbos[i].enabled {
                    let m_dot = turbo::step_turbo(
                        &config.turbos[i], turbo, intake, exhaust,
                        exhaust_fraction, throttle_eff, dt
                    );
                    total_exhaust_flow += m_dot;
                }
            }

            // Apply total exhaust mass flow (all turbos combined)
            exhaust.mass = (exhaust.mass - total_exhaust_flow * dt).max(1e-9);
            // Newton-cool toward ambient pipe temperature.
            let cool_rate = (1.6 * dt).min(1.0);
            exhaust.temperature += (thermo::T_EXH_AMBIENT - exhaust.temperature) * cool_rate;
            exhaust.flow_signal = exhaust.flow_signal * 0.6 + total_exhaust_flow * 0.4;

            // Find the turbo with highest boost pressure for throttle flow
            // (in reality they merge to a common plenum, so use the highest pressure)
            let highest_boost_idx = turbos.iter().enumerate()
                .filter(|(i, _)| i < &config.turbos.len() && config.turbos[*i].enabled)
                .max_by(|(_, a), (_, b)| {
                    a.boost.pressure().partial_cmp(&b.boost.pressure()).unwrap()
                })
                .map(|(i, _)| i);

            if let Some(idx) = highest_boost_idx {
                if let Some(turbo) = turbos.get_mut(idx) {
                    turbo::throttle_flow_boosted(throttle_area, &mut turbo.boost, intake, throttle_eff, dt);
                }
            }
        } else {
            manifold::throttle_flow_cfg(config, intake, throttle_eff, dt);
            manifold::exhaust_to_atmosphere_cfg(config, exhaust, dt);
        }

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
                coolant_temp,
                coolant_transfer_coeff,
            );
            total_friction_torque += mech.friction_torque;
            total_friction_heat_w += mech.friction_heat_w;
            total_block_heat_to_oil_w += mech.block_heat_to_oil_w;
            total_block_heat_to_coolant_w += mech.block_heat_to_coolant_w;
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
        {
            let EngineCore { ref oil_config, ref mut oil, .. } = *core;
            oil.step(oil_config, omega, total_friction_heat_w, total_block_heat_to_oil_w, dt);
            if total_oil_consumed > 0.0 {
                oil.consume(total_oil_consumed);
            }
        }
        last_friction_heat_w = total_friction_heat_w;

        // Step the coolant loop.
        {
            let already_seized = core.engine_seized;
            let EngineCore { ref coolant_config, ref mut coolant, .. } = *core;
            coolant.step(coolant_config, omega, total_block_heat_to_coolant_w, dt);
            if !already_seized && coolant.temperature >= coolant_config.boilover_k {
                any_seized = true;
                if new_seizure_reason.is_empty() {
                    new_seizure_reason = format!(
                        "Coolant boilover at {:.0} °C — head gasket failure",
                        coolant.temperature - 273.15,
                    );
                }
            }
        }

        // Overheat penalties: thermal expansion tightens clearances (extra
        // friction) and pre-ignition/detonation derate combustion output.
        let overheat = core.coolant.overheat_factor(&core.coolant_config);
        if overheat > 0.0 {
            total_friction_torque += overheat * 15.0 * num_cyl as f32;
            total_torque_from_gas  *= 1.0 - overheat * 0.30;
        }

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

        // ── Clutch torque (slip × stiffness, clamped by engagement/fade/wear) ─
        let slip = core.omega - core.drivetrain_omega;

        // Heat-based fade: capacity drops above ~500K (227°C)
        let fade_factor = ((1000.0 - core.clutch_temp) / 500.0).clamp(0.0, 1.0);
        let wear_factor = crate::engine::gearbox::clutch_wear_factor(&core.gearbox);

        // Torque capacity scales with engagement, thermal fade, and irreversible wear
        let max_clutch_torque =
            core.clutch_engagement * core.config.clutch_max_torque * fade_factor * wear_factor;

        // The clutch tries to zero the slip.
        let clutch_torque = (slip * 50.0).clamp(-max_clutch_torque, max_clutch_torque);

        // Heat generation (P = T * Δω) and cooling (Newton's law)
        let heat_generated_w = (clutch_torque * slip).abs();
        let cooling_w = (core.clutch_temp - 300.0) * core.config.clutch_cooling_coeff;

        let net_heat_w = heat_generated_w - cooling_w;
        core.clutch_temp += (net_heat_w / core.config.clutch_thermal_mass) * dt;
        core.clutch_temp = core.clutch_temp.max(300.0);

        // Clutch wear (irreversible) — extends the existing thermal model
        let clutch_temp_snap = core.clutch_temp;
        crate::engine::gearbox::step_clutch_wear(&mut core.gearbox, clutch_temp_snap, dt);

        // Engine loses torque to the clutch
        tau_total -= clutch_torque;

        // ── Gearbox + vehicle load ───────────────────────────────────────
        // Snapshot inputs we need before the mutable borrow of `core.gearbox`.
        let redline_omega = core.config.redline_rpm * TAU / 60.0;
        let drivetrain_inertia_base = core.config.drivetrain_inertia;
        let drivetrain_omega_now = core.drivetrain_omega;

        let gb_cfg = core.gearbox_config.clone();
        let gb_out = crate::engine::gearbox::step_gearbox(
            &gb_cfg,
            &mut core.gearbox,
            clutch_torque,
            drivetrain_inertia_base,
            drivetrain_omega_now,
            redline_omega,
            dt,
        );

        let mut drivetrain_tau = gb_out.drivetrain_tau;

        // ── Dyno absorption brake ────────────────────────────────────────
        if dyno.active {
            drivetrain_tau -= dyno.absorption_torque;
        }

        // Gearbox damage: above 0.9 it drags significantly
        if core.gearbox.gearbox_damage > 0.9 {
            drivetrain_tau -= drivetrain_omega_now.signum() * 30.0 * core.gearbox.gearbox_damage;
        }

        // Money-shift seizure: spike rod_damage on a representative cylinder so
        // the existing failure pipeline (bearing.rs / cylinder.rs) takes over.
        if gb_out.money_shift && !core.cylinders.is_empty() {
            let idx = core.cylinders.len() / 2;
            core.cylinders[idx].rod_damage = 1.0;
            core.engine_seized = true;
            core.seizure_reason = "Money-shift: engine over-revved past redline".to_string();
        }

        let drivetrain_accel = drivetrain_tau / gb_out.drivetrain_inertia_eff.max(1e-3);
        core.drivetrain_omega += drivetrain_accel * dt;

        // Allow reverse drivetrain omega only if engaged in reverse; otherwise clamp >= 0
        let in_reverse = matches!(core.gearbox.selector, crate::engine::gearbox::GearSelector::Reverse);
        if !in_reverse && core.drivetrain_omega < 0.0 {
            core.drivetrain_omega = 0.0;
        }

        let d_dt_angle = core.drivetrain_omega * dt;
        core.drivetrain_angle = (core.drivetrain_angle + d_dt_angle).rem_euclid(TAU);

        // Propagate to vehicle speed + cosmetic cog angles
        let eng_omega = core.omega;
        let drv_omega = core.drivetrain_omega;
        crate::engine::gearbox::post_integrate(&gb_cfg, &mut core.gearbox, eng_omega, drv_omega, dt);

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

        // Push audio sample — raw pressures + RPM + bearing knock impulse.
        // For multi-turbo, use the first enabled turbo's data for audio.
        if core.audio_enabled {
            let first_turbo = core.turbos.iter().enumerate()
                .find(|(i, _)| core.config.turbos.get(*i).map(|c| c.enabled).unwrap_or(false));

            audio_buffer.push(crate::audio::AudioSample {
                dt,
                exhaust_pressure: core.exhaust.pressure(),
                intake_pressure:  core.intake.pressure(),
                knock:            substep_knock.clamp(0.0, 1.0),
                rpm:              core.rpm(),
                turbo_enabled:    core.config.turbo_enabled(),
                turbo_shaft_rpm:  first_turbo.map(|(_, t)| t.shaft_rpm()).unwrap_or(0.0),
                boost_pa:         first_turbo.map(|(_, t)| t.boost_gauge_pa()).unwrap_or(0.0),
                bov_envelope:     first_turbo.map(|(_, t)| t.bov_envelope).unwrap_or(0.0),
                blade_count:      first_turbo
                    .and_then(|(i, _)| core.config.turbos.get(i))
                    .map(|c| c.blade_count)
                    .unwrap_or(11),
            });
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
    core.coolant_temp_smoothed +=
        (core.coolant.temperature - core.coolant_temp_smoothed) * alpha;
    core.throttle_smoothed += (core.throttle - core.throttle_smoothed) * alpha;
}
