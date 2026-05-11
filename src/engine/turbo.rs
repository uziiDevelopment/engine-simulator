//! Single-turbo forced induction.
//!
//! ```text
//!   atmosphere ─► [compressor] ─► boost plenum ─► throttle ─► intake manifold
//!                      ▲
//!                      │ shaft
//!                      ▼
//!   exhaust manifold ─► [turbine + wastegate bypass] ─► atmosphere
//! ```
//!
//! All physical quantities are SI.  The compressor is modelled as a
//! pressure-source feeding a small "boost" plenum; the throttle plate then
//! bleeds boost into the intake manifold.  The turbine is modelled as an
//! orifice that extracts enthalpy from the exhaust stream and converts a
//! fraction of it (turbine efficiency) into shaft work.  The wastegate
//! routes a controlled fraction of exhaust mass flow around the turbine
//! (no work extracted) to hold boost on target.

use super::manifold::Manifold;
use super::thermo::*;

#[derive(Clone, Debug)]
pub struct TurboConfig {
    pub enabled: bool,
    /// Target gauge boost pressure (Pa above atmosphere) the wastegate aims to hold.
    pub target_boost_pa: f32,
    /// Shaft polar inertia (kg·m²).  ~5e-6 for a small automotive turbo.
    pub shaft_inertia: f32,
    /// Hard mechanical limit on shaft speed (rad/s).
    pub max_shaft_rad_s: f32,
    /// Turbine isentropic efficiency (0..1).
    pub turbine_efficiency: f32,
    /// Compressor isentropic efficiency (0..1).
    pub compressor_efficiency: f32,
    /// Effective discharge area of the turbine inlet (m²).
    pub turbine_area: f32,
    /// Effective discharge area of the wastegate when fully open (m²).
    pub wastegate_area: f32,
    /// Compressor impeller tip radius (m) — sets how fast the wheel converts
    /// shaft RPM into pressure rise (Euler turbomachinery: ΔP ∝ U²).
    pub impeller_radius: f32,
    /// Effective compressor outlet area (m²) — flow restriction from the
    /// compressor into the boost plenum.
    pub compressor_area: f32,
    /// Volume of the boost plenum / charge pipe (m³).
    pub boost_plenum_volume: f32,
    /// Intercooler effectiveness (0..1) — fraction of post-compressor charge
    /// temperature rise removed before reaching the intake.
    pub intercooler_effectiveness: f32,
    /// Boost overshoot above target (Pa) that triggers the BOV.
    pub bov_threshold_pa: f32,
    /// Number of compressor blades (for whine fundamental).
    pub blade_count: u32,
}

impl Default for TurboConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            target_boost_pa: 0.8e5, // 0.8 bar
            shaft_inertia: 5.0e-6,
            max_shaft_rad_s: 20_000.0, // ~190k RPM
            turbine_efficiency: 0.70,
            compressor_efficiency: 0.72,
            turbine_area: 0.00060,
            wastegate_area: 0.00040,
            impeller_radius: 0.030, // 60 mm wheel
            compressor_area: 0.00080,
            boost_plenum_volume: 0.0010, // 1.0 L charge pipe
            intercooler_effectiveness: 0.65,
            bov_threshold_pa: 0.30e5,
            blade_count: 11,
        }
    }
}

impl TurboConfig {
    /// Build a turbo config scaled to the engine's total displacement.
    ///
    /// The default values are tuned for a ~500 cc single cylinder (0.0005 m³).
    /// Larger engines need proportionally bigger turbos: more turbine/wastegate
    /// flow area to handle N× the exhaust, heavier rotors (more inertia), and
    /// larger charge plumbing.  Without this scaling the turbine sees far too
    /// much enthalpy from multi-cylinder exhaust, overspeeds instantly, and
    /// produces runaway boost.
    pub fn for_displacement(total_displacement_m3: f32) -> Self {
        Self::default().scaled_for_displacement(total_displacement_m3)
    }

    /// Scale this turbo configuration for a given engine displacement.
    ///
    /// This takes a bespoke base config (e.g., with custom target boost,
    /// efficiency values, etc.) and scales the physical size parameters
    /// (turbine area, wastegate area, shaft inertia, etc.) to match the
    /// engine's total displacement.
    ///
    /// Use this in engine configs to create custom turbos that are properly
    /// sized for the engine while having unique characteristics.
    ///
    /// # Example
    /// ```ignore
    /// turbo: TurboConfig {
    ///     enabled: true,
    ///     target_boost_pa: 1.2e5,  // 1.2 bar boost
    ///     turbine_efficiency: 0.75,
    ///     ..Default::default()
    /// }.scaled_for_displacement(0.0034), // 3.4L engine
    /// ```
    pub fn scaled_for_displacement(mut self, total_displacement_m3: f32) -> Self {
        let base_disp = 0.0005_f32; // 500 cc reference
        let ratio = (total_displacement_m3 / base_disp).clamp(0.25, 32.0);
        let sqrt_ratio = ratio.sqrt();

        // Turbine and wastegate both scale with √ratio to maintain proper
        // proportionality for boost control. If wastegate scales linearly
        // while turbine scales with √ratio, the wastegate becomes oversized
        // on larger engines, causing poor boost regulation and overboost.
        // Shaft inertia scales linearly to prevent overspeed.
        self.shaft_inertia = 5.0e-6 * ratio;
        self.turbine_area = 0.00060 * sqrt_ratio;
        self.wastegate_area = 0.00040 * sqrt_ratio; // Fixed: was linear, now √ratio
        self.compressor_area = 0.00080 * sqrt_ratio; // Also √ratio for consistency
        self.boost_plenum_volume = 0.0010 * ratio.powf(0.7);

        self
    }

    /// Enable this turbo configuration.
    pub fn enabled(mut self) -> Self {
        self.enabled = true;
        self
    }
}

#[derive(Clone, Debug)]
pub struct TurboState {
    pub shaft_omega: f32,           // rad/s
    pub boost: Manifold,            // post-compressor / pre-throttle plenum
    pub wastegate_open_frac: f32,   // 0..1
    pub wastegate_integral: f32,    // PI controller integral term
    pub bov_envelope: f32,          // 0..1, decays after a BOV pop
    pub last_throttle: f32,         // for BOV detection (throttle drop)
    pub last_boost: f32,            // previous boost for derivative calculation
    pub compressor_power_w: f32,    // instantaneous, for telemetry
    pub turbine_power_w: f32,       // instantaneous, for telemetry
    pub compressor_outlet_temp: f32,
}

impl TurboState {
    pub fn fresh(cfg: &TurboConfig) -> Self {
        let v = cfg.boost_plenum_volume.max(1e-5);
        Self {
            shaft_omega: 0.0,
            boost: Manifold {
                volume: v,
                mass: P_ATM * v / (R_AIR * T_ATM),
                temperature: T_ATM,
                flow_signal: 0.0,
                label: "boost",
            },
            wastegate_open_frac: 0.0,
            wastegate_integral: 0.0,
            bov_envelope: 0.0,
            last_throttle: 0.0,
            last_boost: 0.0,
            compressor_power_w: 0.0,
            turbine_power_w: 0.0,
            compressor_outlet_temp: T_ATM,
        }
    }

    #[inline]
    pub fn shaft_rpm(&self) -> f32 { self.shaft_omega * 60.0 / std::f32::consts::TAU }

    #[inline]
    pub fn boost_gauge_pa(&self) -> f32 { (self.boost.pressure() - P_ATM).max(0.0) }
}

/// Step the turbo: turbine pulls from exhaust, compressor fills boost plenum,
/// shaft integrates, wastegate PI-controls boost, BOV vents on overshoot.
///
/// Call this AFTER cylinders have updated the manifolds for this substep but
/// BEFORE `exhaust_to_atmosphere_cfg` and `throttle_flow_cfg` (which the
/// turbo-enabled paths replace).
pub fn step_turbo(
    cfg: &TurboConfig,
    state: &mut TurboState,
    intake: &Manifold,         // read-only: needed for BOV trigger
    exhaust: &mut Manifold,
    throttle: f32,
    dt: f32,
) {
    if !cfg.enabled { return; }

    // ── Wastegate PID controller with feedforward ───────────────────────
    let boost_actual = state.boost_gauge_pa();
    let err = boost_actual - cfg.target_boost_pa;

    // Derivative term: respond to rising boost quickly (prevents overshoot)
    let boost_rate = (boost_actual - state.last_boost) / dt.max(1e-6);
    state.last_boost = boost_actual;

    // Anti-windup: clamp integral.
    state.wastegate_integral += err * dt;
    state.wastegate_integral = state.wastegate_integral.clamp(-0.5e5, 0.5e5);

    // Aggressive PID gains for tight boost control
    let kp = 15.0e-6;  // Proportional: respond to current error
    let ki = 8.0e-6;   // Integral: eliminate steady-state error
    let kd = 2.0e-6;   // Derivative: respond to rate of change (reduces overshoot)

    // Feedforward: start opening wastegate at high throttle before boost hits target
    // This prevents the "spike" at full throttle when turbo is at max flow
    let throttle_ff = if throttle > 0.85 {
        // At full throttle, preemptively open wastegate based on proximity to target
        let proximity = (boost_actual / cfg.target_boost_pa).clamp(0.0, 1.5);
        0.15 * proximity  // Start opening early at ~60% of target
    } else {
        0.0
    };

    let raw = kp * err + ki * state.wastegate_integral - kd * boost_rate + throttle_ff;
    state.wastegate_open_frac = raw.clamp(0.0, 1.0);

    // ── Turbine + wastegate flow (exhaust → atmosphere) ───────────────────
    let p_e = exhaust.pressure();
    let t_e = exhaust.temperature.max(300.0);
    let gamma_e = GAMMA_BURNED;
    let cp_e = CV_BURNED + R_AIR;

    let area_turb = cfg.turbine_area * (1.0 - state.wastegate_open_frac);
    let area_wg   = cfg.wastegate_area * state.wastegate_open_frac;

    // Mass flow through each path (positive = exhaust → atmosphere).
    let m_dot_turb = if p_e > P_ATM {
        orifice_flow(p_e, P_ATM, t_e, area_turb, gamma_e, R_AIR)
    } else { 0.0 };
    let m_dot_wg = if p_e > P_ATM {
        orifice_flow(p_e, P_ATM, t_e, area_wg, gamma_e, R_AIR)
    } else { 0.0 };
    let m_dot_total = m_dot_turb + m_dot_wg;

    // Remove mass from exhaust plenum.
    exhaust.mass = (exhaust.mass - m_dot_total * dt).max(1e-9);
    // Newton-cool toward ambient pipe temperature.
    let cool_rate = (1.6 * dt).min(1.0);
    exhaust.temperature += (T_EXH_AMBIENT - exhaust.temperature) * cool_rate;
    exhaust.flow_signal = exhaust.flow_signal * 0.6 + m_dot_total * 0.4;

    // Turbine isentropic enthalpy drop.
    let pr_t = (P_ATM / p_e.max(1e-3)).clamp(0.0, 1.0);
    let isen_factor = 1.0 - pr_t.powf((gamma_e - 1.0) / gamma_e);
    let p_turb = (m_dot_turb * cp_e * t_e * isen_factor * cfg.turbine_efficiency).max(0.0);
    state.turbine_power_w = p_turb;

    // ── Compressor flow (atmosphere → boost plenum) ───────────────────────
    // Pressure rise from a centrifugal impeller, simple Euler relation:
    //   ΔP_ideal = ρ * U²  with U = ω * r.
    // We treat that as the compressor's "no-flow head" pressure source;
    // actual pressure ratio in the plenum settles based on outflow demand.
    let u = state.shaft_omega * cfg.impeller_radius;
    let rho_in = P_ATM / (R_AIR * T_ATM);
    let dp_head = rho_in * u * u; // Pa
    let p_source = P_ATM + dp_head * cfg.compressor_efficiency;
    let p_boost = state.boost.pressure();

    // Mass flow from compressor source into plenum (or back-flow if surge).
    let m_dot_comp = if p_source > p_boost {
        orifice_flow(p_source, p_boost, T_ATM, cfg.compressor_area, GAMMA_AIR, R_AIR)
    } else if p_boost > p_source {
        // Compressor stalled / wheel slow — small surge backflow.
        -orifice_flow(p_boost, p_source, state.boost.temperature,
                      cfg.compressor_area * 0.3, GAMMA_AIR, R_AIR)
    } else { 0.0 };

    // Compressor outlet temperature: isentropic compression from ambient.
    let pr_c = (p_source / P_ATM).max(1.0);
    let isen_rise = pr_c.powf((GAMMA_AIR - 1.0) / GAMMA_AIR) - 1.0;
    let t_outlet_isen = T_ATM * (1.0 + isen_rise / cfg.compressor_efficiency.max(0.05));
    // Intercooler removes a fraction of the rise.
    let t_after_ic = t_outlet_isen
        - (t_outlet_isen - T_ATM) * cfg.intercooler_effectiveness.clamp(0.0, 1.0);
    state.compressor_outlet_temp = t_after_ic;

    let dm_b = m_dot_comp * dt;
    state.boost.mass = (state.boost.mass + dm_b).max(1e-9);
    if dm_b > 0.0 {
        let weight = (dm_b / state.boost.mass).clamp(0.0, 1.0);
        state.boost.temperature =
            (1.0 - weight) * state.boost.temperature + weight * t_after_ic;
    }

    // Compressor power: enthalpy rise of the air pumped.
    let p_comp = (m_dot_comp.max(0.0) * CP_AIR
        * (t_outlet_isen - T_ATM).max(0.0)) / cfg.compressor_efficiency.max(0.05);
    state.compressor_power_w = p_comp;

    // ── Shaft torque integration ──────────────────────────────────────────
    let omega_safe = state.shaft_omega.max(50.0); // avoid div-by-zero at standstill
    let tau_turb = p_turb / omega_safe;
    let tau_comp = p_comp / omega_safe;
    // Bearing drag: viscous + windage.
    let tau_drag = 5.0e-7 * state.shaft_omega
                 + 1.0e-10 * state.shaft_omega * state.shaft_omega;
    let net_tau = tau_turb - tau_comp - tau_drag;
    state.shaft_omega += net_tau / cfg.shaft_inertia.max(1e-8) * dt;
    state.shaft_omega = state.shaft_omega.clamp(0.0, cfg.max_shaft_rad_s);

    state.boost.flow_signal =
        state.boost.flow_signal * 0.6 + m_dot_comp.abs() * 0.4;

    // ── BOV detection (sudden throttle close while spooled) ───────────────
    // Trigger if throttle dropped sharply AND boost > threshold AND intake
    // pressure is well below boost (closed throttle traps boost upstream).
    let throttle_drop = (state.last_throttle - throttle).max(0.0);
    let intake_p = intake.pressure();
    let trigger = throttle_drop > 0.30
        && boost_actual > cfg.bov_threshold_pa
        && (state.boost.pressure() - intake_p) > 0.20e5;
    if trigger {
        state.bov_envelope = 1.0;
    }
    if state.bov_envelope > 0.0 {
        // Vent boost plenum to atmosphere over ~150 ms.
        let vent_area = 0.0008 * state.bov_envelope;
        let m_dot_bov = orifice_flow(
            state.boost.pressure(), P_ATM, state.boost.temperature,
            vent_area, GAMMA_AIR, R_AIR,
        );
        state.boost.mass = (state.boost.mass - m_dot_bov * dt).max(1e-9);
        // Decay envelope (~exp time-constant ≈ 0.15 s).
        state.bov_envelope -= dt / 0.15;
        if state.bov_envelope < 0.0 { state.bov_envelope = 0.0; }
    }
    state.last_throttle = throttle;
}

/// Throttle plate variant that pulls from the boost plenum (turbocharged path).
/// Mirrors `manifold::throttle_flow_cfg` but with `boost` as upstream instead
/// of atmosphere.
pub fn throttle_flow_boosted(
    throttle_area: f32,
    boost: &mut Manifold,
    intake: &mut Manifold,
    throttle: f32,
    dt: f32,
) {
    let area = throttle_area;
    let p_boost = boost.pressure();
    let p_in = intake.pressure();
    let t_boost = boost.temperature;

    let m_dot = if p_boost >= p_in {
        orifice_flow(p_boost, p_in, t_boost, area, GAMMA_AIR, R_AIR)
    } else {
        -orifice_flow(p_in, p_boost, intake.temperature, area, GAMMA_AIR, R_AIR)
    };
    let dm = m_dot * dt;
    intake.mass = (intake.mass + dm).max(1e-9);
    boost.mass  = (boost.mass  - dm).max(1e-9);

    if dm > 0.0 {
        let weight = (dm / intake.mass).clamp(0.0, 1.0);
        intake.temperature = (1.0 - weight) * intake.temperature + weight * t_boost;
    }
    intake.flow_signal = intake.flow_signal * 0.6 + dm.abs() / dt.max(1e-9) * 0.4;
    let _ = throttle; // (area already includes throttle position)
}
