//! Intake and exhaust plenums.
//!
//! Each manifold is a fixed-volume reservoir tracking:
//!   * total mass `m`,
//!   * temperature `T`.
//!
//! Pressure is recovered from the ideal-gas law on demand.  We don't bother
//! tracking composition in the manifolds — fresh air on the intake side, hot
//! burnt gas on the exhaust side — it's a fine simplification at this scale.

use super::config::EngineConfig;
use super::thermo::*;

#[derive(Clone, Debug)]
pub struct Manifold {
    pub volume:      f32,    // m³ (fixed)
    pub mass:        f32,    // kg
    pub temperature: f32,    // K
    /// Bulk gas-flow magnitude (kg/s) crossing this manifold's outer port,
    /// accumulated by the throttle / tailpipe systems for visualisation.
    pub flow_signal: f32,
    pub label: &'static str,
}

impl Manifold {
    #[inline]
    pub fn pressure(&self) -> f32 { ideal_gas_pressure(self.mass, self.temperature, self.volume) }
}

pub fn make_intake_manifold() -> Manifold {
    let volume = 0.0020; // 2.0 L plenum
    Manifold {
        volume,
        mass: P_ATM * volume / (R_AIR * T_ATM),
        temperature: T_ATM,
        flow_signal: 0.0,
        label: "intake",
    }
}

pub fn make_exhaust_manifold() -> Manifold {
    let volume = 0.0015; // 1.5 L
    Manifold {
        volume,
        mass: P_ATM * volume / (R_AIR * T_EXH_AMBIENT),
        temperature: T_EXH_AMBIENT,
        flow_signal: 0.0,
        label: "exhaust",
    }
}

// ───────────────────── Throttle plate (atmosphere ↔ intake) ─────────────────
pub fn throttle_flow(intake: &mut Manifold, throttle: f32, dt: f32) {
    // Throttle plate progressively un-shrouds: small idle bleed even at 0%.
    let max_area = 0.0014;                                  // m² wide-open
    let area = (0.012 + 0.988 * throttle * throttle) * max_area;

    let p_in = intake.pressure();
    let m_dot = if P_ATM >= p_in {
        orifice_flow(P_ATM, p_in, T_ATM, area, GAMMA_AIR, R_AIR)
    } else {
        -orifice_flow(p_in, P_ATM, intake.temperature, area, GAMMA_AIR, R_AIR)
    };
    let dm = m_dot * dt;
    intake.mass = (intake.mass + dm).max(1e-9);

    if dm > 0.0 {
        // Mix in cool atmospheric air (mass-weighted enthalpy mix).
        let weight = (dm / intake.mass).clamp(0.0, 1.0);
        intake.temperature = (1.0 - weight) * intake.temperature + weight * T_ATM;
    }
    intake.flow_signal = intake.flow_signal * 0.6 + dm.abs() / dt.max(1e-9) * 0.4;
}

// ───────────────────── Tailpipe (exhaust manifold → atmosphere) ─────────────
pub fn exhaust_to_atmosphere(exhaust: &mut Manifold, dt: f32) {
    let area = 0.0010; // 10 cm² tailpipe
    let p_e = exhaust.pressure();

    let m_dot = if p_e >= P_ATM {
        orifice_flow(p_e, P_ATM, exhaust.temperature, area, GAMMA_AIR, R_AIR)
    } else {
        -orifice_flow(P_ATM, p_e, T_ATM, area, GAMMA_AIR, R_AIR)
    };
    let dm = m_dot * dt;
    exhaust.mass = (exhaust.mass - dm).max(1e-9);

    // Slow Newton-cooling toward ambient pipe temperature (radiation + convection).
    let cool_rate = (1.6 * dt).min(1.0);
    exhaust.temperature += (T_EXH_AMBIENT - exhaust.temperature) * cool_rate;

    exhaust.flow_signal = exhaust.flow_signal * 0.6 + dm.abs() / dt.max(1e-9) * 0.4;
}

// ─────────────────────── Config-aware variants ───────────────────────────────

/// Throttle flow using engine config parameters.
pub fn throttle_flow_cfg(cfg: &EngineConfig, intake: &mut Manifold, throttle: f32, dt: f32) {
    let area = cfg.throttle_area(throttle);

    let p_in = intake.pressure();
    let m_dot = if P_ATM >= p_in {
        orifice_flow(P_ATM, p_in, T_ATM, area, GAMMA_AIR, R_AIR)
    } else {
        -orifice_flow(p_in, P_ATM, intake.temperature, area, GAMMA_AIR, R_AIR)
    };
    let dm = m_dot * dt;
    intake.mass = (intake.mass + dm).max(1e-9);

    if dm > 0.0 {
        let weight = (dm / intake.mass).clamp(0.0, 1.0);
        intake.temperature = (1.0 - weight) * intake.temperature + weight * T_ATM;
    }
    intake.flow_signal = intake.flow_signal * 0.6 + dm.abs() / dt.max(1e-9) * 0.4;
}

/// Exhaust to atmosphere using engine config tailpipe area.
pub fn exhaust_to_atmosphere_cfg(cfg: &EngineConfig, exhaust: &mut Manifold, dt: f32) {
    let area = cfg.tailpipe_area;
    let p_e = exhaust.pressure();

    let m_dot = if p_e >= P_ATM {
        orifice_flow(p_e, P_ATM, exhaust.temperature, area, GAMMA_AIR, R_AIR)
    } else {
        -orifice_flow(P_ATM, p_e, T_ATM, area, GAMMA_AIR, R_AIR)
    };
    let dm = m_dot * dt;
    exhaust.mass = (exhaust.mass - dm).max(1e-9);

    let cool_rate = (1.6 * dt).min(1.0);
    exhaust.temperature += (T_EXH_AMBIENT - exhaust.temperature) * cool_rate;

    exhaust.flow_signal = exhaust.flow_signal * 0.6 + dm.abs() / dt.max(1e-9) * 0.4;
}
