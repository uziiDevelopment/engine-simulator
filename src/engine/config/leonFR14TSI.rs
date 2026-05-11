use super::{EngineConfig, EngineLayout, MaterialsConfig, TurboConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // SEAT Leon FR 1.4 TSI ACT 150 PS
    // 1395 cc, 74.5 x 80 mm, 150 hp @ 5000-6000 rpm, 250 Nm @ 1500-3500 rpm
    EngineConfig {
        name: "1.4L Inline-4 Turbo (SEAT Leon FR 1.4 TSI ACT 150)",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 4,

        bore: 0.0745,
        stroke: 0.0800,
        rod_length: 0.138,

        compression_ratio: 10.0,

        // Standard inline-4 firing/cycle spacing approximation
        crank_phases: vec![0.0, PI, 0.0, PI],
        firing_offsets_deg: vec![0.0, 180.0, 360.0, 540.0],

        flywheel_inertia: 0.095,
        clutch_max_torque: 320.0,
        clutch_thermal_mass: 1350.0,
        clutch_cooling_coeff: 0.85,
        drivetrain_inertia: 0.18,
        friction_base: 10.8,
        friction_viscous: 0.033,
        friction_windage: 0.00009,

        starter_torque: 170.0,
        starter_disengage_rpm: 500.0,
        redline_rpm: 6500.0,
        stall_rpm: 300.0,

        throttle_area_max: 0.00135,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.010,

        intake_volume: 0.00235,
        exhaust_volume: 0.00185,
        tailpipe_area: 0.00105,

        // Turbo 1.4 TSI-style character
        intake_open_deg: 350.0,
        intake_close_deg: 566.0,
        exhaust_open_deg: 138.0,
        exhaust_close_deg: 360.0,
        intake_peak_lift: 0.0095,
        exhaust_peak_lift: 0.0088,
        intake_valve_diameter: 0.0335,
        exhaust_valve_diameter: 0.0295,

        intake_runner_length: 0.31,
        intake_runner_area: 1.05e-3,

        cylinder_spacing: 0.086,
        materials: MaterialsConfig::default_for_bore(0.0745),

        turbos: vec![
            TurboConfig {
                enabled: true,
                target_boost_pa: 1.03e5,
                turbine_efficiency: 0.74,
                compressor_efficiency: 0.76,
                wastegate_area: 0.00050,
                intercooler_effectiveness: 0.82,
                bov_threshold_pa: 0.22e5,
                blade_count: 11,
                ..Default::default()
            }
            .scaled_for_displacement(
                std::f32::consts::PI * 0.0745 * 0.0745 * 0.25 * 0.0800 * 4.0,
            ),
        ],
    }
}