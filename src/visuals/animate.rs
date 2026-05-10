//! Per-frame animation systems.  Read [`EngineCore`], write transforms +
//! material handles.  No simulation logic here.

use bevy::prelude::*;

use crate::engine::{
    EngineCore, P_ATM, VIS_SCALE,
};

use super::{
    ConRod, Crankshaft, CylinderGasViz, DamageSource, DamageVisual,
    ManifoldKind, ManifoldViz, Piston, Valve, ValveKind, RodAttachmentPoints,
};

// ─────────────────────────────────── Crank ──────────────────────────────────
pub fn animate_crank(core: Res<EngineCore>, mut q: Query<&mut Transform, With<Crankshaft>>) {
    for mut t in &mut q {
        t.rotation = Quat::from_rotation_x(core.angle);
    }
}

pub fn animate_drivetrain(core: Res<EngineCore>, mut q: Query<&mut Transform, With<super::Clutch>>) {
    let base_rot = Quat::from_rotation_y(-std::f32::consts::PI / 2.0);
    
    // Recalculate rear_x relative to the crankshaft origin
    let positions = core.config.crank_positions();
    let spacing = core.config.cylinder_spacing * VIS_SCALE;
    let center_x = (positions as f32 - 1.0) * 0.5 * spacing;
    let rear_x = center_x + spacing * 0.5;
    
    // Clutch throw: move 50mm away when fully disengaged
    let throw = (1.0 - core.clutch_engagement) * 0.050;

    for mut t in &mut q {
        // Since it's parented to the crankshaft, we must subtract the crank's rotation
        // to make it rotate at the drivetrain's speed in world space.
        let relative_angle = core.drivetrain_angle - core.angle;
        t.rotation = Quat::from_rotation_x(relative_angle) * base_rot;
        
        // Local position relative to crankshaft origin.
        // Flywheel is at (rear_x, 0, 0).
        t.translation = Vec3::new(rear_x + 0.06 * VIS_SCALE + throw, 0.0, 0.0);
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
pub fn animate_rods(core: Res<EngineCore>, mut q: Query<(&ConRod, &mut Transform, Option<&RodAttachmentPoints>)>) {
    let r = core.config.crank_radius() * VIS_SCALE;
    for (rod, mut t, points) in &mut q {
        if rod.idx >= core.config.num_cylinders { continue; }
        let pin_phase = core.config.crank_phases[rod.idx];
        let world_theta = core.angle + pin_phase;
        // Crank pin position (in world space, crank rotates in Y-Z plane)
        let pin = Vec3::new(rod.base_x, r * world_theta.cos(), r * world_theta.sin());
        // Piston wrist-pin position (along the tilted bank axis)
        let y_p = core.config.piston_y(core.angle, rod.idx) * VIS_SCALE;
        let tilt = rod.bank_tilt;
        let small = tilt_vec(rod.base_x, y_p, 0.0, tilt);

        let dir = (small - pin).normalize_or_zero();
        let target_len = (small - pin).length();

        if let Some(pts) = points {
                // Robust alignment: Calculate the model's own axis from its markers
                let model_vec = pts.top - pts.bottom;
                let model_dist = model_vec.length();
                if model_dist > 1e-4 {
                    let scale = target_len / model_dist;
                    t.scale = Vec3::splat(scale);

                    let model_dir = model_vec.normalize();
                    // Twist exactly around the rod's own longitudinal axis
                    let twist = Quat::from_axis_angle(model_dir, std::f32::consts::PI / 2.0);
                    t.rotation = Quat::from_rotation_arc(model_dir, dir) * twist;
                    
                    // Position so `attach_bottom` sits exactly on the crank pin
                    t.translation = pin - t.rotation.mul_vec3(pts.bottom * scale);
                }
        } else {
            // Fallback for placeholder cuboid or before markers are discovered
            let mid = (pin + small) * 0.5;
            t.translation = mid;
            t.rotation = Quat::from_rotation_arc(Vec3::Z, dir);
        }
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
        let x = t.translation.x;
        // Move seat_y down along the tilted axis, preserving z_local lateral offset
        t.translation = tilt_vec(x, v.seat_y - delta, v.z_local, tilt);
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

// ──────────────── FEA-style "jet" colormap (blue→cyan→green→yellow→orange→red) ──
//
// Standard six-stop ramp used in CFD/FEA visualisations.  `t` clamped to 0..=1.
// 0.0 → deep blue (cool/healthy), 1.0 → red (hot/destroyed).
fn jet_colormap(t: f32) -> (f32, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let r = (1.5 - (4.0 * t - 3.0).abs()).clamp(0.0, 1.0);
    let g = (1.5 - (4.0 * t - 2.0).abs()).clamp(0.0, 1.0);
    let b = (1.5 - (4.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    (r, g, b)
}

// ──────────────── Damage / heat colour driver ───────────────────────────────
//
// When `core.damage_view` is on, every [`DamageVisual`] part is recoloured by
// the FEA jet gradient based on its damage source (wall wear, ring wear, rod
// fatigue, piston / block temperature).  Off → restores the part's original
// PBR colour.
pub fn animate_damage(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&DamageVisual>,
) {
    for vis in &q {
        let Some(mat) = materials.get_mut(&vis.material) else { continue };

        if !core.damage_view {
            // Restore original look every frame so toggling off snaps back.
            mat.base_color = vis.base_color;
            mat.emissive = vis.base_emissive;
            continue;
        }

        // Sample the relevant damage value (0..1) from this part's source.
        let t = damage_value(&core, vis.source);

        let (r, g, b) = jet_colormap(t);
        // Preserve the source's original alpha (e.g. translucent block slices)
        // so we don't accidentally make see-through parts opaque.
        let alpha = match vis.base_color {
            Color::Srgba(c) => c.alpha,
            _ => 1.0,
        };
        // In damage view, push opaque parts brighter so the gradient pops; keep
        // translucent parts (block slice) more visible by raising their alpha
        // proportionally to the damage level.
        let a_out = (alpha + 0.55 * t).min(1.0);
        mat.base_color = Color::srgba(r, g, b, a_out);
        // Emissive glow scales with t so red parts visibly glow under damage view.
        let glow = (t * 1.2).clamp(0.0, 1.5);
        mat.emissive = LinearRgba::new(r * glow, g * glow * 0.8, b * glow * 0.4, 1.0);
    }
}

/// Map a [`DamageSource`] into a 0..=1 scalar suitable for the colormap.
fn damage_value(core: &EngineCore, source: DamageSource) -> f32 {
    let mats = &core.config.materials;
    // Common reference: ambient ~ 290 K.
    const T_AMBIENT: f32 = 290.0;
    let temp_norm = |t: f32, melt: f32| -> f32 {
        let span = (melt - T_AMBIENT).max(50.0);
        ((t - T_AMBIENT) / span).clamp(0.0, 1.0)
    };

    match source {
        DamageSource::BlockSlice(i) => {
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            let thermal = temp_norm(cyl.block_temp, mats.block.melting_point);
            cyl.wall_wear.max(thermal)
        }
        DamageSource::Rod(i) => {
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            cyl.rod_damage
        }
        DamageSource::Piston(i) => {
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            let thermal = temp_norm(cyl.piston_temp, mats.piston.melting_point);
            (thermal * 0.85 + cyl.ring_wear * 0.4).clamp(0.0, 1.0)
        }
        DamageSource::PistonRing(i) => {
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            cyl.ring_wear
        }
        DamageSource::CrankPin(i) => {
            // Crank pin heat = stress concentration mirror of attached rod's damage.
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            cyl.rod_damage * 0.8
        }
    }
}



// ──────────────── Flywheel material override (metal) ────────────────────────
pub fn apply_flywheel_material(
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_flywheel: Query<Entity, With<super::Flywheel>>,
    q_children: Query<&Children>,
    q_material: Query<&Handle<StandardMaterial>>,
    mut done: Local<bevy::utils::HashSet<Entity>>,
) {
    for entity in &q_flywheel {
        if done.contains(&entity) { continue; }
        
        // Traverse children to find materials and override them
        let mut found = false;
        for child in q_children.iter_descendants(entity) {
            if let Ok(mat_handle) = q_material.get(child) {
                if let Some(mat) = materials.get_mut(mat_handle) {
                    mat.base_color = Color::srgb(0.7, 0.72, 0.75);
                    mat.metallic = 1.0;
                    mat.perceptual_roughness = 0.25;
                    found = true;
                }
            }
        }
        if found {
            done.insert(entity);
        }
    }
}

// ──────────────── Clutch material override (thermal glow) ───────────────────
pub fn apply_clutch_material(
    core: Res<crate::engine::EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_clutch: Query<Entity, With<super::Clutch>>,
    q_children: Query<&Children>,
    q_material: Query<&Handle<StandardMaterial>>,
) {
    // Temperature-based tinting
    let temp = core.clutch_temp;
    let t_norm = ((temp - 300.0) / 900.0).clamp(0.0, 1.0); // 0 at 300K, 1 at 1200K
    
    let (base_color, emissive) = if t_norm < 0.1 {
        (Color::srgb(0.25, 0.26, 0.28), LinearRgba::BLACK)
    } else if t_norm < 0.5 {
        // Heating up: grey -> dull red
        let alpha = (t_norm - 0.1) / 0.4;
        (
            Color::srgb(0.25 + alpha * 0.5, 0.26 * (1.0 - alpha), 0.28 * (1.0 - alpha)),
            LinearRgba::new(alpha * 0.5, 0.0, 0.0, 1.0)
        )
    } else {
        // Glowing: red -> orange/yellow
        let alpha = (t_norm - 0.5) / 0.5;
        (
            Color::srgb(0.75 + alpha * 0.2, alpha * 0.4, 0.0),
            LinearRgba::new(0.5 + alpha * 10.0, alpha * 3.0, 0.0, 1.0)
        )
    };

    for entity in &q_clutch {
        for child in q_children.iter_descendants(entity) {
            if let Ok(mat_handle) = q_material.get(child) {
                if let Some(mat) = materials.get_mut(mat_handle) {
                    mat.base_color = base_color;
                    mat.emissive = emissive;
                }
            }
        }
    }
}
