//! Dynamometer (dyno) testing system.
//!
//! Simulates an absorption dynamometer (water brake / eddy-current) that holds
//! the engine against a **linearly increasing RPM ramp** while at wide-open
//! throttle.  The target RPM rises smoothly from `start_rpm` to `end_rpm` over
//! the sweep duration — no jagged steps, just like a real inertia dyno.
//!
//! The dyno also simulates an external **oil cooler** so the engine doesn't
//! overheat during the sustained WOT pull.
//!
//! A PID controller adjusts the braking torque each frame to track the ramp.
//! Torque & power are continuously sampled and recorded at regular RPM
//! intervals to build the curve.

use bevy::prelude::*;
use std::f32::consts::TAU;

use super::state::{EngineCore, RunState};
use super::thermo::T_ATM;

// ═══════════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════════

/// One recorded data-point from the dyno sweep.
#[derive(Clone, Debug)]
pub struct DynoSample {
    pub rpm:       f32,
    pub torque_nm: f32,
    pub power_kw:  f32,
    pub power_hp:  f32,
}

/// Which phase of the sweep the dyno is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynoPhase {
    /// Not running — waiting for user to press Start.
    Idle,
    /// The ramp is in progress — RPM target rising linearly.
    Sweeping,
    /// Sweep finished — results available.
    Complete,
}

/// The full dyno state, stored as a Bevy [`Resource`].
#[derive(Resource)]
pub struct DynoState {
    // ── Run control ─────────────────────────────────────────────────────────
    pub active: bool,
    pub phase:  DynoPhase,

    // ── Sweep parameters (configurable from the UI) ─────────────────────────
    pub start_rpm: f32,
    pub end_rpm:   f32,
    /// RPM interval at which data points are recorded (for the graph).
    pub sample_interval: f32,
    /// How many RPM the target climbs per second of sim-time.
    pub ramp_rate: f32,

    // ── Internal state ──────────────────────────────────────────────────────
    pub target_rpm: f32,
    /// RPM at which the next sample will be recorded.
    pub next_sample_rpm: f32,
    /// Accumulator for averaging between sample points.
    pub torque_accumulator: f32,
    pub power_accumulator:  f32,
    pub accumulator_count:  u32,
    /// Total sim-time elapsed during the sweep.
    pub sweep_elapsed: f32,

    // ── PID controller for absorption brake ─────────────────────────────────
    pub pid_kp: f32,
    pub pid_ki: f32,
    pub pid_kd: f32,
    pub pid_integral:   f32,
    pub pid_prev_error: f32,

    /// The braking torque the dyno is currently applying (Nm, always ≥ 0).
    pub absorption_torque: f32,

    // ── Results ─────────────────────────────────────────────────────────────
    pub results:         Vec<DynoSample>,
    pub peak_hp:         f32,
    pub peak_hp_rpm:     f32,
    pub peak_torque:     f32,
    pub peak_torque_rpm: f32,

    /// Name of the engine config that was tested (for the graph title).
    pub tested_engine_name: String,
}

impl Default for DynoState {
    fn default() -> Self {
        Self {
            active: false,
            phase:  DynoPhase::Idle,

            start_rpm: 1000.0,
            end_rpm:   8000.0,
            sample_interval: 100.0,
            ramp_rate: 400.0,  // RPM/s — takes ~17.5s to sweep 1000→8000

            target_rpm:        1000.0,
            next_sample_rpm:   1000.0,
            torque_accumulator: 0.0,
            power_accumulator:  0.0,
            accumulator_count:  0,
            sweep_elapsed:     0.0,

            pid_kp: 0.8,
            pid_ki: 0.3,
            pid_kd: 0.02,
            pid_integral:   0.0,
            pid_prev_error: 0.0,

            absorption_torque: 0.0,

            results:         Vec::new(),
            peak_hp:         0.0,
            peak_hp_rpm:     0.0,
            peak_torque:     0.0,
            peak_torque_rpm: 0.0,
            tested_engine_name: String::new(),
        }
    }
}

impl DynoState {
    /// Begin a new dyno sweep.  The engine must already be running.
    pub fn start(&mut self, engine_name: &str, redline: f32) {
        self.active = true;
        self.phase = DynoPhase::Sweeping;
        self.end_rpm = redline;
        self.target_rpm = self.start_rpm;
        self.next_sample_rpm = self.start_rpm;
        self.torque_accumulator = 0.0;
        self.power_accumulator = 0.0;
        self.accumulator_count = 0;
        self.sweep_elapsed = 0.0;
        self.pid_integral = 0.0;
        self.pid_prev_error = 0.0;
        self.absorption_torque = 0.0;
        self.results.clear();
        self.peak_hp = 0.0;
        self.peak_hp_rpm = 0.0;
        self.peak_torque = 0.0;
        self.peak_torque_rpm = 0.0;
        self.tested_engine_name = engine_name.to_string();
    }

    /// Abort the sweep and release the brake.
    pub fn stop(&mut self) {
        self.active = false;
        self.phase = DynoPhase::Idle;
        self.absorption_torque = 0.0;
        self.pid_integral = 0.0;
    }

    /// Compute the PID-controlled braking torque to hold `current_rpm` at
    /// `target_rpm`.  Returns the absorption torque (always ≥ 0).
    fn pid_update(&mut self, current_rpm: f32, dt: f32) -> f32 {
        let error = current_rpm - self.target_rpm;
        self.pid_integral += error * dt;
        // Anti-windup: clamp integral
        self.pid_integral = self.pid_integral.clamp(-500.0, 500.0);
        let derivative = if dt > 0.0 { (error - self.pid_prev_error) / dt } else { 0.0 };
        self.pid_prev_error = error;

        let output = self.pid_kp * error + self.pid_ki * self.pid_integral + self.pid_kd * derivative;
        // Absorption torque can only brake (positive = opposing rotation)
        output.max(0.0)
    }

    /// Record a sample and track peaks.
    fn record_sample(&mut self) {
        if self.accumulator_count == 0 { return; }
        let n = self.accumulator_count as f32;
        let avg_torque = self.torque_accumulator / n;
        let avg_power_w = self.power_accumulator / n;
        let avg_power_kw = avg_power_w / 1000.0;
        let avg_power_hp = avg_power_kw * 1.341;

        let sample = DynoSample {
            rpm: self.next_sample_rpm,
            torque_nm: avg_torque.max(0.0),
            power_kw: avg_power_kw.max(0.0),
            power_hp: avg_power_hp.max(0.0),
        };

        if sample.power_hp > self.peak_hp {
            self.peak_hp = sample.power_hp;
            self.peak_hp_rpm = sample.rpm;
        }
        if sample.torque_nm > self.peak_torque {
            self.peak_torque = sample.torque_nm;
            self.peak_torque_rpm = sample.rpm;
        }

        self.results.push(sample);
        self.torque_accumulator = 0.0;
        self.power_accumulator = 0.0;
        self.accumulator_count = 0;
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Bevy system
// ═══════════════════════════════════════════════════════════════════════════════

/// Runs **after** `engine_step`.  Drives the linear RPM ramp, PID brake,
/// continuous sampling, and oil cooling.
pub fn dyno_system(
    time: Res<Time>,
    mut core: ResMut<EngineCore>,
    mut dyno: ResMut<DynoState>,
) {
    if !dyno.active {
        return;
    }

    // If the engine stalled during the sweep, abort.
    if core.run_state != RunState::Running {
        dyno.stop();
        return;
    }

    let dt = time.delta_seconds().min(1.0 / 30.0) * core.time_scale;
    if dt <= 0.0 { return; }

    let current_rpm = core.rpm();

    // Force WOT while dyno is active
    core.throttle = 1.0;

    match dyno.phase {
        DynoPhase::Idle | DynoPhase::Complete => {
            dyno.absorption_torque = 0.0;
            return;
        }

        DynoPhase::Sweeping => {
            // ── Linearly ramp the target RPM ────────────────────────────────
            dyno.sweep_elapsed += dt;
            dyno.target_rpm = (dyno.start_rpm + dyno.ramp_rate * dyno.sweep_elapsed)
                .min(dyno.end_rpm);

            // ── PID braking torque ──────────────────────────────────────────
            dyno.absorption_torque = dyno.pid_update(current_rpm, dt);

            // ── Continuous sampling ─────────────────────────────────────────
            // Measure the engine's output torque
            // We now smooth the flywheel torque BEFORE the dyno brake is applied in engine.rs
            let torque = core.torque_smoothed;
            let omega = current_rpm * TAU / 60.0;
            let power_w = torque * omega;
            dyno.torque_accumulator += torque;
            dyno.power_accumulator += power_w;
            dyno.accumulator_count += 1;

            // When the target crosses the next sample boundary, record a point.
            if dyno.target_rpm >= dyno.next_sample_rpm + dyno.sample_interval {
                dyno.record_sample();
                dyno.next_sample_rpm += dyno.sample_interval;
            }

            // ── Check sweep completion ──────────────────────────────────────
            if dyno.target_rpm >= dyno.end_rpm {
                // Record whatever is left in the accumulator as the final point
                dyno.next_sample_rpm = dyno.end_rpm;
                dyno.record_sample();

                dyno.phase = DynoPhase::Complete;
                dyno.active = false;
                dyno.absorption_torque = 0.0;
                dyno.pid_integral = 0.0;
            }
        }
    }
}
