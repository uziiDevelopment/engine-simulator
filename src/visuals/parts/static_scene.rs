//! Static scene elements that don't depend on engine config: floor lighting.

use bevy::prelude::*;

/// Spawns static lighting. Called once at startup.
pub fn setup_static_scene(
    mut commands: Commands,
    mut _meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let _floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.07, 0.075, 0.09),
        perceptual_roughness: 0.95,
        ..default()
    });

    commands.spawn(DirectionalLightBundle {
        directional_light: DirectionalLight {
            illuminance: 9000.0,
            shadows_enabled: true,
            ..default()
        },
        transform: Transform::from_xyz(4.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 1_500_000.0,
            color: Color::srgb(1.0, 0.55, 0.35),
            range: 25.0,
            ..default()
        },
        transform: Transform::from_xyz(0.0, 0.5, -3.5),
        ..default()
    });
    commands.spawn(PointLightBundle {
        point_light: PointLight {
            intensity: 800_000.0,
            color: Color::srgb(0.5, 0.7, 1.0),
            range: 22.0,
            ..default()
        },
        transform: Transform::from_xyz(-3.5, 3.5, 4.0),
        ..default()
    });
}
