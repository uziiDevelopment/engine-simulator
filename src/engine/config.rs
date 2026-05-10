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
use super::material::{Material, CAST_IRON, ALUMINUM_ALLOY, STOCK_STEEL, FORGED_STEEL};
use super::bearing::BearingConfig;

// ── Per-engine preset submodules ──────────────────────────────────────────────
pub mod inline4;
pub mod inline5;
pub mod inline6;
pub mod v8;
pub mod v10;
pub mod v12;
pub mod w16;
pub mod flat6;
pub mod f1_V6;

#[derive(Clone, Debug)]
pub struct MaterialsConfig {
    pub block: Material,
    pub cylinder_wall: Material,
    pub piston: Material,
    pub piston_ring: Material,
    pub conrod: Material,
    // ── Journal bearings ────────────────────────────────────────────────────
    pub main_bearing: BearingConfig,
    pub rod_bearing: BearingConfig,
    pub cam_bearing: BearingConfig,
}

impl MaterialsConfig {
    /// Build a default materials config with bearing sizes proportional to the
    /// supplied bore diameter.
    pub fn default_for_bore(bore: f32) -> Self {
        Self {
            block: CAST_IRON,
            cylinder_wall: CAST_IRON,
            piston: ALUMINUM_ALLOY,
            piston_ring: STOCK_STEEL,
            conrod: FORGED_STEEL,
            main_bearing: BearingConfig::default_main(bore),
            rod_bearing: BearingConfig::default_rod(bore),
            cam_bearing: BearingConfig::default_cam(bore),
        }
    }
}

impl Default for MaterialsConfig {
    fn default() -> Self {
        Self::default_for_bore(0.086) // default inline-4 bore
    }
}

// ── Maximum supported cylinder count ─────────────────────────────────────────
pub const MAX_CYL: usize = 1000;

/// Physical layout of the engine block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EngineLayout {
    /// All cylinders in a single row (bank angle = 0).
    Inline,
    /// Two banks at an angle (e.g. 60°, 90°).  Even-indexed cylinders on bank A,
    /// odd-indexed on bank B.
    V,
    /// Flat / boxer: two banks at 180°.
    Flat,
    /// W-layout (e.g. Bugatti W16): two narrow-angle VR clusters at 90° to each other.
    /// `narrow_angle` is the angle between the two banks within each VR sub-cluster (rad).
    W { narrow_angle: f32 },
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

    // ── Materials ────────────────────────────────────────────────────────────
    pub materials: MaterialsConfig,
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
    pub fn piston_y(&self, theta: f32, cyl_idx: usize) -> f32 {
        let r = self.crank_radius();
        let l = self.rod_length;
        let tilt = self.cyl_bank_tilt(cyl_idx);
        let phase = self.crank_phases[cyl_idx] - tilt;
        let a = theta + phase;
        let s = a.sin();
        r * a.cos() + (l * l - r * r * s * s).sqrt()
    }

    /// ∂y_p/∂θ — used for torque conversion.
    #[inline]
    pub fn dpiston_dtheta(&self, theta: f32, cyl_idx: usize) -> f32 {
        let r = self.crank_radius();
        let l = self.rod_length;
        let tilt = self.cyl_bank_tilt(cyl_idx);
        let phase = self.crank_phases[cyl_idx] - tilt;
        let a = theta + phase;
        let s = a.sin();
        let c = a.cos();
        let denom = (l * l - r * r * s * s).sqrt().max(1e-9);
        -r * s - (r * r * s * c) / denom
    }

    /// In-cylinder volume at crank angle θ for a given cylinder.
    #[inline]
    pub fn cyl_volume(&self, theta: f32, cyl_idx: usize) -> f32 {
        let displacement_from_tdc = self.stroke_top() - self.piston_y(theta, cyl_idx);
        self.clearance_vol() + self.piston_area() * displacement_from_tdc
    }

    /// Visual X offset for cylinder index (inline layout).
    #[inline]
    pub fn cyl_x(&self, i: usize) -> f32 {
        let center = (self.num_cylinders as f32 - 1.0) * 0.5;
        (i as f32 - center) * self.cylinder_spacing * VIS_SCALE
    }

    /// Number of crank positions along X for this layout.
    /// Inline and Flat: num_cylinders.  V: (num_cylinders + 1) / 2.  W: num_cylinders / 4.
    #[inline]
    pub fn crank_positions(&self) -> usize {
        match self.layout {
            EngineLayout::Inline | EngineLayout::Flat => self.num_cylinders,
            EngineLayout::V => (self.num_cylinders + 1) / 2,
            EngineLayout::W { .. } => self.num_cylinders / 4,
        }
    }

    /// Visual X position along the crankshaft for a cylinder.
    /// For V engines, paired cylinders share the same X.
    /// For Flat engines, cylinders are grouped closely in opposing pairs.
    /// For W engines, 4 cylinders share each axial position.
    #[inline]
    pub fn cyl_visual_x(&self, i: usize) -> f32 {
        match self.layout {
            EngineLayout::Inline | EngineLayout::Flat => {
                let center = (self.num_cylinders as f32 - 1.0) * 0.5;
                (i as f32 - center) * self.cylinder_spacing * VIS_SCALE
            }
            EngineLayout::V => {
                let positions = self.crank_positions();
                let center = (positions as f32 - 1.0) * 0.5;
                ((i / 2) as f32 - center) * self.cylinder_spacing * VIS_SCALE
            }
            EngineLayout::W { .. } => {
                let positions = self.num_cylinders / 4;
                let center = (positions as f32 - 1.0) * 0.5;
                ((i / 4) as f32 - center) * self.cylinder_spacing * VIS_SCALE
            }
        }
    }

    /// Bank tilt angle for a cylinder (rotation around the X/crank axis).
    /// Returns 0 for inline.  For V/Flat: +half_angle for even (bank A),
    /// -half_angle for odd (bank B).
    /// For W: 4 banks, cylinders grouped by i%4.
    #[inline]
    pub fn cyl_bank_tilt(&self, i: usize) -> f32 {
        match self.layout {
            EngineLayout::Inline => 0.0,
            EngineLayout::V | EngineLayout::Flat => {
                let half = self.bank_angle * 0.5;
                if i % 2 == 0 { half } else { -half }
            }
            EngineLayout::W { narrow_angle } => {
                let outer_half = std::f32::consts::FRAC_PI_4; // 45°
                let narrow_half = narrow_angle * 0.5;
                match i % 4 {
                    0 => outer_half + narrow_half,    // Bank A (far left)
                    1 => outer_half - narrow_half,    // Bank B (inner left)
                    2 => -(outer_half - narrow_half), // Bank C (inner right)
                    _ => -(outer_half + narrow_half), // Bank D (far right)
                }
            }
        }
    }

    /// Which bank (0..3) a cylinder belongs to.  Inline always returns 0.
    #[inline]
    pub fn cyl_bank(&self, i: usize) -> usize {
        match self.layout {
            EngineLayout::Inline => 0,
            EngineLayout::V | EngineLayout::Flat => i % 2,
            EngineLayout::W { .. } => i % 4,
        }
    }
}

// VIS_SCALE lives in geometry.rs to avoid ambiguity with glob re-exports.
use super::geometry::VIS_SCALE;

// ══════════════════════════════════════════════════════════════════════════════
// ENGINE PRESETS
// ══════════════════════════════════════════════════════════════════════════════

pub static ENGINES: LazyLock<Vec<EngineConfig>> = LazyLock::new(|| vec![
    inline4::preset(),
    inline5::preset(),
    inline6::preset(),
    v8::preset(),
    v10::preset(),
    v12::preset(),
    w16::preset(),
    flat6::preset(),
    f1_V6::preset_f1_v6(),
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
        // Bank A (even index) and Bank B (odd index) share the same physical throw phase
        phases[pair * 2] = base_phase;
        phases[pair * 2 + 1] = base_phase;
    }
    // Handle odd cylinder count (last one goes on bank A)
    if n % 2 == 1 {
        let base_phase = (pairs as f32 * spacing_deg % 360.0) * PI / 180.0;
        phases[n - 1] = base_phase;
    }
    let _ = bank_angle; // used for documentation only
    phases
}

/// Generate crank phases for a W engine with `n` cylinders (must be multiple of 4).
/// n/4 axial positions; within each group of 4 (A,B,C,D): A+B share one throw,
/// C+D share another throw PI/4 (45°) later.
pub fn generate_w_phases(n: usize) -> Vec<f32> {
    let groups = n / 4;
    let mut phases = vec![0.0_f32; n];
    for g in 0..groups {
        let base = g as f32 * PI / (groups as f32 * 0.5); // 360°/(n/4) * g in radians
        let offset = PI / 4.0; // C,D are 45° offset from A,B
        phases[g * 4 + 0] = base;          // Bank A
        phases[g * 4 + 1] = base;          // Bank B (same throw as A)
        phases[g * 4 + 2] = base + offset; // Bank C
        phases[g * 4 + 3] = base + offset; // Bank D (same throw as C)
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
        EngineLayout::W { .. } => generate_w_phases(num_cylinders),
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
        materials: MaterialsConfig::default_for_bore(bore),
    }
}
