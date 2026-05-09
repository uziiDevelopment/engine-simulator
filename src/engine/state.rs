//! The [`EngineCore`] resource — the assembled simulation state.
//!
//! Cylinders, manifolds, crankshaft, fuel, throttle, time-scale, and smoothed
//! telemetry all live here.  Mutated only by [`crate::engine::engine_step`];
//! everything else (visuals, UI) reads.

use bevy::prelude::*;
use std::f32::consts::TAU;

use super::cylinder::CylinderState;
use super::fuel::{Fuel, FUELS};
use super::geometry::{CRANK_PHASES, NUM_CYL};
use super::manifold::{make_exhaust_manifold, make_intake_manifold, Manifold};
use super::thermo::P_ATM;

/// High-level engine run state.
#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum RunState {
    /// Sitting still.  Press **E** to engage the starter.
    Off,
    /// Starter engaged; combustion takes over above `COMBUSTION_RPM_MIN`.
    Cranking,
    /// Engine sustained by combustion.  Falls back to `Off` if RPM stalls.
    Running,
}

#[derive(Resource)]
pub struct EngineCore {
    // Crank (the sole rotational DOF)
    pub angle:            f32,    // 0..2π
    pub fourstroke_angle: f32,    // 0..4π
    pub omega:            f32,    // rad/s

    // Player inputs / setup
    pub run_state:      RunState,
    pub starter_active: bool,
    pub throttle:       f32,      // 0..=1
    pub time_scale:     f32,      // 1.0 = real-time, < 1 = slow-mo
    pub fuel:           Fuel,
    pub fuel_idx:       usize,

    // Sub-models
    pub cylinders: [CylinderState; NUM_CYL],
    pub intake:    Manifold,
    pub exhaust:   Manifold,

    // Smoothed telemetry (for the UI; updated each frame)
    pub torque_smoothed:           f32, // Nm
    pub power_smoothed:            f32, // W
    pub map_smoothed:              f32, // Pa
    pub afr_smoothed:              f32,
    pub exhaust_pressure_smoothed: f32, // Pa
    pub exhaust_temp_smoothed:     f32, // K
    pub last_frame_fuel_kg:        f32,
    pub last_frame_work_j:         f32,
}

impl EngineCore {
    pub fn new(fuel_idx: usize) -> Self {
        let cylinders: [CylinderState; NUM_CYL] =
            std::array::from_fn(|i| CylinderState::at_rest(CRANK_PHASES[i]));

        Self {
            angle: 0.0,
            fourstroke_angle: 0.0,
            omega: 0.0,
            run_state: RunState::Off,
            starter_active: false,
            throttle: 0.0,
            time_scale: 1.0,
            fuel: FUELS[fuel_idx],
            fuel_idx,
            cylinders,
            intake: make_intake_manifold(),
            exhaust: make_exhaust_manifold(),

            torque_smoothed: 0.0,
            power_smoothed: 0.0,
            map_smoothed: P_ATM,
            afr_smoothed: 0.0,
            exhaust_pressure_smoothed: P_ATM,
            exhaust_temp_smoothed: 600.0,
            last_frame_fuel_kg: 0.0,
            last_frame_work_j: 0.0,
        }
    }

    #[inline]
    pub fn rpm(&self) -> f32 { self.omega.abs() * 60.0 / TAU }

    /// Switch fuel index, returning the new fuel name.
    pub fn select_fuel(&mut self, idx: usize) {
        let idx = idx.min(FUELS.len() - 1);
        self.fuel = FUELS[idx];
        self.fuel_idx = idx;
    }
}
