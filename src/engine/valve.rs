//! Cam-driven valve lift profiles and effective discharge area.
//!
//! Within each cylinder's local 4-stroke phase (0..720°), the four strokes are:
//!
//! ```text
//!     0 °  TDC compression  ← spark just before this
//!   180 °  BDC          (end of power stroke)
//!   360 °  TDC exhaust  ← valve overlap window
//!   540 °  BDC intake
//!   720 °  TDC compression (next cycle)
//! ```
//!
//! Standard automotive timing has both valves open briefly across TDC exhaust
//! ("overlap"), which is what produces the slight residual-burned-gas mixing
//! in the cylinder.

use std::f32::consts::PI;

use super::geometry::FIRING_OFFSETS_DEG;

// ── Cam timing (deg, in cylinder-local 4-stroke phase) ──────────────────────
pub const INTAKE_OPEN_DEG:   f32 = 354.0;   // 6° BTDC of overlap
pub const INTAKE_CLOSE_DEG:  f32 = 580.0;   // 40° ABDC
pub const EXHAUST_OPEN_DEG:  f32 = 140.0;   // 40° BBDC
pub const EXHAUST_CLOSE_DEG: f32 = 366.0;   // 6° ATDC of overlap

pub const PEAK_LIFT:      f32 = 0.010;       // 10 mm
pub const VALVE_DIAMETER: f32 = 0.034;       // 34 mm head

/// Sinusoidal lift profile centred between `open` and `close`.
fn lift_profile(open: f32, close: f32, phase_deg: f32) -> f32 {
    if open >= close { return 0.0; }
    if phase_deg < open || phase_deg > close { return 0.0; }
    let x = (phase_deg - open) / (close - open);
    (x * PI).sin() * PEAK_LIFT
}

/// Cylinder-local 4-stroke phase in degrees, given the global crank phase.
#[inline]
fn cyl_phase_deg(cyl_idx: usize, fourstroke_angle: f32) -> f32 {
    (fourstroke_angle.to_degrees() - FIRING_OFFSETS_DEG[cyl_idx]).rem_euclid(720.0)
}

pub fn intake_lift_for_cyl(cyl_idx: usize, fourstroke_angle: f32) -> f32 {
    lift_profile(INTAKE_OPEN_DEG, INTAKE_CLOSE_DEG, cyl_phase_deg(cyl_idx, fourstroke_angle))
}

pub fn exhaust_lift_for_cyl(cyl_idx: usize, fourstroke_angle: f32) -> f32 {
    lift_profile(EXHAUST_OPEN_DEG, EXHAUST_CLOSE_DEG, cyl_phase_deg(cyl_idx, fourstroke_angle))
}

/// Effective discharge area (Cd · A) of a poppet valve at the given lift.
///
/// At low lift the curtain area `π·D·lift` dominates; at high lift the seat
/// area `π·D²/4` is the limiting throat.
pub fn valve_area(lift: f32) -> f32 {
    if lift <= 0.0 { return 0.0; }
    let cd = 0.7;
    let curtain = PI * VALVE_DIAMETER * lift;
    let seat    = PI * VALVE_DIAMETER * VALVE_DIAMETER * 0.25;
    cd * curtain.min(seat)
}
