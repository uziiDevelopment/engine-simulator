use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 5.0L V8 (like a Ford Coyote / Chevy LS) — 90° cross-plane ──────────
    EngineConfig {
        name: "5.0L V8 (Cross-plane)",
        layout: EngineLayout::V,
        bank_angle: PI / 2.0,
        num_cylinders: 8,
        bore: 0.092,
        stroke: 0.093,
        rod_length: 0.152,
        compression_ratio: 11.0,
        crank_phases: vec![0.0, 0.0, PI * 0.5, PI * 0.5, PI * 1.5, PI * 1.5, PI, PI],
        firing_offsets_deg: vec![45.0, 315.0, 675.0, 225.0, 135.0, 405.0, 585.0, 495.0],

        flywheel_inertia: 0.35,
        clutch_max_torque: 600.0,
        clutch_thermal_mass: 2500.0,
        clutch_cooling_coeff: 1.2,
        drivetrain_inertia: 0.40,
        friction_base: 22.0,
        friction_viscous: 0.065,
        friction_windage: 0.00018,

        starter_torque: 250.0,
        starter_disengage_rpm: 500.0,
        redline_rpm: 7000.0,
        stall_rpm: 280.0,

        throttle_area_max: 0.0024,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.012,

        intake_volume: 0.0050,
        exhaust_volume: 0.0040,
        tailpipe_area: 0.0018,

        intake_open_deg: 350.0,
        intake_close_deg: 590.0,
        exhaust_open_deg: 130.0,
        exhaust_close_deg: 370.0,
        intake_peak_lift: 0.012,
        exhaust_peak_lift: 0.012,
        intake_valve_diameter: 0.037,
        exhaust_valve_diameter: 0.032,

        intake_runner_length: 0.28,
        intake_runner_area: 1.55e-3,

        cylinder_spacing: 0.11,
        materials: MaterialsConfig::default_for_bore(0.092),
        turbo: crate::engine::turbo::TurboConfig::for_displacement(
            std::f32::consts::PI * 0.092 * 0.092 * 0.25 * 0.093 * 8.0,
        ),
    }
}
