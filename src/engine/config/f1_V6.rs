use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset_f1_v6() -> EngineConfig {
    // ── 1.6L V6 Formula 1 ICE ──────────
    // 90° Bank Angle, 3-throw crankshaft yielding a 90°-150° odd-firing sequence.
    EngineConfig {
        name: "1.6L V6 Turbo Hybrid (F1 Spec)",
        layout: EngineLayout::V,
        bank_angle: PI / 2.0, // Mandated 90 degree V
        num_cylinders: 6,
        bore: 0.080,  // Mandated 80mm maximum bore
        stroke: 0.053, // 53mm stroke to achieve precisely 1598cc
        rod_length: 0.104, // Extremely long rods (~1.96 ratio) to minimize piston side-load
        compression_ratio: 16.0, // Very high CR, enabled by Turbulent Jet Ignition (TJI)

        // 3-Throw Crankshaft: Cylinders share pins (1&6 on 0°, 2&4 on 240°, 3&5 on 120°)
        crank_phases: vec![
            0.0, 
            4.0 * PI / 3.0, 
            2.0 * PI / 3.0, 
            4.0 * PI / 3.0, 
            2.0 * PI / 3.0, 
            0.0
        ],
        // Resulting in an authentic F1 odd-firing order: 45 -> 195 (+150°) -> 285 (+90°) -> 435 (+150°) -> 525 (+90°) -> 675 (+150°) -> 45 (+90°)
        firing_offsets_deg: vec![45.0, 285.0, 525.0, 195.0, 435.0, 675.0],

        flywheel_inertia: 0.06, // Almost nonexistent; basically just the carbon clutch basket
        clutch_max_torque: 250.0,
        clutch_thermal_mass: 300.0,
        clutch_cooling_coeff: 0.8,
        drivetrain_inertia: 0.08,
        friction_base: 14.0, // Tightly sprung rings, but fewer cylinders than the V8
        friction_viscous: 0.015, // Ultra-thin aerospace-grade oil running at high temps
        friction_windage: 0.00012, // High RPM windage, mitigated by aggressive crankcase vacuum scavenging

        starter_torque: 80.0, // Spun up by external pit starter or MGU-K
        starter_disengage_rpm: 3500.0,
        redline_rpm: 15000.0, // FIA Mandated rev limit
        stall_rpm: 3800.0, // High-performance cams prevent idling below ~4000 RPM

        throttle_area_max: 0.0045, // Massive airflow requirement at 15,000 RPM
        idle_bleed_frac: 0.045, 
        idle_throttle_min: 0.050, // Needs significant air just to sustain 4,500 RPM idle

        intake_volume: 0.0080, // Large carbon plenum acting as a buffer for the turbocharger
        exhaust_volume: 0.0030, // Extremely short, optimized Inconel primary headers 
        tailpipe_area: 0.0035, // Single mandated exhaust exit for the turbine

        // Extremely aggressive pneumatic valvetrain (300° duration, massive overlap)
        intake_open_deg: 320.0,  // 40° BTDC
        intake_close_deg: 620.0, // 80° ABDC
        exhaust_open_deg: 100.0, // 80° BBDC
        exhaust_close_deg: 400.0, // 40° ATDC
        
        intake_peak_lift: 0.016, // 16mm of lift (only possible safely with pneumatic valve springs)
        exhaust_peak_lift: 0.015,
        intake_valve_diameter: 0.034, // Squeezed tightly into the 80mm bore
        exhaust_valve_diameter: 0.029,

        cylinder_spacing: 0.095, // Highly compact block
        materials: MaterialsConfig::default_for_bore(0.080),
    }
}