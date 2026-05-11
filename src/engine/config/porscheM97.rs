use super::{EngineConfig, EngineLayout, MaterialsConfig, TurboConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // Porsche Cayman S 987.1 M97.21 3.4L flat-6, tuned closer to factory character
    EngineConfig {
        name: "3.4L Flat-6 (Porsche Cayman S M97.21 Tuned)",
        layout: EngineLayout::Flat,
        bank_angle: PI,
        num_cylinders: 6,

        // Factory bore and stroke
        bore: 0.096,
        stroke: 0.078,

        // Approximate OEM rod length
        rod_length: 0.145,

        // Slightly softened to reduce the unrealistically strong low-mid torque
        compression_ratio: 11.0,

        crank_phases: vec![
            0.0,
            PI,
            2.0 * PI / 3.0,
            5.0 * PI / 3.0,
            4.0 * PI / 3.0,
            PI / 3.0,
        ],
        firing_offsets_deg: vec![90.0, 450.0, 330.0, 690.0, 210.0, 570.0],

        flywheel_inertia: 0.11,
        clutch_max_torque: 360.0,
        clutch_thermal_mass: 1750.0,
        clutch_cooling_coeff: 0.8,
        drivetrain_inertia: 0.26,

        // Slightly reduced friction so the top end does not fall off too hard
        friction_base: 14.2,
        friction_viscous: 0.044,
        friction_windage: 0.00012,

        starter_torque: 220.0,
        starter_disengage_rpm: 550.0,
        redline_rpm: 7300.0,
        stall_rpm: 250.0,

        throttle_area_max: 0.00195,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.012,

        // Slightly improved breathing
        intake_volume: 0.00315,
        exhaust_volume: 0.00245,
        tailpipe_area: 0.00135,

        // Shift the usable range a bit higher and reduce the too-strong low rpm fill
        intake_open_deg: 354.0,
        intake_close_deg: 586.0,
        exhaust_open_deg: 142.0,
        exhaust_close_deg: 364.0,

        intake_peak_lift: 0.0115,
        exhaust_peak_lift: 0.0105,
        intake_valve_diameter: 0.0365,
        exhaust_valve_diameter: 0.0315,

        // Shorter, freer breathing for more top-end pull
        intake_runner_length: 0.24,
        intake_runner_area: 1.55e-3,

        cylinder_spacing: 0.118,
        materials: MaterialsConfig::default_for_bore(0.096),

        // Bespoke turbo: moderate boost for street drivability
        turbos: vec![
            TurboConfig {
                enabled: true,
                target_boost_pa: 0.65e5,   // 0.65 bar boost — moderate street tune
                turbine_efficiency: 0.72,
                compressor_efficiency: 0.74,
                intercooler_effectiveness: 0.70,
                bov_threshold_pa: 0.25e5,
                blade_count: 9,            // Distinctive turbo whine character
                ..Default::default()
            }.scaled_for_displacement(
                std::f32::consts::PI * 0.096 * 0.096 * 0.25 * 0.078 * 6.0,
            ),
        ],
    }
}