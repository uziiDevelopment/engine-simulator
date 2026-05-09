//! Crankshaft rotational dynamics (the only true degree of freedom).
//!
//! The integrator lives in `engine.rs::engine_step`.  This module just owns
//! the torque-source pieces that aren't combustion: friction, the starter
//! motor, and a few RPM thresholds.

// ── Dynamic constants ───────────────────────────────────────────────────────
pub const FLYWHEEL_INERTIA:  f32 = 0.18;  // kg·m² — crank + flywheel + clutch
pub const FRICTION_BASE:     f32 = 12.0;  // Nm — Coulomb friction floor (valve train + seals + accessories)
pub const FRICTION_VISCOUS:  f32 = 0.045; // Nm·s/rad (oil shear)
pub const FRICTION_WINDAGE:  f32 = 0.00012; // Nm·s²/rad² (windage at high RPM)

pub const STARTER_TORQUE:        f32 = 80.0;   // Nm at 0 RPM
pub const STARTER_DISENGAGE_RPM: f32 = 600.0;
pub const REDLINE_RPM:           f32 = 8000.0;
pub const STALL_RPM:             f32 = 220.0;

/// Friction torque opposing motion.  Always non-negative.
#[inline]
pub fn friction_torque(omega: f32) -> f32 {
    if omega <= 0.0 { return 0.0; }
    FRICTION_BASE + FRICTION_VISCOUS * omega + FRICTION_WINDAGE * omega * omega
}

/// Starter-motor torque curve: peak at 0 RPM, linearly falling to 0 at the
/// disengage threshold.
#[inline]
pub fn starter_torque(rpm: f32, active: bool) -> f32 {
    if !active || rpm >= STARTER_DISENGAGE_RPM { return 0.0; }
    let factor = (1.0 - rpm / STARTER_DISENGAGE_RPM).max(0.0);
    STARTER_TORQUE * factor
}
