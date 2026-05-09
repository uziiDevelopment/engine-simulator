//! Physical properties and contact-mechanics for engine materials.
//!
//! Two parts that touch are described by a [`ContactSurface`].  Calling
//! [`ContactSurface::evaluate_with_lube`] returns the friction force, the heat
//! generated, and how much each side wore — all derived from the materials'
//! intrinsic properties (hardness, friction coefficient, …) and the lubrication
//! state at the interface.

#[derive(Clone, Debug)]
pub struct Material {
    pub name: &'static str,
    /// Brinell hardness (or similar scale). Determines which part wears out faster in a contact pair.
    pub hardness: f32,
    /// Yield strength in Pascals (N/m²). Used to calculate if a component like a rod fails under stress.
    pub yield_strength: f32,
    /// Baseline dynamic friction coefficient when this material rubs against another.
    pub friction_coeff: f32,
    /// Thermal conductivity in W/(m·K). Affects how quickly heat transfers through or into the material.
    pub thermal_conductivity: f32,
    /// Specific heat capacity in J/(kg·K). Affects how much the temperature rises for a given heat input.
    pub specific_heat: f32,
    /// Density in kg/m³. Used to calculate mass and inertia of components.
    pub density: f32,
    /// Melting / softening point (K).  When a part exceeds this, it loses
    /// strength catastrophically and the engine is considered destroyed.
    pub melting_point: f32,
}

pub struct ContactSurface<'a> {
    pub material_a: &'a Material,
    pub material_b: &'a Material,
}

impl<'a> ContactSurface<'a> {
    pub fn new(material_a: &'a Material, material_b: &'a Material) -> Self {
        Self { material_a, material_b }
    }

    /// Dry contact — no lubricant.  Equivalent to `evaluate_with_lube(.., 0.0)`.
    pub fn evaluate(
        &self,
        normal_force: f32,
        sliding_velocity: f32,
        dt: f32,
    ) -> (f32, f32, f32, f32) {
        self.evaluate_with_lube(normal_force, sliding_velocity, dt, 0.0)
    }

    /// Evaluate the interaction between two materials under a given lubrication state.
    ///
    /// `lube_factor` interpolates between two regimes:
    ///   * `0.0` — dry / boundary lubrication: full material friction, full Archard wear.
    ///   * `1.0` — full hydrodynamic film: friction collapses to viscous shear, wear → 0.
    ///
    /// Returns `(friction_force, heat_generated_J, wear_a_volume, wear_b_volume)`.
    /// `friction_force` is always non-negative; the caller decides the sign.
    pub fn evaluate_with_lube(
        &self,
        normal_force: f32,
        sliding_velocity: f32,
        dt: f32,
        lube_factor: f32,
    ) -> (f32, f32, f32, f32) {
        let lube = lube_factor.clamp(0.0, 1.0);
        let v = sliding_velocity.abs();
        let dist = v * dt;
        let n = normal_force.max(0.0);

        // Stribeck-ish blend: dry coefficient when starved, ~0 when hydroplaning.
        let mu_dry = (self.material_a.friction_coeff + self.material_b.friction_coeff) * 0.5;
        let mu_hydro = 0.005;
        let mu = mu_hydro * lube + mu_dry * (1.0 - lube);

        let friction_force = mu * n;
        let heat_generated = friction_force * v * dt;

        // Archard wear: V = K * W * x / H.  Suppressed by a healthy oil film.
        let wear_constant_base: f32 = 1.0e-12;
        let wear_constant = wear_constant_base * (1.0 - lube);
        let wear_a = (wear_constant * n * dist) / self.material_a.hardness.max(1.0);
        let wear_b = (wear_constant * n * dist) / self.material_b.hardness.max(1.0);

        (friction_force, heat_generated, wear_a, wear_b)
    }

    /// Heat-distribution weights between the two surfaces (sum = 1).
    /// More-conductive material draws away a larger share.
    pub fn heat_split(&self) -> (f32, f32) {
        let ka = self.material_a.thermal_conductivity.max(0.1);
        let kb = self.material_b.thermal_conductivity.max(0.1);
        let total = ka + kb;
        (ka / total, kb / total)
    }
}

// ── Common Material Presets ──────────────────────────────────────────────────

pub const CAST_IRON: Material = Material {
    name: "Cast Iron",
    hardness: 200.0,
    yield_strength: 250.0e6,
    friction_coeff: 0.15,
    thermal_conductivity: 50.0,
    specific_heat: 460.0,
    density: 7200.0,
    melting_point: 1500.0,
};

pub const ALUMINUM_ALLOY: Material = Material {
    name: "Aluminum Alloy",
    hardness: 90.0,
    yield_strength: 275.0e6,
    friction_coeff: 0.20, // Aluminum tends to gall and has higher friction without coatings
    thermal_conductivity: 130.0,
    specific_heat: 897.0,
    density: 2700.0,
    melting_point: 933.0,
};

pub const CAST_ALUMINUM: Material = Material {
    name: "Cast Aluminum (weak)",
    hardness: 60.0,
    yield_strength: 90.0e6, // brittle — useful for rod-snap demos
    friction_coeff: 0.22,
    thermal_conductivity: 120.0,
    specific_heat: 900.0,
    density: 2700.0,
    melting_point: 920.0,
};

pub const FORGED_STEEL: Material = Material {
    name: "Forged Steel",
    hardness: 300.0,
    yield_strength: 650.0e6,
    friction_coeff: 0.12,
    thermal_conductivity: 45.0,
    specific_heat: 490.0,
    density: 7850.0,
    melting_point: 1700.0,
};

pub const STOCK_STEEL: Material = Material {
    name: "Stock Steel",
    hardness: 180.0,
    yield_strength: 350.0e6,
    friction_coeff: 0.14,
    thermal_conductivity: 45.0,
    specific_heat: 490.0,
    density: 7850.0,
    melting_point: 1700.0,
};

pub const TUNGSTEN: Material = Material {
    name: "Tungsten",
    hardness: 400.0, // Extremely hard, will chew through other materials
    yield_strength: 1510.0e6,
    friction_coeff: 0.10,
    thermal_conductivity: 173.0,
    specific_heat: 134.0,
    density: 19250.0,
    melting_point: 3695.0,
};

pub const TITANIUM: Material = Material {
    name: "Titanium",
    hardness: 330.0,
    yield_strength: 830.0e6,
    friction_coeff: 0.30, // High friction/galling tendency unlubricated
    thermal_conductivity: 22.0,
    specific_heat: 520.0,
    density: 4500.0,
    melting_point: 1941.0,
};

pub const BRASS: Material = Material {
    name: "Brass",
    hardness: 120.0,
    yield_strength: 200.0e6,
    friction_coeff: 0.11, // Good bearing properties
    thermal_conductivity: 110.0,
    specific_heat: 380.0,
    density: 8500.0,
    melting_point: 1200.0,
};

pub const BABBIT: Material = Material {
    name: "Babbit (bearing alloy)",
    hardness: 30.0, // Soft on purpose — sacrificial bearing surface
    yield_strength: 60.0e6,
    friction_coeff: 0.06,
    thermal_conductivity: 56.0,
    specific_heat: 200.0,
    density: 7350.0,
    melting_point: 520.0,
};

/// Convenient catalogue used by UI dropdowns.
pub const MATERIAL_CATALOG: &[&Material] = &[
    &CAST_IRON,
    &ALUMINUM_ALLOY,
    &CAST_ALUMINUM,
    &FORGED_STEEL,
    &STOCK_STEEL,
    &TUNGSTEN,
    &TITANIUM,
    &BRASS,
    &BABBIT,
];
