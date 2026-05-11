//! Gearbox visuals: shaft rotation, reverse-idler spin, sleeve sliding,
//! housing damage tint.

use bevy::prelude::*;

use crate::engine::{gearbox::GearSelector, EngineCore};
use crate::visuals::{EngagementSleeve, GearboxHousing, Layshaft, Mainshaft, ReverseIdler};

/// Spin the mainshaft at `drivetrain_angle` and the layshaft at the gearbox's
/// internal layshaft angle (already accumulated in `post_integrate`).
pub fn animate_gearbox_shafts(
    core: Res<EngineCore>,
    mut q_main: Query<&mut Transform, (With<Mainshaft>, Without<Layshaft>)>,
    mut q_lay: Query<&mut Transform, (With<Layshaft>, Without<Mainshaft>)>,
) {
    use std::f32::consts::PI;
    let base = Quat::from_rotation_z(PI / 2.0);
    let main_spin = Quat::from_rotation_y(core.drivetrain_angle);
    for mut t in &mut q_main { t.rotation = base * main_spin; }
    let lay_spin = Quat::from_rotation_y(core.gearbox.layshaft_angle);
    for mut t in &mut q_lay { t.rotation = base * lay_spin; }
}

/// Forward cogs are children of their shaft so they inherit rotation; this
/// system handles only the reverse idler (which sits free in world space).
pub fn animate_gear_cogs(
    core: Res<EngineCore>,
    mut q_idler: Query<&mut Transform, With<ReverseIdler>>,
) {
    let rot = Quat::from_rotation_x(core.gearbox.reverse_idler_angle);
    for mut t in &mut q_idler {
        t.rotation = rot;
    }
}

/// Slide each engagement sleeve toward its target gear position.
pub fn animate_engagement_sleeves(
    time: Res<Time>,
    core: Res<EngineCore>,
    mut q: Query<(&EngagementSleeve, &mut Transform)>,
) {
    let target_offset_for = |hub_idx: u8| -> f32 {
        match (hub_idx, core.gearbox.selector) {
            (0, GearSelector::Gear(1)) => -1.0,
            (0, GearSelector::Gear(2)) =>  1.0,
            (1, GearSelector::Gear(3)) => -1.0,
            (1, GearSelector::Gear(4)) =>  1.0,
            (2, GearSelector::Gear(5)) => -1.0,
            (2, GearSelector::Gear(6)) =>  1.0,
            _ => 0.0,
        }
    };
    let dt = time.delta_seconds();
    let alpha = (dt * 8.0).min(1.0);
    for (sleeve, mut tr) in &mut q {
        let direction = if sleeve.hub_idx == 3 {
            if matches!(core.gearbox.selector, GearSelector::Reverse) { 1.0 } else { 0.0 }
        } else {
            target_offset_for(sleeve.hub_idx)
        };
        let target_y = sleeve.neutral_x + direction * sleeve.engage_offset * 0.85;
        tr.translation.y += (target_y - tr.translation.y) * alpha;
    }
}

/// Tint the gearbox housing redder as `gearbox_damage` grows.
pub fn animate_housing_damage(
    core: Res<EngineCore>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    q: Query<&GearboxHousing>,
) {
    for h in &q {
        let Some(mat) = materials.get_mut(&h.material) else { continue };
        let d = core.gearbox.gearbox_damage.clamp(0.0, 1.0);
        let lin = h.base_color.to_linear();
        let r = lin.red + (0.7 - lin.red) * d;
        let g = lin.green * (1.0 - 0.6 * d);
        let b = lin.blue * (1.0 - 0.7 * d);
        let a = lin.alpha + (0.6 - lin.alpha) * d;
        mat.base_color = Color::linear_rgba(r, g, b, a);
    }
}
