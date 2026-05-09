//! Engine geometry and slider-crank kinematics.
//!
//! Coordinate convention used throughout the codebase:
//!
//! * `+X` — along the crankshaft (cylinders are spaced along this axis)
//! * `+Y` — vertical / cylinder axis (pistons travel up to TDC, down to BDC)
//! * `+Z` — horizontal, perpendicular to the crank
//!
//! All physical constants are SI (metres, seconds, kilograms, Pascals, …).

use std::f32::consts::PI;

// ── Engine specification ────────────────────────────────────────────────────
pub const NUM_CYL:           usize = 4;
pub const BORE:              f32 = 0.086;                 // 86 mm
pub const STROKE:            f32 = 0.086;                 // 86 mm
pub const CRANK_RADIUS:      f32 = STROKE * 0.5;          // 43 mm
pub const ROD_LENGTH:        f32 = 0.145;                 // 145 mm
pub const COMPRESSION_RATIO: f32 = 10.5;

pub const PISTON_AREA:           f32 = PI * BORE * BORE * 0.25;
pub const DISPLACEMENT_PER_CYL:  f32 = PISTON_AREA * STROKE;
pub const TOTAL_DISPLACEMENT:    f32 = DISPLACEMENT_PER_CYL * NUM_CYL as f32;
pub const CLEARANCE_VOL:         f32 = DISPLACEMENT_PER_CYL / (COMPRESSION_RATIO - 1.0);
pub const V_TDC: f32 = CLEARANCE_VOL;
pub const V_BDC: f32 = CLEARANCE_VOL + DISPLACEMENT_PER_CYL;
pub const STROKE_TOP: f32 = CRANK_RADIUS + ROD_LENGTH;    // y_p at TDC

// ── Crank pin layout (inline-4: 1 & 4 share a pin, 2 & 3 share, 180° apart) ─
pub const CRANK_PHASES: [f32; NUM_CYL] = [0.0, PI, PI, 0.0];

// Firing offsets (deg) into the 720° four-stroke cycle.  Order: 1-3-4-2.
pub const FIRING_OFFSETS_DEG: [f32; NUM_CYL] = [0.0, 540.0, 180.0, 360.0];

// ── Visual scale (1 m → VIS_SCALE units in Bevy) ────────────────────────────
pub const VIS_SCALE:   f32 = 8.0;
pub const CYL_SPACING: f32 = 0.10;                        // m, for visuals
#[inline] pub fn cyl_x(i: usize) -> f32 { (i as f32 - 1.5) * CYL_SPACING * VIS_SCALE }

// ────────────────────────── Slider-crank kinematics ─────────────────────────
//
// Pin position for a cylinder of phase φ at crank angle θ:
//     pin = ( x_cyl,  R·cos(θ+φ),  R·sin(θ+φ) )
//
// Piston (constrained to the cylinder axis) sits at:
//     y_p = R·cos(θ+φ) + √( L² − R²·sin²(θ+φ) )

#[inline]
pub fn piston_y(theta: f32, phase: f32) -> f32 {
    let a = theta + phase;
    let s = a.sin();
    CRANK_RADIUS * a.cos() + (ROD_LENGTH * ROD_LENGTH - CRANK_RADIUS * CRANK_RADIUS * s * s).sqrt()
}

/// Derivative ∂y_p/∂θ — positive when the piston is moving away from the head.
/// Used to convert axial gas-force on the piston into torque on the crank:
/// `τ = −F · ∂y_p/∂θ` (sign convention: F positive when gas pushes piston
/// away from the head).
#[inline]
pub fn dpiston_dtheta(theta: f32, phase: f32) -> f32 {
    let a = theta + phase;
    let s = a.sin();
    let c = a.cos();
    let denom = (ROD_LENGTH * ROD_LENGTH - CRANK_RADIUS * CRANK_RADIUS * s * s).sqrt().max(1e-9);
    -CRANK_RADIUS * s - (CRANK_RADIUS * CRANK_RADIUS * s * c) / denom
}

/// In-cylinder volume at crank angle θ for a given crank phase.
#[inline]
pub fn cyl_volume(theta: f32, phase: f32) -> f32 {
    let displacement_from_tdc = STROKE_TOP - piston_y(theta, phase);
    CLEARANCE_VOL + PISTON_AREA * displacement_from_tdc
}
