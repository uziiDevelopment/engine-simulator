use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 2.0L Inline-4 (like a Honda K20 / Toyota 3S-GE) ─────────────────────
    EngineConfig {
        name: "2.0L Inline-4",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 4,
        bore: 0.086,
        stroke: 0.086,
        rod_length: 0.145,
        compression_ratio: 10.5,
        crank_phases: vec![0.0, PI, PI, 0.0],
        firing_offsets_deg: vec![0.0, 540.0, 180.0, 360.0],

        flywheel_inertia: 0.18,
        clutch_max_torque: 350.0,
        clutch_thermal_mass: 1200.0,
        clutch_cooling_coeff: 0.5,
        drivetrain_inertia: 0.25,
        friction_base: 12.0,
        friction_viscous: 0.045,
        friction_windage: 0.00012,

        starter_torque: 250.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 8000.0,
        stall_rpm: 220.0,

        throttle_area_max: 0.0014,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.015,

        intake_volume: 0.0020,
        exhaust_volume: 0.0015,
        tailpipe_area: 0.0010,

        intake_open_deg: 354.0,
        intake_close_deg: 580.0,
        exhaust_open_deg: 140.0,
        exhaust_close_deg: 366.0,
        intake_peak_lift: 0.010,
        exhaust_peak_lift: 0.010,
        intake_valve_diameter: 0.034,
        exhaust_valve_diameter: 0.030,

        intake_runner_length: 0.34,
        intake_runner_area: 1.30e-3,

        cylinder_spacing: 0.10,
        materials: MaterialsConfig::default_for_bore(0.086),
        turbos: vec![
            crate::engine::turbo::TurboConfig::for_displacement(
                std::f32::consts::PI * 0.086 * 0.086 * 0.25 * 0.086 * 4.0,
            ),
        ],
    }
}
