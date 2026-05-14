//! Per-cylinder gas state and the single-step update that integrates it.
//!
//! Each substep performs:
//!
//!   1. Compute `V_old`, `V_new` from the slider-crank.
//!   2. Read valve lifts and convert to discharge area.
//!   3. Compressible-flow mass exchange with both manifolds.
//!   4. Spark detection (cylinder-local angle crossing the advance threshold).
//!   5. Wiebe-fraction heat release (driven by **crank angle**, not time).
//!   6. Heat loss to the cylinder walls.
//!   7. First-law energy balance:  `dU = δQ_burn − δQ_wall + h_in·dm_in − h_out·dm_out − p·dV`
//!   8. Update temperature, mass, pressure.
//!   9. Convert mid-step pressure into a piston force, then into crank torque
//!      via `τ = -F · ∂y_p/∂θ`.

use std::f32::consts::PI;

use super::config::EngineConfig;
use super::fuel::Fuel;
use super::geometry::*;
use super::manifold::Manifold;
use super::material::ContactSurface;
use super::oil::{OilConfig, OilState};
use super::thermo::*;
use super::valve::{
    exhaust_lift_for_cyl, exhaust_lift_for_cyl_cfg,
    intake_lift_for_cyl, intake_lift_for_cyl_cfg,
    valve_area, valve_area_cfg,
};

/// Gas state inside a single cylinder + per-cycle combustion bookkeeping.
#[derive(Clone, Copy, Debug)]
pub struct CylinderState {
    // Bulk gas
    pub mass:        f32,    // kg
    pub temperature: f32,    // K

    // Composition (mass fractions, sum ≈ 1)
    pub air_frac:    f32,
    pub fuel_frac:   f32,
    pub burned_frac: f32,

    // Combustion machinery
    pub burn_progress:    f32,    // 0..1, Wiebe fraction
    pub crank_at_spark:   f32,    // cyl-local 4-stroke phase (rad) when spark fired
    pub spark_armed:      bool,
    pub burning:          bool,
    pub fuel_to_burn:     f32,    // kg of fuel captured at spark

    // Telemetry & visualisation
    pub last_pressure:    f32,
    pub last_volume:      f32,
    pub flash:            f32,    // 0..1, decays after ignition for the visual
    pub intake_lift:      f32,    // m
    pub exhaust_lift:     f32,    // m
    pub last_intake_flow: f32,    // kg/s, signed (+ = into cyl)
    pub last_exhaust_flow:f32,    // kg/s, signed (+ = out of cyl)

    // ── Mechanical health (the new material-aware bits) ─────────────────────
    /// 0..1 — bore-wall material loss fraction.  1.0 = service limit reached.
    pub wall_wear:        f32,
    /// 0..1 — piston-ring material loss fraction.
    pub ring_wear:        f32,
    /// 0..1 — connecting-rod fatigue / yield damage.  1.0 = snapped.
    pub rod_damage:       f32,
    /// K — local cylinder-wall temperature (slice of block thermal mass).
    pub block_temp:       f32,
    /// K — piston crown temperature.
    pub piston_temp:      f32,
    /// Friction heat dumped this substep (W) — telemetry / oil heating.
    pub last_friction_heat: f32,
    /// Mechanical drag this substep (Nm) added to the crank.
    pub last_friction_torque: f32,
    /// Peak axial force the rod has seen (N) — telemetry.
    pub last_rod_stress:  f32,

    /// Bulk gas velocity (m/s) of the intake-runner air slug at the valve face.
    /// Positive = toward cylinder. State variable for the intake-port inertance
    /// model that produces the VE-vs-RPM curve shape (ram tuning + high-RPM rolloff).
    pub intake_slug_velocity: f32,
}

impl CylinderState {
    /// Pressure right now from current state at the supplied volume.
    #[inline]
    pub fn pressure_at(&self, volume: f32) -> f32 {
        ideal_gas_pressure(self.mass, self.temperature, volume)
    }

    #[inline]
    pub fn cv(&self) -> f32 {
        cv_mix(self.air_frac, self.fuel_frac, self.burned_frac)
    }

    #[inline]
    pub fn cp(&self) -> f32 {
        cp_mix(self.air_frac, self.fuel_frac, self.burned_frac)
    }

    /// Inert default — used by colour-sampling fallbacks for missing cylinders.
    /// Real simulation state always uses [`CylinderState::at_rest`] /
    /// [`CylinderState::at_rest_cfg`], which seed the gas state correctly.
    pub fn inert() -> Self {
        Self {
            mass: 0.0, temperature: T_ATM,
            air_frac: 1.0, fuel_frac: 0.0, burned_frac: 0.0,
            burn_progress: 0.0, crank_at_spark: 0.0,
            spark_armed: false, burning: false, fuel_to_burn: 0.0,
            last_pressure: P_ATM, last_volume: 1e-4, flash: 0.0,
            intake_lift: 0.0, exhaust_lift: 0.0,
            last_intake_flow: 0.0, last_exhaust_flow: 0.0,
            wall_wear: 0.0, ring_wear: 0.0, rod_damage: 0.0,
            block_temp: T_ATM, piston_temp: T_ATM,
            last_friction_heat: 0.0, last_friction_torque: 0.0,
            last_rod_stress: 0.0,
            intake_slug_velocity: 0.0,
        }
    }

    /// Initial state for a cylinder at crank angle 0, full of fresh air.
    pub fn at_rest(cyl_idx: usize) -> Self {
        let v0 = cyl_volume(0.0, cyl_idx);
        Self {
            mass: P_ATM * v0 / (R_AIR * T_ATM),
            temperature: T_ATM,
            air_frac: 1.0,
            fuel_frac: 0.0,
            burned_frac: 0.0,
            burn_progress: 0.0,
            crank_at_spark: 0.0,
            spark_armed: false,
            burning: false,
            fuel_to_burn: 0.0,
            last_pressure: P_ATM,
            last_volume: v0,
            flash: 0.0,
            intake_lift: 0.0,
            exhaust_lift: 0.0,
            last_intake_flow: 0.0,
            last_exhaust_flow: 0.0,
            wall_wear: 0.0,
            ring_wear: 0.0,
            rod_damage: 0.0,
            block_temp: T_ATM,
            piston_temp: T_ATM,
            last_friction_heat: 0.0,
            last_friction_torque: 0.0,
            last_rod_stress: 0.0,
            intake_slug_velocity: 0.0,
        }
    }

    /// Config-aware initial state for a cylinder at crank angle 0.
    pub fn at_rest_cfg(cfg: &EngineConfig, cyl_idx: usize) -> Self {
        let v0 = cfg.cyl_volume(0.0, cyl_idx);
        Self {
            mass: P_ATM * v0 / (R_AIR * T_ATM),
            temperature: T_ATM,
            air_frac: 1.0,
            fuel_frac: 0.0,
            burned_frac: 0.0,
            burn_progress: 0.0,
            crank_at_spark: 0.0,
            spark_armed: false,
            burning: false,
            fuel_to_burn: 0.0,
            last_pressure: P_ATM,
            last_volume: v0,
            flash: 0.0,
            intake_lift: 0.0,
            exhaust_lift: 0.0,
            last_intake_flow: 0.0,
            last_exhaust_flow: 0.0,
            wall_wear: 0.0,
            ring_wear: 0.0,
            rod_damage: 0.0,
            block_temp: T_ATM,
            piston_temp: T_ATM,
            last_friction_heat: 0.0,
            last_friction_torque: 0.0,
            last_rod_stress: 0.0,
            intake_slug_velocity: 0.0,
        }
    }
}

/// Per-substep cylinder update.  Mutates `cyl`, `intake`, `exhaust`.
/// Returns `(crank torque from this cylinder's gas force, fuel burned in step)`.
pub fn step_cylinder(
    cyl: &mut CylinderState,
    intake: &mut Manifold,
    exhaust: &mut Manifold,
    fuel: &Fuel,
    cyl_idx: usize,
    angle_old: f32,
    angle_new: f32,
    fourstroke_old: f32,
    fourstroke_new: f32,
    dt: f32,
    combustion_enabled: bool,
    throttle: f32,
) -> (f32, f32) {
    // ── Volume + valve geometry ────────────────────────────────────────────
    let v_old = cyl_volume(angle_old, cyl_idx);
    let v_new = cyl_volume(angle_new, cyl_idx);
    let dv = v_new - v_old;

    cyl.intake_lift  = intake_lift_for_cyl(cyl_idx, fourstroke_new);
    cyl.exhaust_lift = exhaust_lift_for_cyl(cyl_idx, fourstroke_new);
    let a_intake  = valve_area(cyl.intake_lift);
    let a_exhaust = valve_area(cyl.exhaust_lift);

    let p_old = cyl.pressure_at(v_old);

    // ── Compressible mass flow through valves ──────────────────────────────
    //
    //   intake:  positive `m_dot_in`  = manifold → cyl
    //   exhaust: positive `m_dot_out` = cyl → manifold
    //
    // Bug 1 fix: exhaust flow uses cylinder's actual γ (burned products have
    // lower γ ~1.28 vs air's 1.40), which changes choked-flow mass flux by ~8%.
    let gamma_cyl = gamma_mix(cyl.air_frac, cyl.fuel_frac, cyl.burned_frac);

    let m_dot_in = if a_intake > 0.0 {
        flow_between(
            intake.pressure(), intake.temperature,
            p_old, cyl.temperature,
            a_intake, GAMMA_AIR, R_AIR,
        )
    } else { 0.0 };

    let m_dot_out = if a_exhaust > 0.0 {
        flow_between(
            p_old, cyl.temperature,
            exhaust.pressure(), exhaust.temperature,
            a_exhaust, gamma_cyl, R_AIR,
        )
    } else { 0.0 };

    let dm_in  = m_dot_in  * dt;
    let dm_out = m_dot_out * dt;

    cyl.last_intake_flow  = m_dot_in;
    cyl.last_exhaust_flow = m_dot_out;

    // ── Spark detection ────────────────────────────────────────────────────
    //
    // Cyl-local 4-stroke phase ranges 0..4π.  Phase 0 = TDC compression.
    // Spark fires `spark_advance` radians *before* phase=0 — i.e. shortly
    // before the crank rolls over to 4π and wraps back to 0.
    let firing_offset = FIRING_OFFSETS_DEG[cyl_idx].to_radians();
    let phase_4s = (fourstroke_new - firing_offset).rem_euclid(4.0 * PI);

    // Re-arm spark during the exhaust stroke (~180°-360° local, so 540°-720° in
    // 0-720° terms — but our 0 is TDC compression, so 180°-360° == π..2π).
    if phase_4s > PI && phase_4s < 2.0 * PI {
        cyl.spark_armed = true;
        cyl.burning = false;
        cyl.burn_progress = 0.0;
    }

    let spark_phase = 4.0 * PI - fuel.spark_advance_deg.to_radians();
    if cyl.spark_armed && phase_4s >= spark_phase && phase_4s < 4.0 * PI {
        if combustion_enabled {
            cyl.burning = true;
            cyl.crank_at_spark = phase_4s;
            cyl.burn_progress = 0.0;
            // Capture trapped fuel mass to burn this cycle.
            cyl.fuel_to_burn = (cyl.fuel_frac * cyl.mass).max(0.0);
            cyl.flash = 1.0;
        }
        cyl.spark_armed = false;
    }

    // ── Wiebe heat release ─────────────────────────────────────────────────
    // Bug 2 fix: snapshot cv *before* updating composition so the initial
    // internal-energy calculation uses pre-combustion cv, not post-combustion cv.
    let cv_before = cyl.cv();

    let mut heat_release = 0.0_f32;
    if cyl.burning {
        let burn_dur = fuel.burn_duration_deg.to_radians();
        let mut delta = phase_4s - cyl.crank_at_spark;
        if delta < 0.0 { delta += 4.0 * PI; }
        // Bug 3 fix: use fuel-specific Wiebe shape parameters (diesel uses m=0.3
        // for sharper early peak vs. SI's m=2.0 smooth bell).
        let new_progress = wiebe(delta, burn_dur, fuel.wiebe_a, fuel.wiebe_m).min(1.0);
        let dxb = (new_progress - cyl.burn_progress).max(0.0);
        cyl.burn_progress = new_progress;

        let mass_fuel_burning = (cyl.fuel_to_burn * dxb).min(cyl.fuel_frac * cyl.mass);
        let mass_air_consumed = (mass_fuel_burning * fuel.afr_stoich).min(cyl.air_frac * cyl.mass);
        let mass_burned_produced = mass_fuel_burning + mass_air_consumed;

        if cyl.mass > 1e-9 {
            cyl.fuel_frac   = (cyl.fuel_frac   * cyl.mass - mass_fuel_burning).max(0.0) / cyl.mass;
            cyl.air_frac    = (cyl.air_frac    * cyl.mass - mass_air_consumed).max(0.0) / cyl.mass;
            cyl.burned_frac = (cyl.burned_frac * cyl.mass + mass_burned_produced).max(0.0) / cyl.mass;
        }
        heat_release = mass_fuel_burning * fuel.lhv;

        if new_progress >= 0.999 {
            cyl.burning = false;
        }
    }

    // ── Heat loss to walls ─────────────────────────────────────────────────
    //
    // Bug 4 partial fix: add bore and temperature scaling from Woschni.
    // Normalized so that at bore=86mm, T=800K the result equals the old formula.
    // Velocity term omitted here (no omega in legacy path); step_cylinder_cfg has it.
    let wall_temp = 410.0; // K (hot block)
    let p_ratio = (p_old / P_ATM).max(1.0);
    let bore_factor = (0.086_f32 / BORE).powf(0.2);
    let temp_factor = (800.0_f32 / cyl.temperature.max(200.0)).powf(0.55);
    let h_w = 130.0 * p_ratio.powf(0.8) * bore_factor * temp_factor;
    let surface_area = PI * BORE * (STROKE_TOP - piston_y(angle_new, cyl_idx)).max(0.0)
        + 2.0 * PISTON_AREA;
    let q_wall = h_w * surface_area * (cyl.temperature - wall_temp) * dt;

    // ── Energy balance ─────────────────────────────────────────────────────
    let p_mid = p_old; // forward Euler — fine at our substep size
    let work = p_mid * dv;

    // Enthalpy fluxes: incoming gas brings its source enthalpy, outgoing gas
    // leaves with the cylinder's specific enthalpy.
    let cp_cyl    = cyl.cp();
    let cp_intake = CP_AIR; // intake side is mostly air (+ a little fuel)
    let h_in_per_kg  = cp_intake * intake.temperature;
    let h_cyl_per_kg = cp_cyl * cyl.temperature;
    let h_exh_per_kg = CP_AIR * exhaust.temperature;

    // dm_in > 0 brings in manifold gas; dm_in < 0 expels cyl gas to the
    // manifold (back-flow).  Same logic for the exhaust port.
    let energy_in  = if dm_in  > 0.0 { h_in_per_kg  * dm_in  } else { h_cyl_per_kg * dm_in  };
    let energy_out = if dm_out > 0.0 { h_cyl_per_kg * dm_out } else { h_exh_per_kg * dm_out };

    // Bug 2 fix: use pre-combustion cv for initial internal energy.
    let internal_energy = cyl.mass * cv_before * cyl.temperature;
    let new_internal_energy = internal_energy + heat_release - q_wall - work + energy_in - energy_out;

    // ── Mass + composition update ──────────────────────────────────────────
    let mass_before = cyl.mass;
    let new_mass = (mass_before + dm_in - dm_out).max(1e-9);

    if dm_in > 0.0 {
        // Fresh charge entering: port-injected fuel at the fuel's target AFR
        // (richened at WOT).
        let throttle_factor = (intake.pressure() / P_ATM).clamp(0.0, 1.5);
        let enrichment = 1.0 + (fuel.power_enrichment - 1.0) * (throttle_factor - 0.4).clamp(0.0, 1.0);
        let target_afr = (fuel.afr_target / enrichment).max(0.5);
        
        // SI engines use port injection, but for consistency we still apply
        // the throttle scaling here (though it's usually 1.0 for SI as the
        // manifold pressure already handles the air/fuel reduction).
        let mass_fuel_added = (dm_in / (1.0 + target_afr)) * throttle.max(0.01);
        let mass_air_added  = dm_in - mass_fuel_added;

        let air_total    = cyl.air_frac    * mass_before + mass_air_added;
        let fuel_total   = cyl.fuel_frac   * mass_before + mass_fuel_added;
        let burned_total = cyl.burned_frac * mass_before;
        let total = (air_total + fuel_total + burned_total).max(1e-9);
        cyl.air_frac    = air_total    / total;
        cyl.fuel_frac   = fuel_total   / total;
        cyl.burned_frac = burned_total / total;
    } else if dm_out < 0.0 {
        // Exhaust back-flow: manifold gas (assumed mostly burned products)
        // enters the cylinder.
        let mass_back = -dm_out;
        let burned_total = cyl.burned_frac * mass_before + mass_back;
        let total = (mass_before + mass_back).max(1e-9);
        cyl.air_frac    = (cyl.air_frac    * mass_before / total).clamp(0.0, 1.0);
        cyl.fuel_frac   = (cyl.fuel_frac   * mass_before / total).clamp(0.0, 1.0);
        cyl.burned_frac = (burned_total / total).clamp(0.0, 1.0);
    }
    // (other directions don't change composition fractions)

    cyl.mass = new_mass;
    cyl.temperature = (new_internal_energy / (cyl.mass * cyl.cv())).clamp(180.0, 4500.0);

    // ── Manifold mass / temperature exchange (the other side of the flow) ─
    intake.mass = (intake.mass - dm_in).max(1e-9);
    exhaust.mass = (exhaust.mass + dm_out).max(1e-9);

    if dm_out > 0.0 && exhaust.mass > 0.0 {
        // Mix outgoing hot cylinder gas into the exhaust plenum.
        let weight = (dm_out / exhaust.mass).clamp(0.0, 1.0);
        exhaust.temperature = (1.0 - weight) * exhaust.temperature + weight * cyl.temperature;
    }
    if dm_in < 0.0 && intake.mass > 0.0 {
        // Back-flow heats the intake.
        let weight = (-dm_in / intake.mass).clamp(0.0, 1.0);
        intake.temperature = (1.0 - weight) * intake.temperature + weight * cyl.temperature;
    }

    // ── Pressure, force, torque ────────────────────────────────────────────
    let p_new = cyl.pressure_at(v_new);
    cyl.last_pressure = p_new;
    cyl.last_volume   = v_new;

    // Force on piston from gas above ambient.  Gas pushes piston away from
    // head (downward, decreasing y_p) when p_cyl > P_ATM.
    let p_avg = 0.5 * (p_old + p_new);
    let force_axial = (p_avg - P_ATM) * PISTON_AREA;
    let theta_mid = 0.5 * (angle_old + angle_new);
    let dy_dtheta = dpiston_dtheta(theta_mid, cyl_idx);
    let torque = -force_axial * dy_dtheta;

    // ── Decay the combustion-flash visual ──────────────────────────────────
    cyl.flash = (cyl.flash - dt * 5.5).max(0.0);

    let fuel_burned_step = if heat_release > 0.0 { heat_release / fuel.lhv.max(1.0) } else { 0.0 };
    (torque, fuel_burned_step)
}

/// Config-aware per-substep cylinder update.
/// Uses `EngineConfig` for geometry, valve timing, and firing offsets.
pub fn step_cylinder_cfg(
    cfg: &EngineConfig,
    cyl: &mut CylinderState,
    intake: &mut Manifold,
    exhaust: &mut Manifold,
    fuel: &Fuel,
    cyl_idx: usize,
    angle_old: f32,
    angle_new: f32,
    fourstroke_old: f32,
    fourstroke_new: f32,
    dt: f32,
    combustion_enabled: bool,
    omega: f32,
    throttle: f32,
) -> (f32, f32) {
    // ── Volume + valve geometry ────────────────────────────────────────────
    let v_old = cfg.cyl_volume(angle_old, cyl_idx);
    let v_new = cfg.cyl_volume(angle_new, cyl_idx);
    let dv = v_new - v_old;

    cyl.intake_lift  = intake_lift_for_cyl_cfg(cfg, cyl_idx, fourstroke_new);
    cyl.exhaust_lift = exhaust_lift_for_cyl_cfg(cfg, cyl_idx, fourstroke_new);
    let a_intake  = valve_area_cfg(cyl.intake_lift, cfg.intake_valve_diameter);
    let a_exhaust = valve_area_cfg(cyl.exhaust_lift, cfg.exhaust_valve_diameter);

    let p_old = cyl.pressure_at(v_old);

    // ── Compressible mass flow through valves ──────────────────────────────
    // Bug 1 fix: exhaust flow uses the cylinder's actual γ (burned products
    // have γ ~1.28 vs. air's 1.40, changing choked mass flux by ~8%).
    let gamma_cyl = gamma_mix(cyl.air_frac, cyl.fuel_frac, cyl.burned_frac);

    // Intake: 1-D inertial model of the runner air slug (Newton's 2nd law on
    // a gas column of length L_r and area A_r between manifold and valve face).
    //
    //   ρ·L·dv/dt = (P_man − P_cyl) − ρ·(A_r/Cd_A)²·|v|·v / 2
    //
    // ⇒ steady state matches quasi-static Bernoulli orifice flow, but the slug
    // can't change velocity instantly: that lag is the ram-tuning mechanism that
    // gives a real volumetric-efficiency curve (low-RPM ~OK, mid-RPM peak with
    // momentum overshoot, high-RPM rolloff because slug never accelerates fully).
    let p_man = intake.pressure();
    let rho_man = (p_man / (R_AIR * intake.temperature.max(60.0))).max(1.0e-3);
    let a_runner = cfg.intake_runner_area.max(1.0e-6);
    let l_runner = cfg.intake_runner_length.max(1.0e-3);

    let v_slug = cyl.intake_slug_velocity;
    if a_intake > 1.0e-7 {
        // Bernoulli loss coefficient — uses the valve's instantaneous effective area.
        // Treat the loss as a dissipation: it can't reverse the slug's direction in
        // a single substep, so clamp loss_accel to |v|/dt for forward-Euler stability
        // when the valve is at low lift (Cd·A → 0 makes the raw expression huge).
        let cd_a = a_intake;
        let loss_accel_raw = (a_runner * a_runner) * v_slug.abs() * v_slug
            / (2.0 * l_runner * cd_a * cd_a).max(1.0e-9);
        let loss_cap = v_slug.abs() / dt.max(1.0e-9);
        let loss_accel = loss_accel_raw.clamp(-loss_cap, loss_cap);
        let drive_accel = (p_man - p_old) / (rho_man * l_runner);
        let dv = drive_accel - loss_accel;
        cyl.intake_slug_velocity = v_slug + dv * dt;
    } else {
        // Valve closed: the slug bounces against the closed port, kinetic
        // energy bleeds off into the manifold via the open end.  First-order
        // decay with ~15 ms time constant captures this without a separate
        // runner-capacitance state.
        cyl.intake_slug_velocity = v_slug * (-65.0 * dt).exp();
    }

    let m_dot_in = if a_intake > 1.0e-7 {
        // Signed mass flow: + into cylinder, − backflow into manifold.
        let rho_used = if cyl.intake_slug_velocity >= 0.0 {
            rho_man
        } else {
            // Backflow carries cylinder gas back to manifold.
            (p_old / (R_AIR * cyl.temperature.max(60.0))).max(1.0e-3)
        };
        rho_used * a_runner * cyl.intake_slug_velocity
    } else { 0.0 };

    let m_dot_out = if a_exhaust > 0.0 {
        flow_between(
            p_old, cyl.temperature,
            exhaust.pressure(), exhaust.temperature,
            a_exhaust, gamma_cyl, R_AIR,
        )
    } else { 0.0 };

    let dm_in  = m_dot_in  * dt;
    let dm_out = m_dot_out * dt;

    cyl.last_intake_flow  = m_dot_in;
    cyl.last_exhaust_flow = m_dot_out;

    // ── Ignition / injection detection ────────────────────────────────────
    let firing_offset = cfg.firing_offsets_deg[cyl_idx].to_radians();
    let phase_4s = (fourstroke_new - firing_offset).rem_euclid(4.0 * PI);

    // Re-arm during the exhaust stroke (π..2π local) for both SI and CI.
    if phase_4s > PI && phase_4s < 2.0 * PI {
        cyl.spark_armed = true;
        cyl.burning = false;
        cyl.burn_progress = 0.0;
    }

    let rpm = omega.abs() * 30.0 / PI;

    if fuel.is_ci {
        // ── Compression-ignition path (Diesel) ────────────────────────────────
        //
        // Fuel is injected directly into hot compressed air near TDC.
        // Auto-ignition occurs when bulk temperature exceeds the threshold
        // (always true at operating CR ≥ 14 once the engine is warm).
        // `spark_advance_deg` is reused as injection advance in degrees BTDC.
        let injection_phase = 4.0 * PI - fuel.spark_advance_deg.to_radians();
        if cyl.spark_armed && phase_4s >= injection_phase && phase_4s < 4.0 * PI {
            // Direct injection: add fuel proportional to trapped air mass,
            // scaled by the player's throttle and an idle governor.
            let air_mass = (cyl.air_frac * cyl.mass).max(0.0);
            
            // Simple idle governor: maintain ~650 RPM by adding fuel if low.
            let idle_target = 650.0;
            let idle_error = (idle_target - rpm).max(0.0);
            let idle_fuel = (idle_error * 0.0015).min(0.12);
            let fuel_demand = (throttle + idle_fuel).clamp(0.0, 1.0);

            let fuel_to_inject = (air_mass / fuel.afr_target.max(1.0)) * fuel_demand;
            if fuel_to_inject > 1e-12 {
                let new_mass = cyl.mass + fuel_to_inject;
                cyl.air_frac    = (cyl.air_frac    * cyl.mass) / new_mass;
                cyl.fuel_frac   = (cyl.fuel_frac   * cyl.mass + fuel_to_inject) / new_mass;
                cyl.burned_frac = (cyl.burned_frac * cyl.mass) / new_mass;
                cyl.mass = new_mass;
            }
            cyl.fuel_to_burn = (cyl.fuel_frac * cyl.mass).max(0.0);
            // Auto-ignition: fires if cylinder temperature exceeds threshold.
            // At compression ratios ≥ 14:1 the bulk gas temperature at TDC
            // easily exceeds 523 K (250 °C) once cranking is underway.
            if cyl.temperature > fuel.auto_ignition_temp && combustion_enabled {
                cyl.burning = true;
                cyl.crank_at_spark = phase_4s;
                cyl.burn_progress = 0.0;
                cyl.flash = 1.0;
            }
            cyl.spark_armed = false;
        }
    } else {
        // ── Spark-ignition path (SI) ───────────────────────────────────────
        //
        // RPM-dependent spark advance: flame propagation takes roughly constant
        // time, so advance must increase with RPM.  Ramps from 8° at idle to
        // fuel.spark_advance_deg at 4000 RPM.
        let rpm_factor = ((rpm - 1000.0) / 3000.0).clamp(0.0, 1.0);
        let advance_deg = 8.0 + (fuel.spark_advance_deg - 8.0) * rpm_factor;
        let spark_phase = 4.0 * PI - advance_deg.to_radians();
        if cyl.spark_armed && phase_4s >= spark_phase && phase_4s < 4.0 * PI {
            if combustion_enabled {
                cyl.burning = true;
                cyl.crank_at_spark = phase_4s;
                cyl.burn_progress = 0.0;
                cyl.fuel_to_burn = (cyl.fuel_frac * cyl.mass).max(0.0);
                cyl.flash = 1.0;
            }
            cyl.spark_armed = false;
        }
    }

    // ── Wiebe heat release ─────────────────────────────────────────────────
    // Bug 2 fix: snapshot cv *before* updating composition so the initial
    // internal-energy uses pre-combustion cv; post-combustion cv is only used
    // to derive the final temperature (consistent with the new state).
    let cv_before = cyl.cv();

    let mut heat_release = 0.0_f32;
    if cyl.burning {
        // Burn duration stretches at low piston speed.  Turbulent flame speed
        // scales with charge motion (∝ mean piston speed); when piston speed
        // drops below the well-developed-turbulence threshold (~8 m/s), the
        // laminar floor dominates, so the burn takes more crank degrees and
        // peak heat release ends up late ATDC — which is precisely why NA
        // engines lose IMEP at low RPM and peak torque sits in the mid-range
        // rather than at idle.  Clamped at 1.8× so cranking/idle still light off.
        let mean_piston_speed = (omega.abs() * cfg.stroke) / PI;
        let burn_stretch = (8.0_f32 / mean_piston_speed.max(1.5))
            .sqrt()
            .clamp(1.0, 1.8);
        let burn_dur = (fuel.burn_duration_deg * burn_stretch).to_radians();
        let mut delta = phase_4s - cyl.crank_at_spark;
        if delta < 0.0 { delta += 4.0 * PI; }
        // Bug 3 fix: use fuel-specific Wiebe parameters (diesel: m=0.3 for
        // sharper early-peak diffusion shape; SI fuels: m=2.0 smooth bell).
        let new_progress = wiebe(delta, burn_dur, fuel.wiebe_a, fuel.wiebe_m).min(1.0);
        let dxb = (new_progress - cyl.burn_progress).max(0.0);
        cyl.burn_progress = new_progress;

        let mass_fuel_burning = (cyl.fuel_to_burn * dxb).min(cyl.fuel_frac * cyl.mass);
        let mass_air_consumed = (mass_fuel_burning * fuel.afr_stoich).min(cyl.air_frac * cyl.mass);
        let mass_burned_produced = mass_fuel_burning + mass_air_consumed;

        if cyl.mass > 1e-9 {
            cyl.fuel_frac   = (cyl.fuel_frac   * cyl.mass - mass_fuel_burning).max(0.0) / cyl.mass;
            cyl.air_frac    = (cyl.air_frac    * cyl.mass - mass_air_consumed).max(0.0) / cyl.mass;
            cyl.burned_frac = (cyl.burned_frac * cyl.mass + mass_burned_produced).max(0.0) / cyl.mass;
        }
        heat_release = mass_fuel_burning * fuel.lhv;

        if new_progress >= 0.999 {
            cyl.burning = false;
        }
    }

    // ── Heat loss to walls ─────────────────────────────────────────────────
    //
    // Bug 4 fix: proper Woschni form with bore, temperature, and piston-velocity
    // corrections.  Normalized at bore=86mm, T=800K, w=15m/s so that at those
    // reference conditions the result equals the old simpler formula.
    let wall_temp = cyl.block_temp;
    let p_ratio = (p_old / P_ATM).max(1.0);
    let bore = cfg.bore;
    let piston_area = cfg.piston_area();
    let bore_factor = (0.086_f32 / bore).powf(0.2);
    let temp_factor = (800.0_f32 / cyl.temperature.max(200.0)).powf(0.55);
    let mean_piston_speed = omega.abs() * cfg.stroke / PI;
    let w = (2.28 * mean_piston_speed).max(1.0);
    let w_factor = (w / 15.0_f32).powf(0.8);
    let h_w = 130.0 * p_ratio.powf(0.8) * bore_factor * temp_factor * w_factor;
    let surface_area = PI * bore * (cfg.stroke_top() - cfg.piston_y(angle_new, cyl_idx)).max(0.0)
        + 2.0 * piston_area;
    let q_wall = h_w * surface_area * (cyl.temperature - wall_temp) * dt;
    let block_thermal_mass =
        1.5 * cfg.materials.block.specific_heat.max(100.0); // ~1.5 kg per cyl slice
    cyl.block_temp += q_wall / block_thermal_mass;

    // ── Energy balance ─────────────────────────────────────────────────────
    let p_mid = p_old;
    let work = p_mid * dv;

    let cp_cyl    = cyl.cp();
    let cp_intake = CP_AIR;
    let h_in_per_kg  = cp_intake * intake.temperature;
    let h_cyl_per_kg = cp_cyl * cyl.temperature;
    let h_exh_per_kg = CP_AIR * exhaust.temperature;

    let energy_in  = if dm_in  > 0.0 { h_in_per_kg  * dm_in  } else { h_cyl_per_kg * dm_in  };
    let energy_out = if dm_out > 0.0 { h_cyl_per_kg * dm_out } else { h_exh_per_kg * dm_out };

    // Bug 2 fix: use pre-combustion cv for initial internal energy.
    let internal_energy = cyl.mass * cv_before * cyl.temperature;
    let new_internal_energy = internal_energy + heat_release - q_wall - work + energy_in - energy_out;

    // ── Mass + composition update ──────────────────────────────────────────
    let mass_before = cyl.mass;
    let new_mass = (mass_before + dm_in - dm_out).max(1e-9);

    if dm_in > 0.0 {
        if fuel.is_ci {
            // Diesel direct injection: intake charge is pure air only.
            // Fuel is added later at the injection event (CI path above), not here.
            let air_total    = cyl.air_frac    * mass_before + dm_in;
            let fuel_total   = cyl.fuel_frac   * mass_before;
            let burned_total = cyl.burned_frac * mass_before;
            let total = (air_total + fuel_total + burned_total).max(1e-9);
            cyl.air_frac    = air_total    / total;
            cyl.fuel_frac   = fuel_total   / total;
            cyl.burned_frac = burned_total / total;
        } else {
            // SI port injection: fuel is pre-mixed with the incoming air charge.
            let throttle_factor = (intake.pressure() / P_ATM).clamp(0.0, 1.5);
            let enrichment = 1.0 + (fuel.power_enrichment - 1.0) * (throttle_factor - 0.4).clamp(0.0, 1.0);
            let target_afr = (fuel.afr_target / enrichment).max(0.5);
            let mass_fuel_added = dm_in / (1.0 + target_afr);
            let mass_air_added  = dm_in - mass_fuel_added;

            let air_total    = cyl.air_frac    * mass_before + mass_air_added;
            let fuel_total   = cyl.fuel_frac   * mass_before + mass_fuel_added;
            let burned_total = cyl.burned_frac * mass_before;
            let total = (air_total + fuel_total + burned_total).max(1e-9);
            cyl.air_frac    = air_total    / total;
            cyl.fuel_frac   = fuel_total   / total;
            cyl.burned_frac = burned_total / total;
        }
    } else if dm_out < 0.0 {
        let mass_back = -dm_out;
        let burned_total = cyl.burned_frac * mass_before + mass_back;
        let total = (mass_before + mass_back).max(1e-9);
        cyl.air_frac    = (cyl.air_frac    * mass_before / total).clamp(0.0, 1.0);
        cyl.fuel_frac   = (cyl.fuel_frac   * mass_before / total).clamp(0.0, 1.0);
        cyl.burned_frac = (burned_total / total).clamp(0.0, 1.0);
    }

    cyl.mass = new_mass;
    cyl.temperature = (new_internal_energy / (cyl.mass * cyl.cv())).clamp(180.0, 4500.0);

    // ── Manifold mass / temperature exchange ───────────────────────────────
    intake.mass = (intake.mass - dm_in).max(1e-9);
    exhaust.mass = (exhaust.mass + dm_out).max(1e-9);

    if dm_out > 0.0 && exhaust.mass > 0.0 {
        let weight = (dm_out / exhaust.mass).clamp(0.0, 1.0);
        exhaust.temperature = (1.0 - weight) * exhaust.temperature + weight * cyl.temperature;
    }
    if dm_in < 0.0 && intake.mass > 0.0 {
        let weight = (-dm_in / intake.mass).clamp(0.0, 1.0);
        intake.temperature = (1.0 - weight) * intake.temperature + weight * cyl.temperature;
    }

    // ── Pressure, force, torque ────────────────────────────────────────────
    let p_new = cyl.pressure_at(v_new);
    cyl.last_pressure = p_new;
    cyl.last_volume   = v_new;

    let p_avg = 0.5 * (p_old + p_new);
    let force_axial = (p_avg - P_ATM) * piston_area;
    let theta_mid = 0.5 * (angle_old + angle_new);
    let dy_dtheta = cfg.dpiston_dtheta(theta_mid, cyl_idx);
    let torque = -force_axial * dy_dtheta;

    // ── Decay the combustion-flash visual ──────────────────────────────────
    cyl.flash = (cyl.flash - dt * 5.5).max(0.0);

    let fuel_burned_step = if heat_release > 0.0 { heat_release / fuel.lhv.max(1.0) } else { 0.0 };
    (torque, fuel_burned_step)
}

/// Result of a single mechanical / material substep for one cylinder.
#[derive(Clone, Copy, Debug, Default)]
pub struct MechanicalStep {
    /// Torque drag from boundary/hydrodynamic friction at the ring/wall interface (Nm, ≥ 0).
    pub friction_torque: f32,
    /// Frictional power dumped into the oil this step (W).
    pub friction_heat_w: f32,
    /// Thermal power transferred from the cylinder block into the oil this step (W).
    pub block_heat_to_oil_w: f32,
    /// Thermal power transferred from the cylinder block into the coolant jacket (W).
    pub block_heat_to_coolant_w: f32,
    /// Oil mass burned by blow-by past worn rings (kg).
    pub oil_consumed: f32,
    /// True if the cylinder friction-welded / galled to the block due to extreme heat.
    pub seized: bool,
    /// True if the rod (big-end) journal bearing has failed (spun or wiped).
    pub bearing_seized: bool,
}

fn block_thermal_mass(cfg: &EngineConfig) -> f32 {
    1.5 * cfg.materials.block.specific_heat.max(100.0)
}

fn piston_thermal_mass(cfg: &EngineConfig) -> f32 {
    let mass = cfg.materials.piston.density * cfg.piston_area() * 0.04;
    mass.max(0.1) * cfg.materials.piston.specific_heat.max(100.0)
}

/// Apply one substep of material-aware mechanics to a single cylinder.
///
/// Reads (gas state already updated by `step_cylinder_cfg`) + the active oil
/// and coolant states, writes the cylinder's wear, block / piston temperatures,
/// and rod damage.  Returns the friction drag the caller should subtract from
/// the net crankshaft torque, plus the heat to dump into the oil and coolant.
pub fn apply_mechanical_step_cfg(
    cfg: &EngineConfig,
    cyl: &mut CylinderState,
    cyl_idx: usize,
    angle_old: f32,
    angle_new: f32,
    omega: f32,
    dt: f32,
    oil: &OilState,
    oil_cfg: &OilConfig,
    wear_time_scale: f32,
    coolant_temp: f32,
    coolant_transfer_coeff: f32,
) -> MechanicalStep {
    let mats = &cfg.materials;
    let lube = oil.lubrication_factor(oil_cfg);

    // ── Slider-crank quantities at mid-step ─────────────────────────────────
    let theta = 0.5 * (angle_old + angle_new);
    let phase = cfg.crank_phases[cyl_idx];
    let crank_local = theta + phase;
    let s = crank_local.sin();
    let crank_r = cfg.crank_radius();
    let rod_l = cfg.rod_length;
    let rod_sin = (crank_r / rod_l) * s;
    let rod_cos = (1.0 - rod_sin * rod_sin).max(0.0).sqrt().max(1e-6);
    let tan_rod = rod_sin.abs() / rod_cos;

    let dy_dtheta = cfg.dpiston_dtheta(theta, cyl_idx);
    let piston_speed = (omega * dy_dtheta).abs();

    // ── Side thrust at the ring/wall interface ──────────────────────────────
    let piston_area = cfg.piston_area();
    let p_now = cyl.last_pressure;
    let force_axial_gas = (p_now - P_ATM).max(0.0) * piston_area;
    let piston_mass = (mats.piston.density * piston_area * 0.04).max(0.05);
    let piston_accel = omega * omega * crank_r * crank_local.cos();
    let force_axial_inertia = (piston_mass * piston_accel).abs();
    let force_axial = force_axial_gas + force_axial_inertia;
    // Ring spring pre-load — dominates side thrust at low gas pressure.
    // Kept modest so a healthy lubricated engine sees just a few Nm of drag.
    let ring_tension: f32 = 60.0;
    let normal_force = ring_tension + force_axial * tan_rod;

    // ── Ring vs wall contact ────────────────────────────────────────────────
    let contact_rw = ContactSurface::new(&mats.piston_ring, &mats.cylinder_wall);
    let (friction_force, heat_j, wear_ring_v, wear_wall_v) =
        contact_rw.evaluate_with_lube(normal_force, piston_speed, dt, lube);

    let friction_torque = (friction_force * dy_dtheta.abs()).max(0.0);

    // ── Wear accumulation (Archard volume → 0..1 fraction) ──────────────────
    // WEAR_NORM converts m³ of removed material into a 0..1 service-life
    // fraction.  Tuned so ~tens of seconds of fully-dry abuse (oil drained or
    // very mismatched materials at scale=1000) takes a part to red.
    const WEAR_NORM: f32 = 1.0e9;
    let scale = wear_time_scale.max(0.0);
    cyl.ring_wear = (cyl.ring_wear + wear_ring_v * WEAR_NORM * scale).clamp(0.0, 1.0);
    cyl.wall_wear = (cyl.wall_wear + wear_wall_v * WEAR_NORM * scale).clamp(0.0, 1.0);

    // ── Heat distribution between piston (ring side) and block (wall side) ──
    let (split_a, split_b) = contact_rw.heat_split();
    let q_piston = heat_j * split_a;
    let q_block_friction = heat_j * split_b;

    let block_cap = block_thermal_mass(cfg);
    let piston_cap = piston_thermal_mass(cfg);
    cyl.block_temp += q_block_friction / block_cap;
    cyl.piston_temp += q_piston / piston_cap;

    // Combustion-side heat that hits the piston crown each cycle.
    let crown_h = 320.0;
    let crown_q = crown_h * piston_area * (cyl.temperature - cyl.piston_temp).max(-200.0) * dt;
    cyl.piston_temp += crown_q / piston_cap;

    // ── Dissipation: piston → block, block → oil + ambient ──────────────────
    let pist_to_block_K = (cyl.piston_temp - cyl.block_temp) * 0.20 * dt;
    cyl.piston_temp -= pist_to_block_K;
    cyl.block_temp += pist_to_block_K * (piston_cap / block_cap);

    let oil_presence = (oil.mass / oil_cfg.capacity).clamp(0.0, 1.0);
    let block_to_oil_K = (cyl.block_temp - oil.temperature) * 0.05 * oil_presence * dt;
    cyl.block_temp -= block_to_oil_K;
    let block_heat_to_oil_w = if dt > 0.0 { (block_to_oil_K * block_cap) / dt } else { 0.0 };

    // Block → coolant water jacket (primary cooling path).
    // `coolant_transfer_coeff` is zero when coolant is drained; only extracts
    // heat if block is hotter than the coolant.
    let block_to_coolant_K =
        (cyl.block_temp - coolant_temp).max(0.0) * coolant_transfer_coeff * dt;
    cyl.block_temp -= block_to_coolant_K;
    let block_heat_to_coolant_w =
        if dt > 0.0 { (block_to_coolant_K * block_cap) / dt } else { 0.0 };

    // Small residual radiation / conduction through unwater-jacketed surfaces.
    let block_to_air_K = (cyl.block_temp - T_ATM) * 0.008 * dt;
    cyl.block_temp -= block_to_air_K;

    cyl.block_temp = cyl.block_temp.clamp(T_ATM - 5.0, 2500.0);
    cyl.piston_temp = cyl.piston_temp.clamp(T_ATM - 5.0, 2500.0);

    // ── Rod stress vs yield ─────────────────────────────────────────────────
    let rod_area = (cfg.bore * cfg.bore * 0.06).max(1.0e-4);
    let rod_force = force_axial_gas + force_axial_inertia;
    let stress = rod_force / rod_area;
    let yield_str = mats.conrod.yield_strength * (1.0 - cyl.rod_damage * 0.7).max(0.3);
    if stress > yield_str {
        let overload = (stress - yield_str) / yield_str.max(1.0);
        cyl.rod_damage = (cyl.rod_damage + overload * dt * 4.0).clamp(0.0, 1.0);
    }
    cyl.last_rod_stress = stress;

    // ── Oil consumption from blow-by past worn rings ────────────────────────
    let blow_by_factor = (cyl.ring_wear - 0.5).max(0.0) / 0.5;
    let oil_consumed = blow_by_factor * 1.0e-7 * piston_speed * dt;

    // ── Failure detection ───────────────────────────────────────────────────
    // Thermal Galling / Melting: The aluminum piston expands and friction-welds to the bore.
    let melted_piston = cyl.piston_temp > mats.piston.melting_point;
    let melted_block = cyl.block_temp > mats.block.melting_point;
    let seized = melted_piston || melted_block;

    cyl.last_friction_torque = friction_torque;
    let friction_power = friction_force * piston_speed;
    cyl.last_friction_heat = friction_power;

    MechanicalStep {
        friction_torque,
        friction_heat_w: friction_power,
        block_heat_to_oil_w,
        block_heat_to_coolant_w,
        oil_consumed,
        seized,
        bearing_seized: false, // rod bearing is stepped separately in engine.rs
    }
}
