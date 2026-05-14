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
    /// For SI engines: spark advance before TDC compression (deg).
    /// For CI (diesel): injection advance before TDC compression (deg).
    pub spark_advance_deg: f32,
    /// At wide-open throttle, multiply injected fuel by this for power
    /// enrichment (think 12.5 AFR target on race gas, 1.0 AFR on nitro).
    /// Unused for CI engines (injection quantity is set by afr_target directly).
    pub power_enrichment: f32,
    /// Wiebe efficiency parameter `a` (controls end-of-combustion completeness).
    /// 5.0 is standard for SI engines.
    pub wiebe_a: f32,
    /// Wiebe shape exponent `m`.  SI engines use ~2.0 (smooth bell-shaped rate).
    /// Diesel uses ~0.4 (sharp early peak matching premixed + diffusion character).
    pub wiebe_m: f32,
    /// Flame colour during the combustion flash, linear RGB.
    pub flame_color: [f32; 3],
    /// True for compression-ignition engines (diesel).  When set, the cylinder
    /// model takes pure-air intake (no port injection), injects fuel directly
    /// at `spark_advance_deg` BTDC, and auto-ignites once temperature exceeds
    /// `auto_ignition_temp` — no spark event required.
    pub is_ci: bool,
    /// Minimum bulk in-cylinder temperature (K) required for auto-ignition.
    /// Diesel: ~523 K (250 °C).  SI fuels set this high so they never CI.
    pub auto_ignition_temp: f32,
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
        wiebe_a:            5.0,
        wiebe_m:            2.0,
        flame_color:        [1.00, 0.55, 0.18],
        is_ci:              false,
        auto_ignition_temp: 700.0,        // high — SI gasoline won't self-ignite
    },
    Fuel {
        name:               "E85 Ethanol",
        lhv:                29_500_000.0, // lower energy density / kg
        afr_stoich:          9.7,
        afr_target:          9.0,
        burn_duration_deg:  50.0,         // faster flame than gasoline
        spark_advance_deg:  26.0,
        power_enrichment:   1.15,
        wiebe_a:            5.0,
        wiebe_m:            2.0,
        flame_color:        [0.55, 0.85, 1.00],
        is_ci:              false,
        auto_ignition_temp: 700.0,
    },
    Fuel {
        name:               "Methanol (M100)",
        lhv:                19_900_000.0,
        afr_stoich:          6.45,
        afr_target:          5.4,         // typically run very rich
        burn_duration_deg:  45.0,
        spark_advance_deg:  30.0,
        power_enrichment:   1.20,
        wiebe_a:            5.0,
        wiebe_m:            2.0,
        flame_color:        [0.55, 0.95, 1.00],
        is_ci:              false,
        auto_ignition_temp: 700.0,
    },
    Fuel {
        // Compression-ignition diesel.  Key differences from SI fuels:
        //
        //   • is_ci = true  → cylinder model uses the CI path: pure-air intake,
        //     direct fuel injection at `spark_advance_deg` BTDC, auto-ignition
        //     when T > auto_ignition_temp (always met at CR≥14 when running).
        //
        //   • afr_target = 22  → typical full-load diesel lambda ~1.5.
        //     Idle is naturally leaner because trapped air mass is small
        //     (manifold pressure low, no boost yet).
        //
        //   • burn_duration_deg = 45  → combined premixed + diffusion phase at
        //     full load.  The burn_stretch factor in the cylinder code lengthens
        //     this at low RPM, matching the real CI combustion centroid shift.
        //
        //   • wiebe_m = 0.4  → sharper, earlier-peaking heat-release rate that
        //     represents the dominant premixed ignition phase of CI combustion
        //     (vs. SI's smooth m=2.0 bell curve).
        //
        //   • spark_advance_deg = 5  → start of injection 5° BTDC — the
        //     typical injection advance for a common-rail diesel at cruise load.
        name:               "Diesel #2",
        lhv:                42_800_000.0,
        afr_stoich:         14.5,
        afr_target:         22.0,
        burn_duration_deg:  45.0,
        spark_advance_deg:   5.0,
        power_enrichment:   1.00,         // unused for CI; injection qty from afr_target
        wiebe_a:            6.5,          // slightly higher completeness vs SI default 5.0
        wiebe_m:            0.4,
        flame_color:        [1.00, 0.30, 0.05], // hotter orange than SI
        is_ci:              true,
        auto_ignition_temp: 523.0,        // 250 °C — diesel auto-ignition temperature
    },
    Fuel {
        name:               "Hydrogen (H₂)",
        lhv:               120_000_000.0, // huge per kg
        afr_stoich:         34.3,
        afr_target:         30.0,         // typically lean for SI H2
        burn_duration_deg:  30.0,         // very fast laminar flame
        spark_advance_deg:  12.0,
        power_enrichment:   1.00,
        wiebe_a:            5.0,
        wiebe_m:            2.0,
        flame_color:        [0.70, 0.85, 1.00],
        is_ci:              false,
        auto_ignition_temp: 700.0,
    },
    Fuel {
        name:               "Nitromethane",
        lhv:                11_300_000.0,
        afr_stoich:          1.7,
        afr_target:          1.4,         // top-fuel runs ~1:1
        burn_duration_deg:  55.0,
        spark_advance_deg:  35.0,
        power_enrichment:   1.50,
        wiebe_a:            5.0,
        wiebe_m:            2.0,
        flame_color:        [0.85, 1.00, 0.50],
        is_ci:              false,
        auto_ignition_temp: 700.0,
    },
];

#[inline]
pub fn fuel_count() -> usize { FUELS.len() }
