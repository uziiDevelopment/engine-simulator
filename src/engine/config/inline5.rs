use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 2.5L Inline-5 (like an Audi 2.5 TFSI) ───────────────────────────────
    EngineConfig {
        name: "2.5L Inline-5",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 5,
        bore: 0.0825,
        stroke: 0.0928,
        rod_length: 0.144,
        compression_ratio: 10.0,
        // 144-degree evenly spaced crank throws
        crank_phases: vec![0.0, 0.8 * PI, 1.2 * PI, 1.6 * PI, 0.4 * PI],
        firing_offsets_deg: vec![0.0, 576.0, 144.0, 432.0, 288.0],

        flywheel_inertia: 0.22,
        clutch_max_torque: 480.0,
        drivetrain_inertia: 0.35,
        friction_base: 14.5,
        friction_viscous: 0.052,
        friction_windage: 0.00014,

        starter_torque: 280.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 7200.0,
        stall_rpm: 240.0,

        throttle_area_max: 0.0018,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.015,
        intake_volume: 0.0025,
        exhaust_volume: 0.0020,
        tailpipe_area: 0.0012,

        intake_open_deg: 354.0,
        intake_close_deg: 580.0,
        exhaust_open_deg: 140.0,
        exhaust_close_deg: 366.0,
        intake_peak_lift: 0.011,
        exhaust_peak_lift: 0.011,
        intake_valve_diameter: 0.034,
        exhaust_valve_diameter: 0.030,
        cylinder_spacing: 0.10,
        materials: MaterialsConfig::default_for_bore(0.0825),
    }
}
