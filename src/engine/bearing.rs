//! Journal bearing physics model.
//!
//! Covers the three classes of plain bearing in a piston engine:
//!
//! | Type         | Location                                 | Count              |
//! |--------------|------------------------------------------|--------------------|
//! | **Main**     | Crankshaft journals in the block          | ~(n_cyl/2 + 1)    |
//! | **Rod**      | Big-end (crank-pin) per connecting rod    | 1 per cylinder     |
//! | **Cam**      | Camshaft journals in the head             | 1 aggregate        |
//!
//! Each bearing is modelled as a simplified Sommerfeld-number hydrodynamic
//! bearing.  When the dimensionless Sommerfeld number is high the shaft rides
//! on a full oil film (near-zero wear, low friction).  When the number drops
//! (heavy load, low speed, thin oil, large clearance) the bearing transitions
//! through mixed lubrication into boundary contact, causing Archard-type wear
//! on the soft shell material and generating significant frictional heat.
//!
//! Failure modes:
//!   * **Wipe-out** — bearing shell temperature exceeds the shell material's
//!     melting point (e.g. Babbitt at ~520 K).
//!   * **Spin**     — shell wear > 0.9 under high load → the shell rotates in
//!     its housing, blocking oil feed and seizing the engine.

use super::material::{Material, BABBIT, FORGED_STEEL, STOCK_STEEL};
use super::oil::OilState;
use super::oil::OilConfig;

// ═══════════════════════════════════════════════════════════════════════════════
// Configuration (per bearing)
// ═══════════════════════════════════════════════════════════════════════════════

/// Physical description of a single journal bearing.
#[derive(Clone, Debug)]
pub struct BearingConfig {
    /// Journal (shaft) diameter (m).
    pub diameter: f32,
    /// Axial width of the bearing shell (m).
    pub width: f32,
    /// Radial clearance between journal and shell (m).
    /// Typical: 0.025–0.050 mm.
    pub clearance: f32,
    /// The soft sacrificial shell lining.
    pub shell_material: Material,
    /// The rotating journal surface (crank pin, camshaft, etc.).
    pub journal_material: Material,
}

impl BearingConfig {
    /// Projected bearing area (D × L).
    #[inline]
    pub fn projected_area(&self) -> f32 {
        self.diameter * self.width
    }

    /// Default main bearing sized proportionally to bore.
    pub fn default_main(bore: f32) -> Self {
        Self {
            diameter: bore * 0.70,         // ~60 mm for an 86 mm bore
            width: bore * 0.30,            // ~26 mm
            clearance: 0.000_040,          // 40 µm
            shell_material: BABBIT,
            journal_material: FORGED_STEEL,
        }
    }

    /// Default rod (big-end) bearing — slightly smaller than main.
    pub fn default_rod(bore: f32) -> Self {
        Self {
            diameter: bore * 0.55,         // ~47 mm
            width: bore * 0.25,            // ~22 mm
            clearance: 0.000_035,          // 35 µm
            shell_material: BABBIT,
            journal_material: FORGED_STEEL,
        }
    }

    /// Default cam bearing — small, lightly loaded.
    pub fn default_cam(bore: f32) -> Self {
        Self {
            diameter: bore * 0.35,         // ~30 mm
            width: bore * 0.20,            // ~17 mm
            clearance: 0.000_045,          // 45 µm
            shell_material: BABBIT,
            journal_material: STOCK_STEEL,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Live state (per bearing instance)
// ═══════════════════════════════════════════════════════════════════════════════

/// Runtime state of a single journal bearing.
#[derive(Clone, Debug)]
pub struct BearingState {
    /// 0..1 — fraction of shell service life consumed.  1.0 = wiped out.
    pub shell_wear: f32,
    /// Bearing shell temperature (K).
    pub temperature: f32,
    /// Minimum oil-film thickness this substep (m) — telemetry.
    pub film_thickness: f32,
    /// Sommerfeld number this substep — telemetry.
    pub sommerfeld: f32,
    /// True when the shell has rotated in its housing (catastrophic).
    pub spun: bool,
    /// True when the shell material has melted / wiped out.
    pub wiped: bool,
}

impl BearingState {
    pub fn fresh() -> Self {
        Self {
            shell_wear: 0.0,
            temperature: super::thermo::T_ATM,
            film_thickness: 0.0,
            sommerfeld: 0.0,
            spun: false,
            wiped: false,
        }
    }

    /// Has this bearing failed catastrophically?
    #[inline]
    pub fn is_failed(&self) -> bool {
        self.spun || self.wiped
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Per-substep bearing physics
// ═══════════════════════════════════════════════════════════════════════════════

/// Result of stepping one bearing for one substep.
#[derive(Clone, Copy, Debug, Default)]
pub struct BearingStepResult {
    /// Friction torque at the journal surface (Nm, ≥ 0).
    pub friction_torque: f32,
    /// Heat dumped into the oil (W).
    pub heat_to_oil_w: f32,
    /// True if this substep caused a seizure event.
    pub seized: bool,
}

/// Step a single journal bearing for one substep.
///
/// # Arguments
/// * `cfg`             — bearing geometry and materials
/// * `state`           — mutable bearing state (wear, temp, etc.)
/// * `radial_load`     — net radial force on the journal (N)
/// * `shaft_omega`     — angular velocity of the journal (rad/s)
/// * `oil`             — current oil state (viscosity, pressure, temperature)
/// * `oil_cfg`         — oil system configuration
/// * `dt`              — substep duration (s)
/// * `wear_time_scale` — Archard wear multiplier (same as the cylinder system)
pub fn step_bearing(
    cfg: &BearingConfig,
    state: &mut BearingState,
    radial_load: f32,
    shaft_omega: f32,
    oil: &OilState,
    oil_cfg: &OilConfig,
    dt: f32,
    wear_time_scale: f32,
) -> BearingStepResult {
    // Already catastrophically failed — infinite drag.
    if state.is_failed() {
        return BearingStepResult {
            friction_torque: 500.0,   // locked-up drag
            heat_to_oil_w: 0.0,
            seized: true,
        };
    }

    let r = cfg.diameter * 0.5;          // journal radius
    let c = cfg.clearance.max(1.0e-6);   // radial clearance (never zero)
    let area = cfg.projected_area();
    let w = radial_load.abs().max(1.0);  // load, never zero for stability

    // Journal surface speed
    let v_surface = shaft_omega.abs() * r;
    let rev_per_s = shaft_omega.abs() / std::f32::consts::TAU;

    // ── Sommerfeld number ──────────────────────────────────────────────────
    // S = (µ · N · D · L / W) · (r/c)²
    //
    // The (r/c)² term is essential — for typical engine journals r/c ≈ 700,
    // so omitting it would inflate S by ~500 000× and push the bearing
    // permanently into the hydrodynamic regime.
    //
    // High S → thick film → hydrodynamic.
    // Low S  → thin film → boundary/mixed.
    let mu = oil.viscosity.max(0.001);
    let r_over_c = (cfg.diameter * 0.5) / c;
    let sommerfeld = (mu * rev_per_s * cfg.diameter * cfg.width)
        / (w * c * c + 1.0e-12)
        * (r_over_c * r_over_c);
    state.sommerfeld = sommerfeld;

    // ── Film thickness estimate (with Squeeze-Film effect) ─────────────────
    // Minimum film h_min ≈ c · (1 - ε), where eccentricity ratio ε ≈ 1/(1+k·S).
    // Coefficient k=4.0 is fitted to Raimondi-Boyd curves for L/D ≈ 0.4–0.5
    // bearings, using the correctly-scaled Sommerfeld number above.
    // Static calculation would cause instant collapse under peak combustion.
    // Real oil films take time to squeeze out, protecting the bearing.
    let epsilon = 1.0 / (1.0 + 4.0 * sommerfeld.max(0.0));
    let target_h_min = c * (1.0 - epsilon);

    // Squeeze out slowly, draw in quickly
    let alpha = if target_h_min < state.film_thickness {
        (15.0 * dt).clamp(0.0, 1.0)
    } else {
        (150.0 * dt).clamp(0.0, 1.0)
    };
    
    // Smooth the actual film thickness toward the target
    let h_min = state.film_thickness + (target_h_min - state.film_thickness) * alpha;
    state.film_thickness = h_min;

    // ── Lubrication regime (0 = boundary, 1 = full hydrodynamic) ──────────
    // Transition around h_min ≈ 0.5 µm (typical surface roughness of micro-polished engine journals).
    // Using 3.0 µm was artificially throwing the bearing into metal-on-metal contact prematurely.
    let roughness = 0.5e-6_f32;
    let lambda = h_min / roughness;       // film-thickness ratio
    let regime = ((lambda - 1.0) / 2.0).clamp(0.0, 1.0); // 0..1

    // Oil pressure contribution — if the pump isn't delivering, there's no
    // hydrodynamic wedge regardless of Sommerfeld.
    let oil_presence = oil.lubrication_factor(oil_cfg).clamp(0.0, 1.0);
    let effective_regime = regime * oil_presence;

    // ── Friction ───────────────────────────────────────────────────────────
    // Hydrodynamic: Petroff friction  F_h = π · µ · v · A / c
    //   (the π arises from integrating viscous shear over the full 2π journal
    //    circumference; omitting it underestimates hydrodynamic friction by ~3×)
    // Boundary:     Coulomb from contact surface
    let f_hydro = std::f32::consts::PI * mu * v_surface * area / c;

    let mu_dry = (cfg.shell_material.friction_coeff
        + cfg.journal_material.friction_coeff) * 0.5;
    let f_boundary = mu_dry * w;

    let friction_force = effective_regime * f_hydro
        + (1.0 - effective_regime) * f_boundary;

    let friction_torque = friction_force * r;
    let friction_power = friction_force * v_surface;

    // ── Heat ───────────────────────────────────────────────────────────────
    // Hydrodynamic friction mostly heats the oil film directly (carried away
    // by side leakage). Boundary friction heats the metal asperities directly.
    // The shell (being thin) only absorbs a fraction of this heat.
    let heat_to_shell_w = effective_regime * f_hydro * v_surface * 0.05
        + (1.0 - effective_regime) * f_boundary * v_surface * 0.50;
    
    let heat_j = heat_to_shell_w * dt;

    // Bearing shell temperature: heated by friction, cooled by oil flow.
    let shell_thermal_mass = cfg.shell_material.density
        * area * 0.002          // ~2 mm shell thickness
        * cfg.shell_material.specific_heat.max(100.0);
    let shell_thermal_mass = shell_thermal_mass.max(0.5);  // J/K floor

    state.temperature += heat_j / shell_thermal_mass;

    // Cooling: oil carries heat away proportional to flow (≈ oil pressure).
    // Forced convection coefficient.
    let oil_cooling = (state.temperature - oil.temperature)
        * 15.0 * oil_presence * dt;
    state.temperature -= oil_cooling;

    // Ambient dissipation (through massive block/rod)
    let ambient_cooling = (state.temperature - super::thermo::T_ATM) * 2.0 * dt;
    state.temperature -= ambient_cooling;

    state.temperature = state.temperature.clamp(
        super::thermo::T_ATM - 5.0,
        3000.0,
    );

    // ── Wear (Archard) ────────────────────────────────────────────────────
    // Only the boundary-contact fraction causes wear.
    let boundary_frac = (1.0 - effective_regime).max(0.0);
    let dist = v_surface * dt;
    let wear_k: f32 = 1.0e-12;
    let wear_vol = wear_k * boundary_frac * w * dist
        / cfg.shell_material.hardness.max(1.0);

    // Normalize into 0..1 service life — same WEAR_NORM as cylinders.
    const WEAR_NORM: f32 = 1.0e9;
    let scale = wear_time_scale.max(0.0);
    state.shell_wear = (state.shell_wear + wear_vol * WEAR_NORM * scale)
        .clamp(0.0, 1.0);

    // ── Failure checks ────────────────────────────────────────────────────
    // Wipe-out: shell material melts.
    if state.temperature > cfg.shell_material.melting_point {
        state.wiped = true;
    }

    // Bearing spin: severely worn shell under high load loses its interference
    // fit and rotates in the housing, blocking the oil feed hole.
    if state.shell_wear > 0.9 && w > 5000.0 {
        state.spun = true;
    }

    let seized = state.is_failed();

    BearingStepResult {
        friction_torque,
        // Partition friction power: shell absorbs heat_to_shell_w; the rest
        // is carried away by circulating oil.  Returning the full friction_power
        // would double-count energy (the shell would also be heated by heat_to_shell_w
        // in the same substep).
        heat_to_oil_w: (friction_power - heat_to_shell_w).max(0.0),
        seized,
    }
}

/// Estimate the number of main bearings for a given cylinder count.
/// Inline: n_cyl + 1 (one between each cylinder plus two ends).
/// V/Flat: (n_cyl / 2) + 1.
#[inline]
pub fn main_bearing_count(num_cylinders: usize, is_inline: bool) -> usize {
    if is_inline {
        num_cylinders + 1
    } else {
        num_cylinders / 2 + 1
    }
}
