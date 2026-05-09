//! Engine geometry and slider-crank kinematics.
//!
//! Coordinate convention used throughout the codebase:
//!
//! * `+X` — along the crankshaft (cylinders are spaced along this axis)
//! * `+Y` — vertical / cylinder axis (pistons travel up to TDC, down to BDC)
//! * `+Z` — horizontal, perpendicular to the crank
//!
//! All physical constants are SI (metres, seconds, kilograms, Pascals, …).
//!
//! **NOTE**: The constants below are kept for backward-compatibility and reflect
//! the default Inline-4 preset.  New code should read from [`super::config::EngineConfig`].

use std::f32::consts::PI;

use super::config::EngineConfig;

// ── Legacy constants (default Inline-4) ─────────────────────────────────────
pub const NUM_CYL:           usize = 4;
pub const BORE:              f32 = 0.086;
pub const STROKE:            f32 = 0.086;
pub const CRANK_RADIUS:      f32 = STROKE * 0.5;
pub const ROD_LENGTH:        f32 = 0.145;
pub const COMPRESSION_RATIO: f32 = 10.5;

pub const PISTON_AREA:           f32 = PI * BORE * BORE * 0.25;
pub const DISPLACEMENT_PER_CYL:  f32 = PISTON_AREA * STROKE;
pub const TOTAL_DISPLACEMENT:    f32 = DISPLACEMENT_PER_CYL * NUM_CYL as f32;
pub const CLEARANCE_VOL:         f32 = DISPLACEMENT_PER_CYL / (COMPRESSION_RATIO - 1.0);
pub const V_TDC: f32 = CLEARANCE_VOL;
pub const V_BDC: f32 = CLEARANCE_VOL + DISPLACEMENT_PER_CYL;
pub const STROKE_TOP: f32 = CRANK_RADIUS + ROD_LENGTH;

pub const CRANK_PHASES: [f32; NUM_CYL] = [0.0, PI, PI, 0.0];
pub const FIRING_OFFSETS_DEG: [f32; NUM_CYL] = [0.0, 540.0, 180.0, 360.0];

pub const VIS_SCALE:   f32 = 8.0;
pub const CYL_SPACING: f32 = 0.10;
#[inline] pub fn cyl_x(i: usize) -> f32 { (i as f32 - 1.5) * CYL_SPACING * VIS_SCALE }

// ────────────────────────── Slider-crank kinematics ─────────────────────────
//
// These free functions use the legacy constants.  The config-aware versions
// live on `EngineConfig` directly (see config.rs).

#[inline]
pub fn piston_y(theta: f32, cyl_idx: usize) -> f32 {
    let phase = CRANK_PHASES[cyl_idx];
    let a = theta + phase;
    let s = a.sin();
    CRANK_RADIUS * a.cos() + (ROD_LENGTH * ROD_LENGTH - CRANK_RADIUS * CRANK_RADIUS * s * s).sqrt()
}

#[inline]
pub fn dpiston_dtheta(theta: f32, cyl_idx: usize) -> f32 {
    let phase = CRANK_PHASES[cyl_idx];
    let a = theta + phase;
    let s = a.sin();
    let c = a.cos();
    let denom = (ROD_LENGTH * ROD_LENGTH - CRANK_RADIUS * CRANK_RADIUS * s * s).sqrt().max(1e-9);
    -CRANK_RADIUS * s - (CRANK_RADIUS * CRANK_RADIUS * s * c) / denom
}

#[inline]
pub fn cyl_volume(theta: f32, cyl_idx: usize) -> f32 {
    let displacement_from_tdc = STROKE_TOP - piston_y(theta, cyl_idx);
    CLEARANCE_VOL + PISTON_AREA * displacement_from_tdc
}

// ────────────────────────── Config-aware free functions ──────────────────────

#[inline]
pub fn piston_y_cfg(cfg: &EngineConfig, theta: f32, cyl_idx: usize) -> f32 {
    cfg.piston_y(theta, cyl_idx)
}

#[inline]
pub fn dpiston_dtheta_cfg(cfg: &EngineConfig, theta: f32, cyl_idx: usize) -> f32 {
    cfg.dpiston_dtheta(theta, cyl_idx)
}

#[inline]
pub fn cyl_volume_cfg(cfg: &EngineConfig, theta: f32, cyl_idx: usize) -> f32 {
    cfg.cyl_volume(theta, cyl_idx)
}
