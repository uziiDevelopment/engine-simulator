//! Rendering: spawns the engine geometry dynamically, animates it from [`EngineCore`].
//!
//! The visuals are read-only consumers of the simulation: every system in
//! `animate` queries components + the resource and writes to `Transform`s and
//! material handles.  No physics happens here.
//!
//! The scene is **fully dynamic**: when the engine config changes (detected via
//! `config_generation`), all engine entities are despawned and re-spawned to
//! match the new cylinder count / layout.

mod animate;
mod parts;
mod particles;

use bevy::prelude::*;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VisualGeneration(u64::MAX)) // force initial spawn
            .add_systems(Startup, parts::setup_static_scene)
            .add_systems(
                Update,
                (
                    rebuild_engine_visuals,
                    animate::animate_crank,
                    animate::animate_drivetrain,
                    animate::animate_pistons,
                    animate::animate_rods,
                    animate::animate_valves,
                    animate::animate_cylinder_gas,
                    animate::animate_manifolds,
                    animate::animate_turbo,
                    animate::animate_damage,
                    animate::apply_flywheel_material,
                    animate::apply_clutch_material,
                    sync_damage_visual_materials,
                    discover_rod_attachments,
                )
                    .chain()
                    .after(crate::engine::engine_step),
            )
            .add_plugins(particles::ParticlesPlugin);
    }
}

// ── Resource tracking which generation we last spawned ───────────────────────
#[derive(Resource)]
struct VisualGeneration(u64);

/// Detects config changes and respawns the engine visual entities.
fn rebuild_engine_visuals(
    mut commands: Commands,
    core: Res<crate::engine::EngineCore>,
    mut vis_gen: ResMut<VisualGeneration>,
    engine_q: Query<Entity, With<EngineVisual>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    if vis_gen.0 == core.config_generation {
        return;
    }
    // Despawn all existing engine visual entities
    for entity in &engine_q {
        commands.entity(entity).despawn_recursive();
    }
    // Spawn new visuals based on current config
    parts::spawn_engine_visuals(&mut commands, &core, &mut meshes, &mut materials, &asset_server);
    vis_gen.0 = core.config_generation;
}

// ── Marker / data components shared across the submodules ───────────────────

/// Marker on every entity that belongs to the engine visual (for despawn).
#[derive(Component)]
pub struct EngineVisual;

#[derive(Component)] pub struct Crankshaft;
#[derive(Component)] pub struct Flywheel;
#[derive(Component)] pub struct Clutch;

/// The compressor wheel of the turbocharger (cold side, intake).
#[derive(Component)] pub struct CompressorWheel;
/// The turbine wheel of the turbocharger (hot side, exhaust).
#[derive(Component)] pub struct TurbineWheel;
/// The translucent housing material — tinted by boost pressure.
#[derive(Component)]
pub struct TurboHousing { pub material: Handle<StandardMaterial> }

#[derive(Component)]
pub struct Piston {
    pub idx: usize,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

#[derive(Component)]
pub struct ConRod {
    pub idx: usize,
    pub base_x: f32,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

/// Stores the local positions of the markers found in the GLB model.
#[derive(Component)]
pub struct RodAttachmentPoints {
    pub top: Vec3,
    pub bottom: Vec3,
}

#[derive(Component, Clone, Copy)]
pub enum ValveKind { Intake, Exhaust }

#[derive(Component)]
pub struct Valve {
    pub cyl: usize,
    pub kind: ValveKind,
    pub seat_y: f32,
    pub z_local: f32,
    /// Bank tilt angle (rad) — 0 for inline.
    pub bank_tilt: f32,
}

#[derive(Component)]
pub struct CylinderGasViz {
    pub idx: usize,
    pub bore_material: Handle<StandardMaterial>,
    pub bank_tilt: f32,
}

#[derive(Component, Clone, Copy)]
pub enum ManifoldKind { Intake, Exhaust }

#[derive(Component)]
pub struct ManifoldViz {
    pub kind: ManifoldKind,
    pub material: Handle<StandardMaterial>,
}

/// What this part's damage colour should be sampled from.
#[derive(Component, Clone, Copy, Debug)]
pub enum DamageSource {
    /// Cylinder block / wall slice for cylinder `i` — drives off `wall_wear`
    /// and `block_temp`.
    BlockSlice(usize),
    /// Connecting-rod `i` — drives off `rod_damage`.
    Rod(usize),
    /// Piston `i` — drives off `piston_temp` and `ring_wear` (mild).
    Piston(usize),
    /// Piston ring on cylinder `i` — drives off `ring_wear`.
    PistonRing(usize),
    /// Crank pin attached to cylinder `i` — drives off `rod_damage`
    /// (stress concentration mirror).
    CrankPin(usize),
}

/// Marker on every part whose surface colour is driven by per-cylinder damage.
/// `material` is the unique StandardMaterial handle for this part; `base_color`
/// + `base_emissive` capture the original PBR appearance so we can restore it
/// when the player toggles damage-view off.
#[derive(Component, Clone)]
pub struct DamageVisual {
    pub source: DamageSource,
    pub material: Handle<StandardMaterial>,
    pub base_color: Color,
    pub base_emissive: LinearRgba,
}
/// System that propagates the material from a [`DamageVisual`] root entity
/// down to all its children (e.g. sub-meshes within a loaded GLB scene).
pub fn sync_damage_visual_materials(
    q_vis: Query<(Entity, &DamageVisual)>,
    q_children: Query<&Children>,
    mut q_material: Query<&mut Handle<StandardMaterial>>,
) {
    for (root, vis) in &q_vis {
        // Apply to root itself if it has a mesh/material
        if let Ok(mut mat) = q_material.get_mut(root) {
            if *mat != vis.material {
                *mat = vis.material.clone();
            }
        }
        // Apply to all children in the hierarchy
        for child in q_children.iter_descendants(root) {
            if let Ok(mut mat) = q_material.get_mut(child) {
                if *mat != vis.material {
                    *mat = vis.material.clone();
                }
            }
        }
    }
}

/// Scans the children of a spawned rod to find the `attach_top` and `attach_bottom` 
/// markers, then caches their local positions.
pub fn discover_rod_attachments(
    mut commands: Commands,
    q_rods: Query<(Entity, &Children), (With<ConRod>, Without<RodAttachmentPoints>)>,
    q_children: Query<&Children>,
    q_named: Query<(&Name, &Transform)>,
) {
    for (entity, children) in &q_rods {
        let mut top = None;
        let mut bottom = None;

        // Search descendants for the markers
        let mut stack = children.to_vec();
        while let Some(child) = stack.pop() {
            if let Ok((name, transform)) = q_named.get(child) {
                if name.as_str() == "attach_top" {
                    top = Some(transform.translation);
                } else if name.as_str() == "attach_bottom" {
                    bottom = Some(transform.translation);
                }
            }
            if let Ok(child_children) = q_children.get(child) {
                stack.extend(child_children.iter());
            }
        }

        if let (Some(t), Some(b)) = (top, bottom) {
            commands.entity(entity).insert(RodAttachmentPoints { top: t, bottom: b });
        }
    }
}
