use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 6.5L V12 (like a Ferrari 812 / Lamborghini Aventador) ───────────────
    EngineConfig {
        name: "6.5L V12",
        layout: EngineLayout::V,
        bank_angle: PI / 3.0, // 60-degree V
        num_cylinders: 12,
        bore: 0.095,
        stroke: 0.0764,
        rod_length: 0.138,
        compression_ratio: 11.8,
        crank_phases: vec![
            0.0, 0.0,
            2.0 * PI / 3.0, 2.0 * PI / 3.0,
            4.0 * PI / 3.0, 4.0 * PI / 3.0,
            4.0 * PI / 3.0, 4.0 * PI / 3.0,
            2.0 * PI / 3.0, 2.0 * PI / 3.0,
            0.0, 0.0
        ],
        firing_offsets_deg: vec![30.0, 330.0, 630.0, 210.0, 510.0, 450.0, 150.0, 90.0, 270.0, 570.0, 390.0, 690.0],

        flywheel_inertia: 0.32,
        clutch_max_torque: 850.0,
        clutch_thermal_mass: 3000.0,
        clutch_cooling_coeff: 1.5,
        drivetrain_inertia: 0.55,
        friction_base: 28.0,
        friction_viscous: 0.075,
        friction_windage: 0.00022,

        starter_torque: 400.0,
        starter_disengage_rpm: 650.0,
        redline_rpm: 8500.0,
        stall_rpm: 300.0,

        throttle_area_max: 0.0032,
        idle_bleed_frac: 0.009,
        idle_throttle_min: 0.011,
        intake_volume: 0.0065,
        exhaust_volume: 0.0050,
        tailpipe_area: 0.0022,

        intake_open_deg: 345.0,
        intake_close_deg: 595.0,
        exhaust_open_deg: 125.0,
        exhaust_close_deg: 375.0,
        intake_peak_lift: 0.012,
        exhaust_peak_lift: 0.012,
        intake_valve_diameter: 0.038,
        exhaust_valve_diameter: 0.032,
        intake_runner_length: 0.24,
        intake_runner_area: 1.65e-3,
        cylinder_spacing: 0.11,
        materials: MaterialsConfig::default_for_bore(0.095),
        turbo: crate::engine::turbo::TurboConfig::default(),
    }
}
