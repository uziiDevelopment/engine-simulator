//! Fuel definitions and presets.
//!
//! Each fuel is a plain `Copy` struct: the simulation reads its lower heating
//! value, stoichiometric AFR, target operating AFR, Wiebe burn duration, spark
//! advance, and a flame colour for the visualiser.  Switch fuels at runtime
//! via the UI dropdown.

use bevy::prelude::Color;

#[derive(Clone, Copy, Debug)]
pub struct Fuel {
    /// Display name for the UI.
    pub name: &'static str,
    /// Lower heating value (J/kg) — chemical energy released per kg of fuel.
    pub lhv: f32,
    /// Stoichiometric air-fuel ratio (mass air / mass fuel).
    pub afr_stoich: f32,
    /// Operating AFR — what we inject at idle/cruise.  Rich (< stoich) means
    /// extra fuel for cooling and power; lean (> stoich) means efficiency.
    pub afr_target: f32,
    /// Burn duration from 0% → ~100% (Wiebe), in crank degrees.
    pub burn_duration_deg: f32,
    /// Spark advance before TDC compression (deg).
    pub spark_advance_deg: f32,
    /// At wide-open throttle, multiply injected fuel by this for power
    /// enrichment (think 12.5 AFR target on race gas, 1.0 AFR on nitro).
    pub power_enrichment: f32,
    /// Flame colour during the combustion flash, linear RGB.
    pub flame_color: [f32; 3],
}

impl Fuel {
    pub fn flame(&self) -> Color {
        Color::linear_rgb(self.flame_color[0], self.flame_color[1], self.flame_color[2])
    }
}

/// All available fuels.  The runtime keeps an index into this list.
pub const FUELS: &[Fuel] = &[
    Fuel {
        name:               "Gasoline (91 RON)",
        lhv:                44_000_000.0,
        afr_stoich:         14.7,
        afr_target:         13.5,         // slight rich for power
        burn_duration_deg:  60.0,
        spark_advance_deg:  22.0,
        power_enrichment:   1.10,
        flame_color:        [1.00, 0.55, 0.18],
    },
    Fuel {
        name:               "E85 Ethanol",
        lhv:                29_500_000.0, // lower energy density / kg
        afr_stoich:          9.7,
        afr_target:          9.0,
        burn_duration_deg:  50.0,         // faster flame than gasoline
        spark_advance_deg:  26.0,
        power_enrichment:   1.15,
        flame_color:        [0.55, 0.85, 1.00],
    },
    Fuel {
        name:               "Methanol (M100)",
        lhv:                19_900_000.0,
        afr_stoich:          6.45,
        afr_target:          5.4,         // typically run very rich
        burn_duration_deg:  45.0,
        spark_advance_deg:  30.0,
        power_enrichment:   1.20,
        flame_color:        [0.55, 0.95, 1.00],
    },
    Fuel {
        name:               "Diesel #2",
        lhv:                42_800_000.0,
        afr_stoich:         14.5,
        afr_target:         22.0,         // diesel runs lean
        burn_duration_deg:  75.0,         // diffusion combustion is slower
        spark_advance_deg:   8.0,         // simulated as compression-ignition timing
        power_enrichment:   1.00,
        flame_color:        [1.00, 0.35, 0.10],
    },
    Fuel {
        name:               "Hydrogen (H₂)",
        lhv:               120_000_000.0, // huge per kg
        afr_stoich:         34.3,
        afr_target:         30.0,         // typically lean for SI H2
        burn_duration_deg:  30.0,         // very fast laminar flame
        spark_advance_deg:  12.0,
        power_enrichment:   1.00,
        flame_color:        [0.70, 0.85, 1.00],
    },
    Fuel {
        name:               "Nitromethane",
        lhv:                11_300_000.0,
        afr_stoich:          1.7,
        afr_target:          1.4,         // top-fuel runs ~1:1
        burn_duration_deg:  55.0,
        spark_advance_deg:  35.0,
        power_enrichment:   1.50,
        flame_color:        [0.85, 1.00, 0.50],
    },
];

#[inline]
pub fn fuel_count() -> usize { FUELS.len() }
