//! Per-frame animation systems.  Read [`EngineCore`], write transforms +
//! material handles.  No simulation logic here.

use bevy::prelude::*;

use crate::engine::{
    EngineCore, P_ATM, VIS_SCALE,
};

use super::{
    CombustionFlash, ConRod, Crankshaft, CylinderGasViz, ManifoldKind, ManifoldViz, Piston,
    Valve, ValveKind,
};

// ─────────────────────────────────── Crank ──────────────────────────────────
pub fn animate_crank(core: Res<EngineCore>, mut q: Query<&mut Transform, With<Crankshaft>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_x(core.angle);
    }
}

// ─────────────────────────────────── Pistons ────────────────────────────────
pub fn animate_pistons(core: Res<EngineCore>, mut q: Query<(&Piston, &mut Transform)>) {
    for (p, mut t) in &mut q {
        if p.idx >= core.config.num_cylinders { continue; }
        let y_p = core.config.piston_y(core.angle, p.idx) * VIS_SCALE;
        let x = core.config.cyl_visual_x(p.idx);
        let tilt = p.bank_tilt;
        let pos = tilt_vec(x, y_p, 0.0, tilt);
        t.translation = pos;
        t.rotation = Quat::from_rotation_x(tilt);
    }
}

// ─────────────────────────────────── Conrods ────────────────────────────────
pub fn animate_rods(core: Res<EngineCore>, mut q: Query<(&ConRod, &mut Transform)>) {
    let r = core.config.crank_radius() * VIS_SCALE;
    for (rod, mut t) in &mut q {
        if rod.idx >= core.config.num_cylinders { continue; }
        let pin_phase = core.config.crank_phases[rod.idx];
        let world_theta = core.angle + pin_phase;
        // Crank pin position (in world space, crank rotates in Y-Z plane)
        let pin = Vec3::new(rod.base_x, r * world_theta.cos(), r * world_theta.sin());
        // Piston wrist-pin position (along the tilted bank axis)
        let y_p = core.config.piston_y(core.angle, rod.idx) * VIS_SCALE;
        let tilt = rod.bank_tilt;
        let small = tilt_vec(rod.base_x, y_p, 0.0, tilt);
        let mid = (pin + small) * 0.5;
        let dir = (small - pin).normalize_or_zero();
        t.translation = mid;
        t.rotation = Quat::from_rotation_arc(Vec3::Y, dir);
    }
}

// ─────────────────────────────────── Valves ─────────────────────────────────
pub fn animate_valves(core: Res<EngineCore>, mut q: Query<(&Valve, &mut Transform)>) {
    for (v, mut t) in &mut q {
        if v.cyl >= core.cylinders.len() { continue; }
        let lift_m = match v.kind {
            ValveKind::Intake  => core.cylinders[v.cyl].intake_lift,
            ValveKind::Exhaust => core.cylinders[v.cyl].exhaust_lift,
        };
        // Valve heads pull into the cylinder (along bank axis) when they open.
        let delta = lift_m * VIS_SCALE * 1.5;
        let tilt = v.bank_tilt;
        // Move seat_y down along the tilted axis
        t.translation.y = (v.seat_y - delta) * tilt.cos();
        t.translation.z = (v.seat_y - delta) * tilt.sin();
    }
}

/// Compute world position from local (x, y_along_axis, z_lateral) and bank tilt.
#[inline]
fn tilt_vec(x: f32, y_local: f32, z_local: f32, tilt: f32) -> Vec3 {
    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    Vec3::new(
        x,
        y_local * cos_t - z_local * sin_t,
        y_local * sin_t + z_local * cos_t,
    )
}

// ──────────────── Cylinder bore tint = pressure  +  composition ─────────────
pub fn animate_cylinder_gas(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&CylinderGasViz>,
) {
    for viz in &q {
        if viz.idx >= core.cylinders.len() { continue; }
        let cyl = &core.cylinders[viz.idx];
        let v = core.config.cyl_volume(core.angle, viz.idx);
        let p = cyl.pressure_at(v);

        // Pressure ratio (1× ambient → 50×).  Maps to a blue→cyan→yellow→red
        // gradient.  A tiny emissive boost makes high-pressure pulses pop.
        let pr = (p / P_ATM).clamp(0.5, 60.0);
        let t01 = ((pr.ln() / 60.0_f32.ln()).clamp(0.0, 1.0)).powf(0.7);

        let (r, g, b) = pressure_gradient(t01);
        let burned_tint = cyl.burned_frac.clamp(0.0, 1.0) * 0.4;
        let r_final = r * (1.0 - burned_tint) + 0.20 * burned_tint;
        let g_final = g * (1.0 - burned_tint) + 0.15 * burned_tint;
        let b_final = b * (1.0 - burned_tint) + 0.18 * burned_tint;

        // Alpha ramps with pressure so the bore reads as filled with denser
        // gas during compression / combustion.
        let alpha = (0.10 + 0.55 * t01).min(0.85);
        let emissive_strength = (cyl.flash * 1.6 + (t01 - 0.55).max(0.0) * 0.5).clamp(0.0, 2.5);

        if let Some(mat) = materials.get_mut(&viz.bore_material) {
            mat.base_color = Color::srgba(r_final, g_final, b_final, alpha);
            // Tint emissive with the fuel's flame colour during the flash.
            let flame = core.fuel.flame_color;
            mat.emissive = LinearRgba::new(
                emissive_strength * (flame[0] * 0.6 + 0.05),
                emissive_strength * (flame[1] * 0.5 + 0.05),
                emissive_strength * (flame[2] * 0.4 + 0.05),
                1.0,
            );
        }
    }
}

// ──────────────── Combustion-flash sphere at top of each bore ───────────────
pub fn animate_combustion_flash(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&CombustionFlash>,
) {
    for flash in &q {
        if flash.cyl >= core.cylinders.len() { continue; }
        let cyl = &core.cylinders[flash.cyl];
        let intensity = cyl.flash.clamp(0.0, 1.0).powf(0.6);

        if let Some(mat) = materials.get_mut(&flash.material) {
            let flame = core.fuel.flame_color;
            mat.base_color = Color::srgba(flame[0], flame[1], flame[2], intensity * 0.85);
            mat.emissive = LinearRgba::new(
                flame[0] * intensity * 6.0,
                flame[1] * intensity * 6.0,
                flame[2] * intensity * 6.0,
                1.0,
            );
        }
    }
}

// ──────────────── Manifold colour ↔ pressure / temperature ──────────────────
pub fn animate_manifolds(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&ManifoldViz>,
) {
    for vis in &q {
        let Some(mat) = materials.get_mut(&vis.material) else { continue };
        match vis.kind {
            ManifoldKind::Intake => {
                // Manifold-air pressure: deep vacuum (closed throttle, low MAP)
                // reads as a darker pipe; full atmospheric/boost reads brighter.
                let pr = (core.intake.pressure() / P_ATM).clamp(0.2, 1.5);
                let brightness = (pr - 0.2) / 1.3;
                mat.base_color = Color::srgb(
                    0.10 + 0.10 * brightness,
                    0.30 + 0.40 * brightness,
                    0.55 + 0.40 * brightness,
                );
                mat.emissive = LinearRgba::new(
                    0.02 + 0.05 * brightness,
                    0.05 + 0.10 * brightness,
                    0.10 + 0.20 * brightness,
                    1.0,
                );
            }
            ManifoldKind::Exhaust => {
                // Cooler / hotter exhaust shifts dull-cherry → bright orange.
                let t = (core.exhaust.temperature - 600.0) / 1400.0;
                let t = t.clamp(0.0, 1.0);
                let r = 0.30 + 0.70 * t;
                let g = 0.10 + 0.45 * t * t;
                let b = 0.05;
                mat.base_color = Color::srgb(r, g, b);
                mat.emissive = LinearRgba::new(r * t * 1.2, g * t * 1.2, b * t * 1.2, 1.0);
            }
        }
    }
}

// ──────────────── Pressure → RGB gradient (blue → cyan → yellow → red) ──────
fn pressure_gradient(t: f32) -> (f32, f32, f32) {
    // Three colour stops, linearly interpolated.
    if t < 0.5 {
        let u = t / 0.5;
        ( 0.10 + 0.05 * u, 0.40 + 0.55 * u, 0.85 - 0.15 * u )
    } else {
        let u = (t - 0.5) / 0.5;
        ( 0.15 + 0.85 * u, 0.95 - 0.65 * u, 0.70 - 0.65 * u )
    }
}

