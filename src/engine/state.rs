//! The [`EngineCore`] resource — the assembled simulation state.
//!
//! Cylinders, manifolds, crankshaft, fuel, throttle, time-scale, and smoothed
//! telemetry all live here.  Mutated only by [`crate::engine::engine_step`];
//! everything else (visuals, UI) reads.

use bevy::prelude::*;
use std::f32::consts::TAU;

use super::config::{EngineConfig, EngineLayout, ENGINES};
use super::cylinder::CylinderState;
use super::fuel::{Fuel, FUELS};
use super::manifold::Manifold;
use super::oil::{OilConfig, OilState};
use super::thermo::{P_ATM, R_AIR, T_ATM, T_EXH_AMBIENT};
use super::bearing::{BearingState, main_bearing_count};

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
    // ── Active engine configuration ──────────────────────────────────────────
    pub config: EngineConfig,
    pub config_idx: usize,
    /// Incremented every time the config changes (used by visuals for rebuild).
    pub config_generation: u64,

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
    pub audio_enabled:  bool,
    pub particles_enabled: bool,
    /// When true, parts are rendered with the FEA-style heat/damage colormap
    /// (blue → cyan → green → yellow → orange → red) instead of their PBR
    /// material colours.
    pub damage_view: bool,

    // Sub-models (Vec allows variable cylinder counts)
    pub cylinders: Vec<CylinderState>,
    pub intake:    Manifold,
    pub exhaust:   Manifold,

    // ── Lubrication ─────────────────────────────────────────────────────────
    pub oil_config: OilConfig,
    pub oil:        OilState,
    /// Player-facing flag — becomes true as soon as any cylinder reaches a
    /// catastrophic-failure threshold (rod snap, wall worn out, melted piston,
    /// oil starvation).  Engine refuses to spin while set.
    pub engine_seized: bool,

    /// Multiplier on the Archard wear constant so destruction is observable
    /// inside a play session.  1.0 = lab-realistic (basically invisible);
    /// 1e6 = a couple minutes of abuse takes parts to red.
    pub wear_time_scale: f32,

    // ── Journal bearings ────────────────────────────────────────────────────
    /// Main (crankshaft) bearings — one per journal position.
    pub main_bearings: Vec<BearingState>,
    /// Rod (big-end) bearings — one per cylinder.
    pub rod_bearings: Vec<BearingState>,
    /// Cam bearings (aggregated into a single representative bearing).
    pub cam_bearings: Vec<BearingState>,

    // Smoothed telemetry (for the UI; updated each frame)
    pub torque_smoothed:           f32, // Nm
    pub power_smoothed:            f32, // W
    pub map_smoothed:              f32, // Pa
    pub afr_smoothed:              f32,
    pub exhaust_pressure_smoothed: f32, // Pa
    pub exhaust_temp_smoothed:     f32, // K
    pub last_frame_fuel_kg:        f32,
    pub last_frame_work_j:         f32,
    /// Smoothed friction-heat going into the oil (W) — for the gauge.
    pub friction_heat_smoothed:    f32,
}

impl EngineCore {
    pub fn new(engine_idx: usize, fuel_idx: usize) -> Self {
        let engine_idx = engine_idx.min(ENGINES.len() - 1);
        let config = ENGINES[engine_idx].clone();
        let num_cyl = config.num_cylinders;

        let cylinders: Vec<CylinderState> = (0..num_cyl)
            .map(|i| CylinderState::at_rest_cfg(&config, i))
            .collect();

        let intake = Manifold {
            volume: config.intake_volume,
            mass: P_ATM * config.intake_volume / (R_AIR * T_ATM),
            temperature: T_ATM,
            flow_signal: 0.0,
            label: "intake",
        };
        let exhaust = Manifold {
            volume: config.exhaust_volume,
            mass: P_ATM * config.exhaust_volume / (R_AIR * T_EXH_AMBIENT),
            temperature: T_EXH_AMBIENT,
            flow_signal: 0.0,
            label: "exhaust",
        };

        let oil_config = OilConfig::default();
        let oil = OilState::fresh(&oil_config);

        let is_inline = config.layout == EngineLayout::Inline;
        let n_main = main_bearing_count(num_cyl, is_inline);
        let main_bearings: Vec<BearingState> = (0..n_main).map(|_| BearingState::fresh()).collect();
        let rod_bearings: Vec<BearingState> = (0..num_cyl).map(|_| BearingState::fresh()).collect();
        let cam_bearings: Vec<BearingState> = vec![BearingState::fresh()];

        Self {
            config,
            config_idx: engine_idx,
            config_generation: 0,
            angle: 0.0,
            fourstroke_angle: 0.0,
            omega: 0.0,
            run_state: RunState::Off,
            starter_active: false,
            throttle: 0.0,
            time_scale: 1.0,
            fuel: FUELS[fuel_idx.min(FUELS.len() - 1)],
            fuel_idx,
            audio_enabled: true,
            particles_enabled: true,
            damage_view: false,
            cylinders,
            intake,
            exhaust,

            oil_config,
            oil,
            engine_seized: false,
            wear_time_scale: 1_000.0,

            main_bearings,
            rod_bearings,
            cam_bearings,

            torque_smoothed: 0.0,
            power_smoothed: 0.0,
            map_smoothed: P_ATM,
            afr_smoothed: 0.0,
            exhaust_pressure_smoothed: P_ATM,
            exhaust_temp_smoothed: 600.0,
            last_frame_fuel_kg: 0.0,
            last_frame_work_j: 0.0,
            friction_heat_smoothed: 0.0,
        }
    }

    #[inline]
    pub fn rpm(&self) -> f32 { self.omega.abs() * 60.0 / TAU }

    #[inline]
    pub fn num_cyl(&self) -> usize { self.config.num_cylinders }

    /// Switch fuel index.
    pub fn select_fuel(&mut self, idx: usize) {
        let idx = idx.min(FUELS.len() - 1);
        self.fuel = FUELS[idx];
        self.fuel_idx = idx;
    }

    /// Switch engine configuration by preset index — resets all simulation state.
    pub fn select_engine(&mut self, idx: usize) {
        let idx = idx.min(ENGINES.len() - 1);
        if idx == self.config_idx { return; }
        let config = ENGINES[idx].clone();
        self.apply_config(config, idx);
    }

    /// Load an arbitrary EngineConfig (e.g. dynamically built).  Resets simulation.
    pub fn set_config(&mut self, config: EngineConfig) {
        self.apply_config(config, usize::MAX);
    }

    fn apply_config(&mut self, config: EngineConfig, idx: usize) {
        let num_cyl = config.num_cylinders;

        self.cylinders = (0..num_cyl)
            .map(|i| CylinderState::at_rest_cfg(&config, i))
            .collect();

        self.intake = Manifold {
            volume: config.intake_volume,
            mass: P_ATM * config.intake_volume / (R_AIR * T_ATM),
            temperature: T_ATM,
            flow_signal: 0.0,
            label: "intake",
        };
        self.exhaust = Manifold {
            volume: config.exhaust_volume,
            mass: P_ATM * config.exhaust_volume / (R_AIR * T_EXH_AMBIENT),
            temperature: T_EXH_AMBIENT,
            flow_signal: 0.0,
            label: "exhaust",
        };

        self.config = config;
        self.config_idx = idx;
        self.config_generation += 1;
        self.angle = 0.0;
        self.fourstroke_angle = 0.0;
        self.omega = 0.0;
        self.run_state = RunState::Off;
        self.starter_active = false;
        self.engine_seized = false;
        self.oil_config = OilConfig::default();
        self.oil = OilState::fresh(&self.oil_config);

        let is_inline = self.config.layout == EngineLayout::Inline;
        let n_main = main_bearing_count(num_cyl, is_inline);
        self.main_bearings = (0..n_main).map(|_| BearingState::fresh()).collect();
        self.rod_bearings = (0..num_cyl).map(|_| BearingState::fresh()).collect();
        self.cam_bearings = vec![BearingState::fresh()];

        self.torque_smoothed = 0.0;
        self.power_smoothed = 0.0;
        self.map_smoothed = P_ATM;
        self.afr_smoothed = 0.0;
        self.exhaust_pressure_smoothed = P_ATM;
        self.exhaust_temp_smoothed = 600.0;
        self.last_frame_fuel_kg = 0.0;
        self.last_frame_work_j = 0.0;
        self.friction_heat_smoothed = 0.0;
    }
}
