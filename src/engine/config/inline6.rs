use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 3.0L Inline-6 (like a Toyota 2JZ / BMW B58) ─────────────────────────
    EngineConfig {
        name: "3.0L Inline-6",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 6,
        bore: 0.086,
        stroke: 0.086,
        rod_length: 0.142,
        compression_ratio: 9.0,
        // 120-degree mirror-symmetrical crank
        crank_phases: vec![0.0, 2.0 * PI / 3.0, 4.0 * PI / 3.0, 4.0 * PI / 3.0, 2.0 * PI / 3.0, 0.0],
        firing_offsets_deg: vec![0.0, 600.0, 120.0, 480.0, 240.0, 360.0],

        flywheel_inertia: 0.26,
        clutch_max_torque: 550.0,
        clutch_thermal_mass: 2000.0,
        clutch_cooling_coeff: 1.0,
        drivetrain_inertia: 0.40,
        friction_base: 16.0,
        friction_viscous: 0.055,
        friction_windage: 0.00016,

        starter_torque: 300.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 7500.0,
        stall_rpm: 250.0,

        throttle_area_max: 0.0020,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.013,
        intake_volume: 0.0030,
        exhaust_volume: 0.0025,
        tailpipe_area: 0.0014,

        intake_open_deg: 352.0,
        intake_close_deg: 585.0,
        exhaust_open_deg: 135.0,
        exhaust_close_deg: 368.0,
        intake_peak_lift: 0.011,
        exhaust_peak_lift: 0.011,
        intake_valve_diameter: 0.035,
        exhaust_valve_diameter: 0.031,
        intake_runner_length: 0.32,
        intake_runner_area: 1.40e-3,
        cylinder_spacing: 0.10,
        materials: MaterialsConfig::default_for_bore(0.086),
        turbo: crate::engine::turbo::TurboConfig::default(),
    }
}
