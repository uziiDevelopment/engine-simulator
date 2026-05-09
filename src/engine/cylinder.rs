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
            a_exhaust, GAMMA_AIR, R_AIR,
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
    let mut heat_release = 0.0_f32;
    if cyl.burning {
        let burn_dur = fuel.burn_duration_deg.to_radians();
        let mut delta = phase_4s - cyl.crank_at_spark;
        if delta < 0.0 { delta += 4.0 * PI; }
        let new_progress = wiebe(delta, burn_dur).min(1.0);
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
    // Quick-and-dirty Woschni-style coefficient.  Real Woschni uses speed and
    // pressure; we just scale with surface area, ΔT and fix a coefficient.
    let wall_temp = 410.0; // K (hot block)
    let h_w = 480.0;       // W/m²K, average
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

    let internal_energy = cyl.mass * cyl.cv() * cyl.temperature;
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
        let mass_fuel_added = dm_in / (1.0 + target_afr);
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
            a_exhaust, GAMMA_AIR, R_AIR,
        )
    } else { 0.0 };

    let dm_in  = m_dot_in  * dt;
    let dm_out = m_dot_out * dt;

    cyl.last_intake_flow  = m_dot_in;
    cyl.last_exhaust_flow = m_dot_out;

    // ── Spark detection ────────────────────────────────────────────────────
    let firing_offset = cfg.firing_offsets_deg[cyl_idx].to_radians();
    let phase_4s = (fourstroke_new - firing_offset).rem_euclid(4.0 * PI);

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
            cyl.fuel_to_burn = (cyl.fuel_frac * cyl.mass).max(0.0);
            cyl.flash = 1.0;
        }
        cyl.spark_armed = false;
    }

    // ── Wiebe heat release ─────────────────────────────────────────────────
    let mut heat_release = 0.0_f32;
    if cyl.burning {
        let burn_dur = fuel.burn_duration_deg.to_radians();
        let mut delta = phase_4s - cyl.crank_at_spark;
        if delta < 0.0 { delta += 4.0 * PI; }
        let new_progress = wiebe(delta, burn_dur).min(1.0);
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
    let wall_temp = 410.0;
    let h_w = 480.0;
    let bore = cfg.bore;
    let piston_area = cfg.piston_area();
    let surface_area = PI * bore * (cfg.stroke_top() - cfg.piston_y(angle_new, cyl_idx)).max(0.0)
        + 2.0 * piston_area;
    let q_wall = h_w * surface_area * (cyl.temperature - wall_temp) * dt;

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

    let internal_energy = cyl.mass * cyl.cv() * cyl.temperature;
    let new_internal_energy = internal_energy + heat_release - q_wall - work + energy_in - energy_out;

    // ── Mass + composition update ──────────────────────────────────────────
    let mass_before = cyl.mass;
    let new_mass = (mass_before + dm_in - dm_out).max(1e-9);

    if dm_in > 0.0 {
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
