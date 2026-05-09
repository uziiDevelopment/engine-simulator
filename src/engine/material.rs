//! Physical properties and definitions for engine materials.

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
}

pub struct ContactSurface<'a> {
    pub material_a: &'a Material,
    pub material_b: &'a Material,
}

impl<'a> ContactSurface<'a> {
    pub fn new(material_a: &'a Material, material_b: &'a Material) -> Self {
        Self { material_a, material_b }
    }

    /// Evaluates the interaction between two materials.
    /// Returns (friction_force, heat_generated, wear_a_volume, wear_b_volume)
    pub fn evaluate(
        &self,
        normal_force: f32,
        sliding_velocity: f32,
        dt: f32,
    ) -> (f32, f32, f32, f32) {
        // Effective friction coefficient is a combination of both materials.
        // A simple average or max can be used. We'll use the average.
        let mu = (self.material_a.friction_coeff + self.material_b.friction_coeff) * 0.5;
        let friction_force = mu * normal_force;
        
        let heat_generated = friction_force * sliding_velocity * dt;
        let sliding_distance = sliding_velocity * dt;

        // Archard wear equation: V = K * W * x / H
        // Where K is a wear coefficient, W is normal load, x is sliding distance, H is hardness.
        // We use a scaled wear constant for the simulation.
        let wear_constant = 1.0e-12; // Base wear scale factor
        
        let wear_a = (wear_constant * normal_force * sliding_distance) / self.material_a.hardness;
        let wear_b = (wear_constant * normal_force * sliding_distance) / self.material_b.hardness;

        (friction_force, heat_generated, wear_a, wear_b)
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
};

pub const ALUMINUM_ALLOY: Material = Material {
    name: "Aluminum Alloy",
    hardness: 90.0,
    yield_strength: 275.0e6,
    friction_coeff: 0.20, // Aluminum tends to gall and has higher friction without coatings
    thermal_conductivity: 130.0,
    specific_heat: 897.0,
    density: 2700.0,
};

pub const FORGED_STEEL: Material = Material {
    name: "Forged Steel",
    hardness: 300.0,
    yield_strength: 650.0e6,
    friction_coeff: 0.12,
    thermal_conductivity: 45.0,
    specific_heat: 490.0,
    density: 7850.0,
};

pub const STOCK_STEEL: Material = Material {
    name: "Stock Steel",
    hardness: 180.0,
    yield_strength: 350.0e6,
    friction_coeff: 0.14,
    thermal_conductivity: 45.0,
    specific_heat: 490.0,
    density: 7850.0,
};

pub const TUNGSTEN: Material = Material {
    name: "Tungsten",
    hardness: 400.0, // Extremely hard, will chew through other materials
    yield_strength: 1510.0e6,
    friction_coeff: 0.10,
    thermal_conductivity: 173.0,
    specific_heat: 134.0,
    density: 19250.0,
};

pub const TITANIUM: Material = Material {
    name: "Titanium",
    hardness: 330.0,
    yield_strength: 830.0e6,
    friction_coeff: 0.30, // High friction/galling tendency unlubricated
    thermal_conductivity: 22.0,
    specific_heat: 520.0,
    density: 4500.0,
};

pub const BRASS: Material = Material {
    name: "Brass",
    hardness: 120.0,
    yield_strength: 200.0e6,
    friction_coeff: 0.11, // Good bearing properties
    thermal_conductivity: 110.0,
    specific_heat: 380.0,
    density: 8500.0,
};

// add support for oil having flakes of damaged material in it.