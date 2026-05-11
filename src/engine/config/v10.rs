use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 5.2L V10 (like an Audi R8 / Lamborghini Huracan) ────────────────────
    EngineConfig {
        name: "5.2L V10",
        layout: EngineLayout::V,
        bank_angle: PI * 0.4, // 72-degree V for perfect primary balance and even firing
        num_cylinders: 10,
        bore: 0.0845,
        stroke: 0.0928,
        rod_length: 0.150,
        compression_ratio: 12.5,
        crank_phases: vec![
            0.0, 0.0,
            0.8 * PI, 0.8 * PI,
            1.6 * PI, 1.6 * PI,
            0.4 * PI, 0.4 * PI,
            1.2 * PI, 1.2 * PI
        ],
        firing_offsets_deg: vec![36.0, 324.0, 612.0, 180.0, 468.0, 396.0, 684.0, 252.0, 540.0, 108.0],

        flywheel_inertia: 0.28,
        clutch_max_torque: 750.0,
        clutch_thermal_mass: 2800.0,
        clutch_cooling_coeff: 1.4,
        drivetrain_inertia: 0.45,
        friction_base: 25.0,
        friction_viscous: 0.070,
        friction_windage: 0.00020,

        starter_torque: 350.0,
        starter_disengage_rpm: 650.0,
        redline_rpm: 8500.0, // V10 screams!
        stall_rpm: 300.0,

        throttle_area_max: 0.0026,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.012,
        intake_volume: 0.0050,
        exhaust_volume: 0.0040,
        tailpipe_area: 0.0020,

        intake_open_deg: 345.0,
        intake_close_deg: 595.0,
        exhaust_open_deg: 125.0,
        exhaust_close_deg: 375.0,
        intake_peak_lift: 0.012,
        exhaust_peak_lift: 0.012,
        intake_valve_diameter: 0.036,
        exhaust_valve_diameter: 0.031,
        intake_runner_length: 0.25,
        intake_runner_area: 1.45e-3,
        cylinder_spacing: 0.11,
        materials: MaterialsConfig::default_for_bore(0.0845),
        turbo: crate::engine::turbo::TurboConfig::for_displacement(
            std::f32::consts::PI * 0.0845 * 0.0845 * 0.25 * 0.0928 * 10.0,
        ),
    }
}
