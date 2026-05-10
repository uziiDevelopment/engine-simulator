use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 3.8L Flat-6 (like a Porsche 997) ────────────────────────────────────
    EngineConfig {
        name: "3.8L Flat-6",
        layout: EngineLayout::Flat,
        bank_angle: PI,
        num_cylinders: 6,
        bore: 0.102,
        stroke: 0.077,
        rod_length: 0.128,
        compression_ratio: 12.5,
        crank_phases: vec![0.0, PI, 2.0*PI/3.0, 5.0*PI/3.0, 4.0*PI/3.0, PI/3.0],
        firing_offsets_deg: vec![90.0, 450.0, 330.0, 690.0, 210.0, 570.0],

        flywheel_inertia: 0.14,
        clutch_max_torque: 450.0,
        clutch_thermal_mass: 1800.0,
        clutch_cooling_coeff: 0.8,
        drivetrain_inertia: 0.28,
        friction_base: 16.0,
        friction_viscous: 0.050,
        friction_windage: 0.00015,

        starter_torque: 250.0,
        starter_disengage_rpm: 550.0,
        redline_rpm: 8500.0,
        stall_rpm: 250.0,

        throttle_area_max: 0.0020,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.013,

        intake_volume: 0.0035,
        exhaust_volume: 0.0025,
        tailpipe_area: 0.0014,

        intake_open_deg: 348.0,
        intake_close_deg: 576.0,
        exhaust_open_deg: 136.0,
        exhaust_close_deg: 368.0,
        intake_peak_lift: 0.011,
        exhaust_peak_lift: 0.011,
        intake_valve_diameter: 0.036,
        exhaust_valve_diameter: 0.031,

        intake_runner_length: 0.26,
        intake_runner_area: 1.45e-3,

        cylinder_spacing: 0.12,
        materials: MaterialsConfig::default_for_bore(0.102),
    }
}
