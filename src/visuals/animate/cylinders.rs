//! Cylinder bore gas visualisation: tint + emissive driven by pressure +
//! burned fraction + flash impulse.

use bevy::prelude::*;

use crate::engine::{EngineCore, P_ATM};
use crate::visuals::CylinderGasViz;

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

        // Pressure ratio (1× ambient → 60×). Maps to a blue→cyan→yellow→red ramp.
        let pr = (p / P_ATM).clamp(0.5, 60.0);
        let t01 = ((pr.ln() / 60.0_f32.ln()).clamp(0.0, 1.0)).powf(0.7);

        let (r, g, b) = pressure_gradient(t01);
        let burned_tint = cyl.burned_frac.clamp(0.0, 1.0) * 0.4;
        let r_final = r * (1.0 - burned_tint) + 0.20 * burned_tint;
        let g_final = g * (1.0 - burned_tint) + 0.15 * burned_tint;
        let b_final = b * (1.0 - burned_tint) + 0.18 * burned_tint;

        let alpha = (0.10 + 0.55 * t01).min(0.85);
        let emissive_strength = (cyl.flash * 1.6 + (t01 - 0.55).max(0.0) * 0.5).clamp(0.0, 2.5);

        if let Some(mat) = materials.get_mut(&viz.bore_material) {
            mat.base_color = Color::srgba(r_final, g_final, b_final, alpha);
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

/// Pressure → RGB gradient (blue → cyan → yellow → red).
fn pressure_gradient(t: f32) -> (f32, f32, f32) {
    if t < 0.5 {
        let u = t / 0.5;
        (0.10 + 0.05 * u, 0.40 + 0.55 * u, 0.85 - 0.15 * u)
    } else {
        let u = (t - 0.5) / 0.5;
        (0.15 + 0.85 * u, 0.95 - 0.65 * u, 0.70 - 0.65 * u)
    }
}
