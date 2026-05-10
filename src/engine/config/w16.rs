use super::{EngineConfig, EngineLayout, MaterialsConfig};
use std::f32::consts::PI;

pub fn preset() -> EngineConfig {
    // ── 8.0L W16 (Bugatti Chiron-style) — proper W layout ───────────────────
    //
    // Architecture:
    //   Two narrow-angle VR8 sub-blocks joined at 90°.
    //   Within each VR sub-block, the two banks are offset by 15° (narrow_angle).
    //   This gives 4 distinct banks (A, B, C, D) and 8 crankpin positions.
    //
    // Cylinder indexing (i % 4):
    //   0 → Bank A  (left VR, outer)
    //   1 → Bank B  (left VR, inner)
    //   2 → Bank C  (right VR, inner)
    //   3 → Bank D  (right VR, outer)
    //
    // Firing order:
    //   16 power strokes in 720° → one exactly every 45°.
    //   The firing offsets produce even 45° intervals across the full 720° cycle.

    // FIX: Widen the narrow angle to prevent lateral VR clipping.
    // Real W16s use 15°, but rely on "deck offset" to prevent overlapping. 
    // Since this sim uses pure radial geometry, 15° at an 86mm bore will 
    // always intersect. Increasing this to 40° provides the visual clearance.
    let na = 40.0_f32.to_radians(); 
    let split = na / 2.0;           // Dynamically splits the pins for perfect timing

    // Base crank throws for the 4 axial groups.
    // Offset by 45° across the 8 crank pins for even firing on a 90° W-engine.
    let base_l0 = 45.0_f32.to_radians();
    let base_r0 = 0.0_f32.to_radians();
    
    let base_l1 = 315.0_f32.to_radians();
    let base_r1 = 270.0_f32.to_radians();
    
    let base_l2 = 135.0_f32.to_radians();
    let base_r2 = 90.0_f32.to_radians();
    
    let base_l3 = 225.0_f32.to_radians();
    let base_r3 = 180.0_f32.to_radians();

    EngineConfig {
        name: "8.0L W16",
        layout: EngineLayout::W { narrow_angle: na },
        bank_angle: PI / 2.0, // 90° between the two VR clusters
        num_cylinders: 16,
        bore: 0.086,
        stroke: 0.086, // 16 × 500cc = 8.0L
        rod_length: 0.180, // Increased to push pistons up into the wider part of the V
        compression_ratio: 9.0,

        // Crank phases (radians) — 4 axial groups, properly split-pinned.
        crank_phases: vec![
            // Group 0 (axial pos 0): A, B, C, D
            base_l0 + split, base_l0 - split, base_r0 + split, base_r0 - split,
            // Group 1 (axial pos 1): A, B, C, D
            base_l1 + split, base_l1 - split, base_r1 + split, base_r1 - split,
            // Group 2 (axial pos 2): A, B, C, D
            base_l2 + split, base_l2 - split, base_r2 + split, base_r2 - split,
            // Group 3 (axial pos 3): A, B, C, D
            base_l3 + split, base_l3 - split, base_r3 + split, base_r3 - split,
        ],

        // Even-fire 45° intervals across 720°.
        // The split-pin geometry above clusters the TDCs perfectly into 8 angles, 2 cylinders each.
        // We stagger the pairs by 360° to yield 16 flawless sequential 45° intervals.
        firing_offsets_deg: vec![
            // Grp 0: A0, B0, C0, D0
              0.0, 360.0, 315.0, 675.0,
            // Grp 1: A1, B1, C1, D1
             90.0, 450.0,  45.0, 405.0,
            // Grp 2: A2, B2, C2, D2
            270.0, 630.0, 225.0, 585.0,
            // Grp 3: A3, B3, C3, D3
            180.0, 540.0, 135.0, 495.0,
        ],

        // ── Dynamics — scaled for a 16-cylinder, 8.0L quad-turbo monster ─────
        flywheel_inertia: 0.55,     // lighter than before — Bugatti uses a dual-mass
        friction_base: 30.0,        // was 40 — too high, 16 cyl but modern tolerances
        friction_viscous: 0.065,    // was 0.090 — way too viscous
        friction_windage: 0.00022,  // was 0.00030

        starter_torque: 600.0,     // big engine needs a big starter
        starter_disengage_rpm: 500.0,
        redline_rpm: 6800.0,
        stall_rpm: 350.0,

        // ── Airflow — the key to making power ────────────────────────────────
        // 8.0L needs MUCH more air than a 5.0L V8.
        throttle_area_max: 0.0060,  
        idle_bleed_frac: 0.012,     
        idle_throttle_min: 0.018,   
        intake_volume: 0.0120,      
        exhaust_volume: 0.0100,     
        tailpipe_area: 0.0045,      // quad exhaust tips

        // ── Valve timing — aggressive for a performance W16 ──────────────────
        intake_open_deg: 340.0,     // open earlier for better filling
        intake_close_deg: 600.0,    // hold open longer (Miller cycle effect)
        exhaust_open_deg: 120.0,    // blow down earlier
        exhaust_close_deg: 375.0,   // more overlap
        intake_peak_lift: 0.012,    
        exhaust_peak_lift: 0.012,   
        intake_valve_diameter: 0.037,  // slightly bigger valves
        exhaust_valve_diameter: 0.032, 
        
        // Increased cylinder spacing to prevent visual axial clipping
        cylinder_spacing: 0.135,    
        materials: MaterialsConfig::default_for_bore(0.086),
    }
}