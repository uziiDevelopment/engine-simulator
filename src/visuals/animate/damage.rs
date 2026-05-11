//! Damage-view tint, flywheel + clutch surface materials.

use bevy::prelude::*;

use crate::engine::EngineCore;
use crate::visuals::{Clutch, DamageSource, DamageVisual, Flywheel};

/// Damage view: every `DamageVisual` part takes the FEA jet colormap when
/// `core.damage_view` is on; off → restore original PBR look.
pub fn animate_damage(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&DamageVisual>,
) {
    for vis in &q {
        let Some(mat) = materials.get_mut(&vis.material) else { continue };

        if !core.damage_view {
            mat.base_color = vis.base_color;
            mat.emissive = vis.base_emissive;
            continue;
        }

        let t = damage_value(&core, vis.source);
        let (r, g, b) = jet_colormap(t);
        let alpha = match vis.base_color {
            Color::Srgba(c) => c.alpha,
            _ => 1.0,
        };
        let a_out = (alpha + 0.55 * t).min(1.0);
        mat.base_color = Color::srgba(r, g, b, a_out);
        let glow = (t * 1.2).clamp(0.0, 1.5);
        mat.emissive = LinearRgba::new(r * glow, g * glow * 0.8, b * glow * 0.4, 1.0);
    }
}

/// Standard six-stop ramp blue → cyan → green → yellow → orange → red.
fn jet_colormap(t: f32) -> (f32, f32, f32) {
    let t = t.clamp(0.0, 1.0);
    let r = (1.5 - (4.0 * t - 3.0).abs()).clamp(0.0, 1.0);
    let g = (1.5 - (4.0 * t - 2.0).abs()).clamp(0.0, 1.0);
    let b = (1.5 - (4.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    (r, g, b)
}

/// Map a [`DamageSource`] into 0..=1 for the colormap.
fn damage_value(core: &EngineCore, source: DamageSource) -> f32 {
    let mats = &core.config.materials;
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
            let cyl = core.cylinders.get(i).copied().unwrap_or_else(crate::engine::CylinderState::inert);
            cyl.rod_damage * 0.8
        }
    }
}

// ── Flywheel surface (metallic override, applied once GLB loads) ─────────
pub fn apply_flywheel_material(
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_flywheel: Query<Entity, With<Flywheel>>,
    q_children: Query<&Children>,
    q_material: Query<&Handle<StandardMaterial>>,
    mut done: Local<bevy::utils::HashSet<Entity>>,
) {
    for entity in &q_flywheel {
        if done.contains(&entity) { continue; }
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
        if found { done.insert(entity); }
    }
}

// ── Clutch surface (thermal glow with clutch_temp) ───────────────────────
pub fn apply_clutch_material(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q_clutch: Query<Entity, With<Clutch>>,
    q_children: Query<&Children>,
    q_material: Query<&Handle<StandardMaterial>>,
) {
    let temp = core.clutch_temp;
    let t_norm = ((temp - 300.0) / 900.0).clamp(0.0, 1.0);

    let (base_color, emissive) = if t_norm < 0.1 {
        (Color::srgb(0.25, 0.26, 0.28), LinearRgba::BLACK)
    } else if t_norm < 0.5 {
        let alpha = (t_norm - 0.1) / 0.4;
        (
            Color::srgb(0.25 + alpha * 0.5, 0.26 * (1.0 - alpha), 0.28 * (1.0 - alpha)),
            LinearRgba::new(alpha * 0.5, 0.0, 0.0, 1.0),
        )
    } else {
        let alpha = (t_norm - 0.5) / 0.5;
        (
            Color::srgb(0.75 + alpha * 0.2, alpha * 0.4, 0.0),
            LinearRgba::new(0.5 + alpha * 10.0, alpha * 3.0, 0.0, 1.0),
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
