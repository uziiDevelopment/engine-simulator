//! Coolant loop: water jacket, thermostat, radiator, cooling fan.
//!
//! Heat path:
//!
//!   combustion gas → cylinder wall (`block_temp`)
//!                          ↓  block_transfer_coeff × (T_block − T_coolant)
//!                    coolant jacket  ←────────────────────────────────────
//!                          ↓  thermostat + pump flow
//!                     radiator core → ambient air
//!
//! The thermostat is a slow actuator (~2 s time constant) that gates radiator
//! flow.  An electric fan adds dissipation above its on-temperature and is
//! controlled by hysteresis.  If coolant boils the pressurised cap vents and
//! the head gasket fails — a permanent seizure.

use std::f32::consts::TAU;
use super::thermo::T_ATM;

/// Tunable parameters of the coolant circuit.
#[derive(Clone, Debug)]
pub struct CoolantConfig {
    /// Coolant fill mass (kg).  Typical 50/50 water-glycol: ~5 kg.
    pub capacity_kg: f32,
    /// Specific heat of the coolant mix (J/kg·K).  ~3500 for 50/50 glycol.
    pub specific_heat: f32,
    /// Passive radiator core dissipation coefficient (W/K).
    pub radiator_dissipation: f32,
    /// Additional dissipation when the electric fan runs (W/K).
    pub fan_dissipation: f32,
    /// Coolant temperature at which the thermostat begins to open (K).
    pub thermostat_open_k: f32,
    /// Coolant temperature at which the thermostat is fully open (K).
    pub thermostat_full_k: f32,
    /// Coolant temperature that switches the electric fan on (K).
    pub fan_on_k: f32,
    /// Coolant temperature that switches the electric fan off (K).
    pub fan_off_k: f32,
    /// Coolant temperature at which the pressurised cap vents / head gasket
    /// fails (K).  Triggers permanent engine seizure.
    pub boilover_k: f32,
    /// Normalised block-to-coolant heat-transfer coefficient (s⁻¹).
    /// Each substep: ΔT_block = coeff × (T_block − T_coolant) × dt.
    pub block_transfer_coeff: f32,
}

impl Default for CoolantConfig {
    fn default() -> Self {
        Self {
            capacity_kg:          5.0,
            specific_heat:        3_500.0,
            radiator_dissipation: 1_200.0, // W/K — decent crossflow radiator
            fan_dissipation:        600.0, // W/K — electric pusher fan
            thermostat_open_k:      355.0, // 82 °C
            thermostat_full_k:      368.0, // 95 °C
            fan_on_k:               373.0, // 100 °C
            fan_off_k:              363.0, // 90 °C
            boilover_k:             403.0, // 130 °C — pressurised cap limit
            block_transfer_coeff:   0.35,
        }
    }
}

/// Live coolant state.
#[derive(Clone, Debug)]
pub struct CoolantState {
    /// Bulk coolant temperature (K).
    pub temperature: f32,
    /// Thermostat valve opening fraction (0 = closed, 1 = fully open).
    pub thermostat_fraction: f32,
    /// True when the thermostat-controlled electric fan is running.
    pub fan_active: bool,
    /// kg of coolant remaining.  Draining it collapses the jacket transfer.
    pub mass_kg: f32,
}

impl CoolantState {
    pub fn fresh(cfg: &CoolantConfig) -> Self {
        Self {
            temperature: T_ATM,
            thermostat_fraction: 0.0,
            fan_active: false,
            mass_kg: cfg.capacity_kg,
        }
    }

    /// Normalised s⁻¹ coefficient for block → jacket heat transfer this substep.
    ///
    /// Returns zero when the coolant has been drained.  Scales with pump flow
    /// (proportional to engine speed) and the configured transfer coefficient.
    pub fn block_transfer_factor(&self, cfg: &CoolantConfig, omega: f32) -> f32 {
        if self.mass_kg < 0.1 {
            return 0.0;
        }
        // Pump flow scales with RPM.  Small thermosyphon effect keeps a residual
        // even at zero RPM so the jacket doesn't act like a perfect insulator.
        let pump_factor = (omega.abs() / (800.0 * TAU / 60.0)).clamp(0.05, 1.5);
        cfg.block_transfer_coeff * pump_factor
    }

    /// Advance the coolant reservoir one substep.
    ///
    /// `heat_from_blocks_w` — total power flowing into the coolant from all
    /// cylinder block slices this substep (W).
    pub fn step(
        &mut self,
        cfg: &CoolantConfig,
        omega: f32,
        heat_from_blocks_w: f32,
        dt: f32,
    ) {
        // ── Thermostat (slow, ~2 s time constant to open/close) ────────────
        let target = ((self.temperature - cfg.thermostat_open_k)
            / (cfg.thermostat_full_k - cfg.thermostat_open_k)).clamp(0.0, 1.0);
        self.thermostat_fraction += (target - self.thermostat_fraction) * (dt / 2.0).min(1.0);

        // ── Electric fan hysteresis ─────────────────────────────────────────
        if self.temperature >= cfg.fan_on_k  { self.fan_active = true;  }
        if self.temperature <= cfg.fan_off_k { self.fan_active = false; }

        // ── Radiator heat rejection ─────────────────────────────────────────
        // Coolant only flows through the radiator when the thermostat is open.
        // Pump speed also affects heat exchange (slower pump → less convection).
        let pump_factor = (omega.abs() / (800.0 * TAU / 60.0)).clamp(0.05, 1.5);
        let rad_flow = self.thermostat_fraction * pump_factor.sqrt();
        let mut rad_coeff = cfg.radiator_dissipation * rad_flow;
        if self.fan_active { rad_coeff += cfg.fan_dissipation; }
        let q_rad = rad_coeff * (self.temperature - T_ATM).max(0.0);

        // ── Energy balance ──────────────────────────────────────────────────
        let mass_eff = self.mass_kg.max(0.1);
        let d_temp = (heat_from_blocks_w - q_rad) * dt / (mass_eff * cfg.specific_heat);
        self.temperature = (self.temperature + d_temp).clamp(T_ATM - 5.0, 450.0);
    }

    /// 0..1 overheating severity.
    ///
    /// Zero at normal operating temperature; 1.0 at the threshold just before
    /// boilover.  Used to ramp up friction drag and derate combustion power.
    pub fn overheat_factor(&self, cfg: &CoolantConfig) -> f32 {
        let warn_k = cfg.thermostat_full_k + 10.0; // e.g., 105 °C
        let crit_k  = cfg.boilover_k - 5.0;        // 5 K before cap vents
        ((self.temperature - warn_k) / (crit_k - warn_k)).clamp(0.0, 1.0)
    }

    /// Remove all coolant (simulates a ruptured hose).
    pub fn drain(&mut self) {
        self.mass_kg = 0.0;
    }

    /// Refill to capacity and reset to ambient temperature.
    pub fn refill(&mut self, cfg: &CoolantConfig) {
        self.mass_kg = cfg.capacity_kg;
        self.temperature = T_ATM;
        self.thermostat_fraction = 0.0;
        self.fan_active = false;
    }
}
