//! Data-driven engine configuration.
//!
//! Every physical parameter that defines an engine type lives here in a single
//! [`EngineConfig`] struct.  The simulation reads from the active config at
//! runtime, so swapping engine types is just a matter of picking a different
//! preset (or building one from scratch).
//!
//! Presets are defined in [`ENGINES`].

use std::f32::consts::PI;
use std::sync::LazyLock;

// ── Maximum supported cylinder count ─────────────────────────────────────────
pub const MAX_CYL: usize = 1000;

/// Physical layout of the engine block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EngineLayout {
    /// All cylinders in a single row (bank angle = 0).
    Inline,
    /// Two banks at an angle (e.g. 60°, 90°).  Even-indexed cylinders on bank A,
    /// odd-indexed on bank B.
    V,
    /// Flat / boxer: two banks at 180°.
    Flat,
}

/// Complete physical description of an engine.
#[derive(Clone, Debug)]
pub struct EngineConfig {
    // ── Identity ─────────────────────────────────────────────────────────────
    pub name: &'static str,

    // ── Layout ───────────────────────────────────────────────────────────────
    pub layout: EngineLayout,
    /// Full included angle between the two cylinder banks (radians).
    /// Ignored for Inline.  For a 90° V8 this would be PI/2.
    pub bank_angle: f32,

    // ── Geometry ─────────────────────────────────────────────────────────────
    pub num_cylinders: usize,
    pub bore: f32,              // m
    pub stroke: f32,            // m
    pub rod_length: f32,        // m
    pub compression_ratio: f32,
    /// Crank pin phases (rad).  Length must == num_cylinders.
    pub crank_phases: Vec<f32>,
    /// Firing offsets into the 720° four-stroke cycle (degrees).
    pub firing_offsets_deg: Vec<f32>,

    // ── Dynamics ─────────────────────────────────────────────────────────────
    pub flywheel_inertia: f32,      // kg·m²
    pub friction_base: f32,         // Nm — Coulomb friction floor
    pub friction_viscous: f32,      // Nm·s/rad
    pub friction_windage: f32,      // Nm·s²/rad²

    // ── Starter ──────────────────────────────────────────────────────────────
    pub starter_torque: f32,        // Nm at 0 RPM
    pub starter_disengage_rpm: f32,

    // ── Limits ───────────────────────────────────────────────────────────────
    pub redline_rpm: f32,
    pub stall_rpm: f32,

    // ── Throttle body ────────────────────────────────────────────────────────
    pub throttle_area_max: f32,     // m² — wide-open throttle
    pub idle_bleed_frac: f32,       // fraction of area open at 0% throttle
    pub idle_throttle_min: f32,     // effective minimum throttle (0..1)

    // ── Intake / exhaust manifold volumes ────────────────────────────────────
    pub intake_volume: f32,         // m³
    pub exhaust_volume: f32,        // m³
    pub tailpipe_area: f32,         // m²

    // ── Valve timing (cam profile, in degrees of 4-stroke phase) ─────────────
    pub intake_open_deg: f32,
    pub intake_close_deg: f32,
    pub exhaust_open_deg: f32,
    pub exhaust_close_deg: f32,
    pub intake_peak_lift: f32,      // m
    pub exhaust_peak_lift: f32,     // m
    pub intake_valve_diameter: f32, // m
    pub exhaust_valve_diameter: f32,// m

    // ── Visual layout ────────────────────────────────────────────────────────
    pub cylinder_spacing: f32,      // m between cylinder centres (for 3D)
}

// ─────────────────────────── Derived helpers ─────────────────────────────────
impl EngineConfig {
    #[inline] pub fn crank_radius(&self) -> f32 { self.stroke * 0.5 }
    #[inline] pub fn piston_area(&self) -> f32 { PI * self.bore * self.bore * 0.25 }
    #[inline] pub fn displacement_per_cyl(&self) -> f32 { self.piston_area() * self.stroke }
    #[inline] pub fn total_displacement(&self) -> f32 { self.displacement_per_cyl() * self.num_cylinders as f32 }
    #[inline] pub fn clearance_vol(&self) -> f32 { self.displacement_per_cyl() / (self.compression_ratio - 1.0) }
    #[inline] pub fn stroke_top(&self) -> f32 { self.crank_radius() + self.rod_length }

    /// Friction torque opposing motion at given omega.
    #[inline]
    pub fn friction_torque(&self, omega: f32) -> f32 {
        if omega <= 0.0 { return 0.0; }
        self.friction_base + self.friction_viscous * omega + self.friction_windage * omega * omega
    }

    /// Starter motor torque curve.
    #[inline]
    pub fn starter_torque_at(&self, rpm: f32, active: bool) -> f32 {
        if !active || rpm >= self.starter_disengage_rpm { return 0.0; }
        let factor = (1.0 - rpm / self.starter_disengage_rpm).max(0.0);
        self.starter_torque * factor
    }

    /// Effective throttle opening (idle minimum applied).
    #[inline]
    pub fn effective_throttle(&self, player_throttle: f32, cranking: bool) -> f32 {
        if cranking { 0.10 } else {
            (self.idle_throttle_min + (1.0 - self.idle_throttle_min) * player_throttle).clamp(0.0, 1.0)
        }
    }

    /// Throttle body discharge area at a given effective throttle fraction.
    #[inline]
    pub fn throttle_area(&self, throttle: f32) -> f32 {
        (self.idle_bleed_frac + (1.0 - self.idle_bleed_frac) * throttle * throttle) * self.throttle_area_max
    }

    /// Piston position (y_p) for slider-crank.
    #[inline]
    pub fn piston_y(&self, theta: f32, phase: f32) -> f32 {
        let r = self.crank_radius();
        let l = self.rod_length;
        let a = theta + phase;
        let s = a.sin();
        r * a.cos() + (l * l - r * r * s * s).sqrt()
    }

    /// ∂y_p/∂θ — used for torque conversion.
    #[inline]
    pub fn dpiston_dtheta(&self, theta: f32, phase: f32) -> f32 {
        let r = self.crank_radius();
        let l = self.rod_length;
        let a = theta + phase;
        let s = a.sin();
        let c = a.cos();
        let denom = (l * l - r * r * s * s).sqrt().max(1e-9);
        -r * s - (r * r * s * c) / denom
    }

    /// In-cylinder volume at crank angle θ for a given crank phase.
    #[inline]
    pub fn cyl_volume(&self, theta: f32, phase: f32) -> f32 {
        let displacement_from_tdc = self.stroke_top() - self.piston_y(theta, phase);
        self.clearance_vol() + self.piston_area() * displacement_from_tdc
    }

    /// Visual X offset for cylinder index (inline layout).
    #[inline]
    pub fn cyl_x(&self, i: usize) -> f32 {
        let center = (self.num_cylinders as f32 - 1.0) * 0.5;
        (i as f32 - center) * self.cylinder_spacing * VIS_SCALE
    }

    /// Number of crank positions along X for this layout.
    /// Inline: num_cylinders.  V/Flat: num_cylinders / 2 (pairs share a crank pin X).
    #[inline]
    pub fn crank_positions(&self) -> usize {
        match self.layout {
            EngineLayout::Inline => self.num_cylinders,
            EngineLayout::V | EngineLayout::Flat => (self.num_cylinders + 1) / 2,
        }
    }

    /// Visual X position along the crankshaft for a cylinder.
    /// For V/Flat engines, paired cylinders share the same X.
    #[inline]
    pub fn cyl_visual_x(&self, i: usize) -> f32 {
        let positions = self.crank_positions();
        let idx_along_crank = match self.layout {
            EngineLayout::Inline => i,
            EngineLayout::V | EngineLayout::Flat => i / 2,
        };
        let center = (positions as f32 - 1.0) * 0.5;
        (idx_along_crank as f32 - center) * self.cylinder_spacing * VIS_SCALE
    }

    /// Bank tilt angle for a cylinder (rotation around the X/crank axis).
    /// Returns 0 for inline.  For V/Flat: +half_angle for even (bank A),
    /// -half_angle for odd (bank B).
    #[inline]
    pub fn cyl_bank_tilt(&self, i: usize) -> f32 {
        match self.layout {
            EngineLayout::Inline => 0.0,
            EngineLayout::V | EngineLayout::Flat => {
                let half = self.bank_angle * 0.5;
                if i % 2 == 0 { half } else { -half }
            }
        }
    }

    /// Which bank (0 or 1) a cylinder belongs to.  Inline always returns 0.
    #[inline]
    pub fn cyl_bank(&self, i: usize) -> usize {
        match self.layout {
            EngineLayout::Inline => 0,
            EngineLayout::V | EngineLayout::Flat => i % 2,
        }
    }
}

// VIS_SCALE lives in geometry.rs to avoid ambiguity with glob re-exports.
use super::geometry::VIS_SCALE;

// ══════════════════════════════════════════════════════════════════════════════
// ENGINE PRESETS (lazily initialized since we now use Vec)
// ══════════════════════════════════════════════════════════════════════════════

pub static ENGINES: LazyLock<Vec<EngineConfig>> = LazyLock::new(|| vec![
    // ── 2.0L Inline-4 (like a Honda K20 / Toyota 3S-GE) ─────────────────────
    EngineConfig {
        name: "2.0L Inline-4",
        layout: EngineLayout::Inline,
        bank_angle: 0.0,
        num_cylinders: 4,
        bore: 0.086,
        stroke: 0.086,
        rod_length: 0.145,
        compression_ratio: 10.5,
        crank_phases: vec![0.0, PI, PI, 0.0],
        firing_offsets_deg: vec![0.0, 540.0, 180.0, 360.0],

        flywheel_inertia: 0.18,
        friction_base: 12.0,
        friction_viscous: 0.045,
        friction_windage: 0.00012,

        starter_torque: 80.0,
        starter_disengage_rpm: 600.0,
        redline_rpm: 8000.0,
        stall_rpm: 220.0,

        throttle_area_max: 0.0014,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.015,

        intake_volume: 0.0020,
        exhaust_volume: 0.0015,
        tailpipe_area: 0.0010,

        intake_open_deg: 354.0,
        intake_close_deg: 580.0,
        exhaust_open_deg: 140.0,
        exhaust_close_deg: 366.0,
        intake_peak_lift: 0.010,
        exhaust_peak_lift: 0.010,
        intake_valve_diameter: 0.034,
        exhaust_valve_diameter: 0.030,

        cylinder_spacing: 0.10,
    },

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
        crank_phases: vec![0.0, PI, PI, 0.0, PI * 0.5, PI * 1.5, PI * 1.5, PI * 0.5],
        firing_offsets_deg: vec![0.0, 90.0, 270.0, 360.0, 450.0, 540.0, 630.0, 180.0],

        flywheel_inertia: 0.35,
        friction_base: 22.0,
        friction_viscous: 0.065,
        friction_windage: 0.00018,

        starter_torque: 120.0,
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

        cylinder_spacing: 0.11,
    },

    // ── 3.8L Flat-6 (like a Porsche 997) ────────────────────────────────────
    EngineConfig {
        name: "3.8L Flat-6",
        layout: EngineLayout::Flat,
        bank_angle: PI,
        num_cylinders: 6,
        bore: 0.102,
        stroke: 0.077,
        rod_length: 0.128,
        compression_ratio: 12.5,
        crank_phases: vec![0.0, 2.0*PI/3.0, 4.0*PI/3.0, PI, PI + 2.0*PI/3.0, PI + 4.0*PI/3.0],
        firing_offsets_deg: vec![0.0, 120.0, 240.0, 360.0, 480.0, 600.0],

        flywheel_inertia: 0.14,
        friction_base: 16.0,
        friction_viscous: 0.050,
        friction_windage: 0.00015,

        starter_torque: 90.0,
        starter_disengage_rpm: 550.0,
        redline_rpm: 8500.0,
        stall_rpm: 250.0,

        throttle_area_max: 0.0020,
        idle_bleed_frac: 0.010,
        idle_throttle_min: 0.013,

        intake_volume: 0.0035,
        exhaust_volume: 0.0025,
        tailpipe_area: 0.0014,

        intake_open_deg: 348.0,
        intake_close_deg: 576.0,
        exhaust_open_deg: 136.0,
        exhaust_close_deg: 368.0,
        intake_peak_lift: 0.011,
        exhaust_peak_lift: 0.011,
        intake_valve_diameter: 0.036,
        exhaust_valve_diameter: 0.031,

        cylinder_spacing: 0.12,
    },
]);

#[inline]
pub fn engine_count() -> usize { ENGINES.len() }

// ══════════════════════════════════════════════════════════════════════════════
// DYNAMIC ENGINE GENERATION — build any cylinder count + layout at runtime
// ══════════════════════════════════════════════════════════════════════════════

/// Generate evenly-spaced crank phases for an inline engine with `n` cylinders.
pub fn generate_inline_phases(n: usize) -> Vec<f32> {
    // Use the standard even-fire pattern: 720° / n between firings.
    // Crank phase = firing_offset converted to crank radians.
    let spacing_deg = 720.0 / n as f32;
    let firing: Vec<f32> = (0..n).map(|i| i as f32 * spacing_deg).collect();
    // For an even-fire inline, crank phases alternate 0 and PI for 4-cyl,
    // or are evenly spaced in crank angle for other counts.
    let mut phases = vec![0.0_f32; n];
    for i in 0..n {
        // Convert firing offset (degrees in 720° cycle) to crank phase (radians in 360° cycle)
        phases[i] = (firing[i] % 360.0) * PI / 180.0;
    }
    phases
}

/// Generate evenly-spaced crank phases for a V or Flat engine with `n` cylinders.
/// Cylinders are interleaved: even indices on bank A, odd on bank B.
pub fn generate_v_phases(n: usize, bank_angle: f32) -> Vec<f32> {
    let pairs = n / 2;
    let spacing_deg = 720.0 / pairs as f32;
    let mut phases = vec![0.0_f32; n];
    for pair in 0..pairs {
        let base_phase = (pair as f32 * spacing_deg % 360.0) * PI / 180.0;
        // Bank A (even index)
        phases[pair * 2] = base_phase;
        // Bank B (odd index) offset by bank angle expressed as crank angle
        phases[pair * 2 + 1] = base_phase + bank_angle * 0.5;
    }
    // Handle odd cylinder count (last one goes on bank A)
    if n % 2 == 1 {
        let base_phase = (pairs as f32 * spacing_deg % 360.0) * PI / 180.0;
        phases[n - 1] = base_phase;
    }
    phases
}

/// Generate even-fire firing offsets for `n` cylinders (degrees, 0..720).
pub fn generate_firing_offsets(n: usize) -> Vec<f32> {
    let spacing = 720.0 / n as f32;
    (0..n).map(|i| i as f32 * spacing).collect()
}

/// Build a complete EngineConfig for an arbitrary cylinder count and layout.
/// Scales physical parameters proportionally from a base inline-4.
pub fn build_engine(
    name: &'static str,
    layout: EngineLayout,
    bank_angle: f32,
    num_cylinders: usize,
    bore: f32,
    stroke: f32,
    rod_length: f32,
    compression_ratio: f32,
    redline_rpm: f32,
) -> EngineConfig {
    let num_cylinders = num_cylinders.min(MAX_CYL).max(1);
    let crank_phases = match layout {
        EngineLayout::Inline => generate_inline_phases(num_cylinders),
        EngineLayout::V | EngineLayout::Flat => generate_v_phases(num_cylinders, bank_angle),
    };
    let firing_offsets_deg = generate_firing_offsets(num_cylinders);

    // Scale dynamics with cylinder count
    let scale = num_cylinders as f32 / 4.0;
    EngineConfig {
        name,
        layout,
        bank_angle,
        num_cylinders,
        bore,
        stroke,
        rod_length,
        compression_ratio,
        crank_phases,
        firing_offsets_deg,

        flywheel_inertia: 0.18 * scale.sqrt(),
        friction_base: 12.0 * scale,
        friction_viscous: 0.045 * scale,
        friction_windage: 0.00012 * scale,

        starter_torque: 80.0 * scale.max(1.0),
        starter_disengage_rpm: 600.0,
        redline_rpm,
        stall_rpm: 220.0,

        throttle_area_max: 0.0014 * scale,
        idle_bleed_frac: 0.012,
        idle_throttle_min: 0.015,

        intake_volume: 0.0020 * scale,
        exhaust_volume: 0.0015 * scale,
        tailpipe_area: 0.0010 * scale,

        intake_open_deg: 354.0,
        intake_close_deg: 580.0,
        exhaust_open_deg: 140.0,
        exhaust_close_deg: 366.0,
        intake_peak_lift: 0.010,
        exhaust_peak_lift: 0.010,
        intake_valve_diameter: 0.034,
        exhaust_valve_diameter: 0.030,

        cylinder_spacing: 0.10,
    }
}
