//! Intake / exhaust manifold colour driven by pressure and temperature.

use bevy::prelude::*;

use crate::engine::{EngineCore, P_ATM};
use crate::visuals::{ManifoldKind, ManifoldViz};

pub fn animate_manifolds(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&ManifoldViz>,
) {
    for vis in &q {
        let Some(mat) = materials.get_mut(&vis.material) else { continue };
        match vis.kind {
            ManifoldKind::Intake => {
                // Manifold-air pressure: deep vacuum darker, boost brighter.
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
                // Dull-cherry → bright orange with temperature.
                let t = ((core.exhaust.temperature - 600.0) / 1400.0).clamp(0.0, 1.0);
                let r = 0.30 + 0.70 * t;
                let g = 0.10 + 0.45 * t * t;
                let b = 0.05;
                mat.base_color = Color::srgb(r, g, b);
                mat.emissive = LinearRgba::new(r * t * 1.2, g * t * 1.2, b * t * 1.2, 1.0);
            }
        }
    }
}
