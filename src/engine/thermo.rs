//! Thermodynamic and gas-dynamic primitives.
//!
//! All gas in the simulation is treated as a single-component ideal gas with
//! mass-weighted heat capacity.  Crude, but entirely sufficient for getting
//! realistic-looking pressure traces, manifold transients, and combustion
//! pulses out of an SI engine model.

// ── Atmosphere ──────────────────────────────────────────────────────────────
pub const P_ATM: f32 = 101_325.0;       // Pa     — sea-level pressure
pub const T_ATM: f32 = 295.0;           // K      — ~22 °C ambient
pub const T_EXH_AMBIENT: f32 = 600.0;   // K      — warm pipework idle

// ── Air properties ──────────────────────────────────────────────────────────
pub const R_AIR:    f32 = 287.05;       // J/(kg·K) specific gas const for air
pub const GAMMA_AIR: f32 = 1.40;
pub const CP_AIR:   f32 = 1005.0;       // J/(kg·K)
pub const CV_AIR:   f32 = 718.0;        // J/(kg·K)

// ── Burned-product approximation ────────────────────────────────────────────
//
// Real combustion products (CO2 + H2O + N2) have noticeably higher cv than dry
// air.  We don't model dissociation or species, just bias cv up so that the
// post-combustion temperature curve looks plausible.
pub const CV_BURNED: f32 = 950.0;
pub const CV_FUEL:   f32 = 1700.0;       // unburned fuel vapour, very rough

// γ for burned products is lower than air due to triatomic CO2/H2O molecules.
// ~1.28 at 1000 K, ~1.23 at 2000 K; we use a single representative value.
pub const GAMMA_BURNED: f32 = 1.28;

#[inline]
pub fn cv_mix(air_frac: f32, fuel_frac: f32, burned_frac: f32) -> f32 {
    (CV_AIR * air_frac + CV_FUEL * fuel_frac + CV_BURNED * burned_frac).max(700.0)
}

#[inline]
pub fn cp_mix(air_frac: f32, fuel_frac: f32, burned_frac: f32) -> f32 {
    cv_mix(air_frac, fuel_frac, burned_frac) + R_AIR
}

/// Mass-weighted γ for a mixed air/fuel/burned-products charge.
/// Used to get correct isentropic flow behaviour for exhaust-side orifice calculations.
#[inline]
pub fn gamma_mix(air_frac: f32, fuel_frac: f32, burned_frac: f32) -> f32 {
    (GAMMA_AIR * (air_frac + fuel_frac) + GAMMA_BURNED * burned_frac).clamp(1.20, 1.40)
}

// ─────────────────────────── Compressible orifice flow ──────────────────────
//
// Steady-state mass flow through a converging nozzle / valve at lift, derived
// from the standard one-dimensional isentropic relations.  Returns positive
// flow from the upstream (high-pressure, given temperature) side to downstream.
//
//   m_dot = Cd·A · p_up · √( γ / (R·T_up) ) · Φ(p_down/p_up)
//
// where Φ is the choked-flow factor (subsonic vs. choked branch).
//
// `area` is already Cd × A (effective discharge area in m²).
pub fn orifice_flow(
    p_up: f32,
    p_down: f32,
    t_up: f32,
    area: f32,
    gamma: f32,
    r_specific: f32,
) -> f32 {
    if area <= 0.0 || p_up <= 0.0 || t_up <= 1.0 || p_up <= p_down {
        return 0.0;
    }

    let pr_critical = (2.0 / (gamma + 1.0)).powf(gamma / (gamma - 1.0));
    let pr = (p_down / p_up).clamp(0.0, 1.0);

    if pr <= pr_critical {
        // Choked
        area * p_up
            * (gamma / (r_specific * t_up)).sqrt()
            * (2.0 / (gamma + 1.0)).powf((gamma + 1.0) / (2.0 * (gamma - 1.0)))
    } else {
        // Subsonic
        let term = pr.powf(2.0 / gamma) - pr.powf((gamma + 1.0) / gamma);
        if term <= 0.0 {
            0.0
        } else {
            area * p_up / (r_specific * t_up).sqrt()
                * (2.0 * gamma / (gamma - 1.0) * term).sqrt()
        }
    }
}

/// Bidirectional flow between two reservoirs.  Positive return value = flow
/// "from a to b"; negative means b→a.
pub fn flow_between(
    p_a: f32,
    t_a: f32,
    p_b: f32,
    t_b: f32,
    area: f32,
    gamma: f32,
    r_specific: f32,
) -> f32 {
    if p_a >= p_b {
        orifice_flow(p_a, p_b, t_a, area, gamma, r_specific)
    } else {
        -orifice_flow(p_b, p_a, t_b, area, gamma, r_specific)
    }
}

// ───────────────────────────── Wiebe heat release ───────────────────────────
//
// The fraction of the trapped fuel that has burned at a given crank-angle
// distance `delta` past spark is approximated by the Wiebe function:
//
//   x_b(δ) = 1 - exp( −a · ( δ/Δθ )^(m+1) )       (a = 5, m = 2 here)
//
// `delta` and `duration` are both in radians.

/// Wiebe burn-fraction function.
/// `a` is the efficiency parameter (~5 for SI), `m` is the shape exponent
/// (2.0 for smooth SI bell; 0.3 for diesel early-peak diffusion flame).
pub fn wiebe(delta: f32, duration: f32, a: f32, m: f32) -> f32 {
    if delta <= 0.0 || duration <= 0.0 { return 0.0; }
    if delta >= duration { return 1.0; }
    let x = delta / duration;
    1.0 - (-a * x.powf(m + 1.0)).exp()
}

// ─────────────────────────── Ideal-gas pressure helper ──────────────────────
#[inline]
pub fn ideal_gas_pressure(mass: f32, temperature: f32, volume: f32) -> f32 {
    mass.max(1e-9) * R_AIR * temperature / volume.max(1e-9)
}
