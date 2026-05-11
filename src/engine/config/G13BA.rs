use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset_suzuki_swift_1_3() -> EngineConfig {
    EngineConfig {
        name: "Suzuki G13BA 1.3L Inline-4",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 4,

        bore: 0.074,
        stroke: 0.0755,
        rod_length: 0.120,
        compression_ratio: 9.5,

        crank_phases: vec![0.0, PI, PI, 0.0],
        firing_offsets_deg: vec![0.0, 540.0, 180.0, 360.0],

        flywheel_inertia: 0.12,
        clutch_max_torque: 180.0,
        drivetrain_inertia: 0.15,

        friction_base: 9.4,
        friction_viscous: 0.033,
        friction_windage: 0.00009,

        clutch_thermal_mass: 1800.0,
        clutch_cooling_coeff: 0.8,

        starter_torque: 150.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 6500.0,
        stall_rpm: 400.0,

        throttle_area_max: 0.00084,
        idle_bleed_frac: 0.015,
        idle_throttle_min: 0.015,

        intake_volume: 0.0014,
        exhaust_volume: 0.0011,
        tailpipe_area: 0.0007,

        intake_open_deg: 360.0,
        intake_close_deg: 590.0,
        exhaust_open_deg: 148.0,
        exhaust_close_deg: 366.0,

        intake_peak_lift: 0.0070,
        exhaust_peak_lift: 0.0066,

        intake_valve_diameter: 0.036,
        exhaust_valve_diameter: 0.030,

        intake_runner_length: 0.24,
        intake_runner_area: 0.80e-3,

        cylinder_spacing: 0.084,
        materials: MaterialsConfig::default_for_bore(0.074),
        turbos: vec![
            crate::engine::turbo::TurboConfig::for_displacement(
                std::f32::consts::PI * 0.074 * 0.074 * 0.25 * 0.0755 * 4.0,
            ),
        ],
    }
}