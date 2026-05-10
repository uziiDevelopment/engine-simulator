use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset_suzuki_swift_1_3() -> EngineConfig {
    // ── 1996 Suzuki Swift GX 1.3 (Suzuki G13BA 8-Valve SOHC) ───────────────
    EngineConfig {
        name: "Suzuki G13BA 1.3L Inline-4",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 4,
        
        // Exact factory block dimensions for the G13BA
        bore: 0.074,            // 74.0 mm bore
        stroke: 0.0755,         // 75.5 mm stroke (1298cc displacement)
        rod_length: 0.120,      // 120.0 mm connecting rod length
        compression_ratio: 9.5, // Standard 9.5:1 compression for the 8v
        
        // Standard Inline-4 1-3-4-2 firing order
        crank_phases: vec![0.0, PI, PI, 0.0],
        firing_offsets_deg: vec![0.0, 540.0, 180.0, 360.0],

        // The G13 is a very small, lightweight aluminum engine 
        flywheel_inertia: 0.12, 
        clutch_max_torque: 180.0,
        drivetrain_inertia: 0.15,
        friction_base: 8.5,
        friction_viscous: 0.030,
        friction_windage: 0.00008,

        clutch_thermal_mass: 1800.0,
        clutch_cooling_coeff: 0.8,

        starter_torque: 150.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 6500.0,    // Standard redline for the SOHC 8v
        stall_rpm: 400.0,

        // Small throttle body (TBI / Single Point Injection)
        throttle_area_max: 0.0009,
        idle_bleed_frac: 0.015,
        idle_throttle_min: 0.015,

        intake_volume: 0.0014,
        exhaust_volume: 0.0011,
        tailpipe_area: 0.0007,

        // Economy SOHC profile: Mild lift and relatively short duration
        intake_open_deg: 355.0,
        intake_close_deg: 575.0,
        exhaust_open_deg: 145.0,
        exhaust_close_deg: 365.0,
        intake_peak_lift: 0.008,  // 8.0mm lift
        exhaust_peak_lift: 0.0075, // 7.5mm lift
        
        // Factory valve sizes for the 8-valve head
        intake_valve_diameter: 0.036,  // 36.0 mm 
        exhaust_valve_diameter: 0.030, // 30.0 mm

        cylinder_spacing: 0.084, // 84mm bore spacing used on Suzuki G-series blocks
        materials: MaterialsConfig::default_for_bore(0.074),
    }
}