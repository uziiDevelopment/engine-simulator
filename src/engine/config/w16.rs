use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 8.0L W16 (like a Bugatti Chiron) — proper W layout ──────────────────
    // Two narrow-angle VR sub-clusters (banks A+B and C+D) at 90° to each other.
    // narrow_angle = 15° between the two banks within each VR cluster.
    EngineConfig {
        name: "8.0L W16",
        layout: EngineLayout::W { narrow_angle: 15.0_f32.to_radians() },
        bank_angle: PI / 2.0, // 90° between the two VR clusters
        num_cylinders: 16,
        bore: 0.086,
        stroke: 0.086, // 16 x 500cc = 8.0L exactly
        rod_length: 0.145,
        compression_ratio: 9.0,
        // 4 banks (A,B,C,D); 4 axial positions.
        // A+B share each throw; C+D share a throw 45° (PI/4) later.
        crank_phases: vec![
            0.0,        0.0,        PI / 4.0,        PI / 4.0,        // group 0
            PI / 2.0,   PI / 2.0,   3.0 * PI / 4.0,  3.0 * PI / 4.0, // group 1
            PI,         PI,         5.0 * PI / 4.0,  5.0 * PI / 4.0, // group 2
            3.0 * PI / 2.0, 3.0 * PI / 2.0, 7.0 * PI / 4.0, 7.0 * PI / 4.0, // group 3
        ],
        // Banks A,C fire rev 1; banks B,D fire rev 2 (360° offset) — even 45° pulses
        firing_offsets_deg: vec![
            0.0, 360.0, 45.0, 405.0,
            90.0, 450.0, 135.0, 495.0,
            180.0, 540.0, 225.0, 585.0,
            270.0, 630.0, 315.0, 675.0,
        ],

        flywheel_inertia: 0.65,
        friction_base: 40.0,
        friction_viscous: 0.090,
        friction_windage: 0.00030,

        starter_torque: 550.0,
        starter_disengage_rpm: 550.0,
        redline_rpm: 6800.0,
        stall_rpm: 350.0,

        throttle_area_max: 0.0040,
        idle_bleed_frac: 0.008,
        idle_throttle_min: 0.010,
        intake_volume: 0.0080,
        exhaust_volume: 0.0065,
        tailpipe_area: 0.0030,

        intake_open_deg: 350.0,
        intake_close_deg: 590.0,
        exhaust_open_deg: 130.0,
        exhaust_close_deg: 370.0,
        intake_peak_lift: 0.011,
        exhaust_peak_lift: 0.011,
        intake_valve_diameter: 0.035,
        exhaust_valve_diameter: 0.030,
        cylinder_spacing: 0.11,
        materials: MaterialsConfig::default_for_bore(0.086),
    }
}
