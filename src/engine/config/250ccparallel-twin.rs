use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset_250cc_twin() -> EngineConfig {
    EngineConfig {
        name: "250cc Parallel-Twin Sportbike",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 2,
        bore: 0.062,         // 62mm
        stroke: 0.0412,      // 41.2mm (very short stroke for high RPM)
        rod_length: 0.090,
        compression_ratio: 11.6,
        
        // 180-degree crank typical of small sportbike twins
        crank_phases: vec![0.0, PI],
        firing_offsets_deg: vec![0.0, 180.0], // Uneven firing intervals (0, 180, then 540 gap)

        flywheel_inertia: 0.04, // Very light flywheel for quick revving
        clutch_max_torque: 80.0,
        clutch_thermal_mass: 400.0,
        clutch_cooling_coeff: 0.2,
        drivetrain_inertia: 0.10,
        friction_base: 5.0,
        friction_viscous: 0.02,
        friction_windage: 0.00005,

        starter_torque: 100.0,
        starter_disengage_rpm: 800.0,
        redline_rpm: 13500.0, // Extremely high redline
        stall_rpm: 800.0,     // High stall RPM

        throttle_area_max: 0.0008,
        idle_bleed_frac: 0.015,
        idle_throttle_min: 0.02,

        intake_volume: 0.0006,
        exhaust_volume: 0.0005,
        tailpipe_area: 0.0004,

        // Aggressive cams for high RPM
        intake_open_deg: 345.0,
        intake_close_deg: 590.0,
        exhaust_open_deg: 130.0,
        exhaust_close_deg: 375.0,
        intake_peak_lift: 0.008,
        exhaust_peak_lift: 0.0075,
        intake_valve_diameter: 0.024,
        exhaust_valve_diameter: 0.021,

        cylinder_spacing: 0.075,
        materials: MaterialsConfig::default_for_bore(0.062),
    }
}