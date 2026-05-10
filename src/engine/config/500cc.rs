use super::{EngineConfig, EngineLayout, MaterialsConfig};

pub fn preset() -> EngineConfig {
    // ── 500cc Single-Cylinder ───────────────────────────────────────────────
    EngineConfig {
        name: "500cc Single",
        layout: EngineLayout::Inline, // Treated as a 1-cylinder inline
        bank_angle: 0.0,
        num_cylinders: 1,
        bore: 0.086,
        stroke: 0.086,
        rod_length: 0.145,
        compression_ratio: 10.5,
        
        // Only one cylinder, so only one phase and one firing offset
        crank_phases: vec![0.0],
        firing_offsets_deg: vec![0.0],

        // Scaled down, but proportionally heavier to keep a 1-cyl from stalling
        flywheel_inertia: 0.08, 
        clutch_max_torque: 100.0,
        clutch_thermal_mass: 400.0,
        clutch_cooling_coeff: 0.2,
        drivetrain_inertia: 0.12,
        
        // Frictions scaled down roughly to 1/4th of the 4-cylinder
        friction_base: 3.0,
        friction_viscous: 0.011,
        friction_windage: 0.00003,

        starter_torque: 80.0, // Needs less torque to crank 1 cylinder
        starter_disengage_rpm: 600.0,
        redline_rpm: 8000.0,
        stall_rpm: 300.0, // Single cylinders generally need a slightly higher idle/stall RPM

        // Breathing capacities scaled down for 500cc
        throttle_area_max: 0.0005,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.015,

        intake_volume: 0.0005,
        exhaust_volume: 0.0004,
        tailpipe_area: 0.0003,

        // Cam timings and valve profiles remain identical
        intake_open_deg: 354.0,
        intake_close_deg: 580.0,
        exhaust_open_deg: 140.0,
        exhaust_close_deg: 366.0,
        intake_peak_lift: 0.010,
        exhaust_peak_lift: 0.010,
        intake_valve_diameter: 0.034,
        exhaust_valve_diameter: 0.030,

        cylinder_spacing: 0.10, // Doesn't matter for 1 cylinder, but kept for struct completion
        materials: MaterialsConfig::default_for_bore(0.086),
    }
}