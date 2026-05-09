//! Particle-based fluid/gas flow visualization.
//!
//! Three particle kinds, all driven from real simulation state:
//!   * **Intake** — blue particles spawned at the atmosphere intake, pulled
//!     through the runner toward whichever cylinder has its intake valve
//!     open, then absorbed at the bore.
//!   * **Combustion** — flame-coloured burst particles that radiate from the
//!     bore top while [`crate::engine::CylinderState::flash`] is active.
//!     Replaces the legacy emissive flash sphere.
//!   * **Exhaust** — orange particles spawned in the bore when the exhaust
//!     valve opens, pushed out through the runner and out the tailpipe.
//!
//! Pure visual effect — no physics is computed here, the simulation is
//! unchanged.

use bevy::pbr::{NotShadowCaster, NotShadowReceiver};
use bevy::prelude::*;
use std::f32::consts::TAU;

use crate::engine::{EngineCore, EngineLayout, VIS_SCALE};

// ── Tunables ─────────────────────────────────────────────────────────────────
const MAX_PARTICLES: usize = 1600;
const PARTICLE_RADIUS_M: f32 = 0.0030;

const INTAKE_PER_KGS: f32 = 3000.0;
const EXHAUST_PER_KGS: f32 = 3000.0;
const COMBUSTION_PER_FLASH_PER_SEC: f32 = 700.0;

const PARTICLE_BASE_SPEED: f32 = 6.0;
const EXHAUST_SPEED_MULT: f32 = 1.4;

const PARTICLE_LIFETIME: f32 = 3.0;
const COMBUSTION_LIFETIME: f32 = 0.55;

const WAYPOINT_REACH: f32 = 0.05 * VIS_SCALE;
const STEER_BLEND: f32 = 0.20;

// ── Components / resources ───────────────────────────────────────────────────

#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum ParticleKind { Intake, Exhaust, Combustion }

#[derive(Component)]
pub struct Particle {
    pub kind: ParticleKind,
    pub waypoints: Vec<Vec3>,
    pub cursor: usize,
    pub velocity: Vec3,
    pub age: f32,
    pub lifetime: f32,
    pub base_scale: f32,
}

#[derive(Component)]
pub struct ParticleVisual;

#[derive(Resource)]
pub struct ParticleAssets {
    pub mesh: Handle<Mesh>,
    pub intake_material: Handle<StandardMaterial>,
    pub exhaust_material: Handle<StandardMaterial>,
    pub combustion_material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
pub struct FlowGeometry {
    pub generation: u64,
    pub valid: bool,
    pub intake_paths: Vec<Vec<Vec3>>,
    pub exhaust_paths: Vec<Vec<Vec3>>,
    pub bore_tops: Vec<Vec3>,
    pub bore_tilts: Vec<f32>,
    pub bore_radius: f32,
}

#[derive(Resource, Default)]
pub struct ParticleSpawnAccum {
    pub intake: f32,
    pub exhaust: f32,
    pub combustion: Vec<f32>,
}

#[derive(Resource, Default)]
pub struct ParticleCount(pub usize);

#[derive(Resource)]
pub struct ParticleRng(u64);

impl ParticleRng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn unit(&mut self) -> f32 { (self.next_u32() as f32) / (u32::MAX as f32) }
    fn signed(&mut self) -> f32 { self.unit() * 2.0 - 1.0 }
    fn weighted_pick(&mut self, weights: &[f32]) -> Option<usize> {
        let total: f32 = weights.iter().sum();
        if total <= 0.0 { return None; }
        let mut t = self.unit() * total;
        for (i, &w) in weights.iter().enumerate() {
            t -= w;
            if t <= 0.0 { return Some(i); }
        }
        Some(weights.len() - 1)
    }
}

// ── Plugin ───────────────────────────────────────────────────────────────────

pub struct ParticlesPlugin;

impl Plugin for ParticlesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FlowGeometry::default())
            .insert_resource(ParticleSpawnAccum::default())
            .insert_resource(ParticleCount::default())
            .insert_resource(ParticleRng(0xCAFE_F00D_DEAD_BEEFu64))
            .add_systems(Startup, setup_particle_assets)
            .add_systems(
                Update,
                (
                    rebuild_flow_geometry,
                    update_combustion_material,
                    spawn_particles,
                    advance_particles,
                )
                    .chain()
                    .after(crate::engine::engine_step),
            );
    }
}

// ── Setup ────────────────────────────────────────────────────────────────────

fn setup_particle_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mesh = match Sphere::new(PARTICLE_RADIUS_M * VIS_SCALE).mesh().ico(1) {
        Ok(m) => meshes.add(m),
        Err(_) => meshes.add(Sphere::new(PARTICLE_RADIUS_M * VIS_SCALE)),
    };
    let intake_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.30, 0.75, 1.00, 0.85),
        emissive: LinearRgba::new(0.20, 0.55, 1.20, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let exhaust_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.00, 0.45, 0.10, 0.90),
        emissive: LinearRgba::new(1.6, 0.55, 0.10, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let combustion_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.00, 0.55, 0.20, 0.95),
        emissive: LinearRgba::new(3.0, 1.5, 0.4, 1.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    commands.insert_resource(ParticleAssets {
        mesh, intake_material, exhaust_material, combustion_material,
    });
}

// Combustion material follows the active fuel's flame colour.
fn update_combustion_material(
    core: Res<EngineCore>,
    assets: Option<Res<ParticleAssets>>,
    mut mats: ResMut<Assets<StandardMaterial>>,
) {
    let Some(assets) = assets else { return; };
    let Some(mat) = mats.get_mut(&assets.combustion_material) else { return; };
    let f = core.fuel.flame_color;
    mat.base_color = Color::srgba(f[0].max(0.4), f[1].max(0.25), f[2].max(0.10), 0.95);
    mat.emissive = LinearRgba::new(f[0] * 4.0, f[1] * 2.5, f[2] * 1.2, 1.0);
}

// ── Geometry rebuild on engine config change ─────────────────────────────────

fn rebuild_flow_geometry(
    core: Res<EngineCore>,
    mut flow: ResMut<FlowGeometry>,
    particles_q: Query<Entity, With<ParticleVisual>>,
    mut commands: Commands,
    mut count: ResMut<ParticleCount>,
    mut accum: ResMut<ParticleSpawnAccum>,
) {
    if flow.valid && flow.generation == core.config_generation {
        return;
    }

    for e in &particles_q {
        commands.entity(e).despawn();
    }
    count.0 = 0;
    accum.intake = 0.0;
    accum.exhaust = 0.0;

    let cfg = &core.config;
    let s = VIS_SCALE;
    let n = cfg.num_cylinders;
    accum.combustion = vec![0.0; n];

    if n == 0 {
        flow.generation = core.config_generation;
        flow.valid = true;
        flow.intake_paths.clear();
        flow.exhaust_paths.clear();
        flow.bore_tops.clear();
        flow.bore_tilts.clear();
        flow.bore_radius = 0.0;
        return;
    }

    let head_y = cfg.rod_length * s + 0.18 * s;
    let port_y = head_y + 0.04 * s;
    let runner_y_inline = head_y + 0.08 * s;
    let runner_y_v = head_y + 0.06 * s;
    let intake_z_inline = -0.10 * s;
    let exhaust_z_inline = 0.10 * s;
    let port_z_intake_local = -0.10 * s;
    let port_z_exhaust_local = 0.10 * s;

    let x_min = (0..n).map(|i| cfg.cyl_visual_x(i)).fold(f32::INFINITY, f32::min);
    let x_max = (0..n).map(|i| cfg.cyl_visual_x(i)).fold(f32::NEG_INFINITY, f32::max);
    let atmo_dist_x = 0.5 * s;
    let tail_dist_x = 0.6 * s;

    let mut intake_paths = Vec::with_capacity(n);
    let mut exhaust_paths = Vec::with_capacity(n);
    let mut bore_tops = Vec::with_capacity(n);
    let mut bore_tilts = Vec::with_capacity(n);

    for i in 0..n {
        let cx = cfg.cyl_visual_x(i);
        let tilt = cfg.cyl_bank_tilt(i);
        let bore_top = tilt_position(cx, head_y - 0.02 * s, 0.0, tilt);
        let intake_port = tilt_position(cx, port_y, port_z_intake_local, tilt);
        let exhaust_port = tilt_position(cx, port_y, port_z_exhaust_local, tilt);

        let intake_path = match cfg.layout {
            EngineLayout::Inline => vec![
                Vec3::new(x_min - atmo_dist_x, runner_y_inline, intake_z_inline),
                Vec3::new(x_min - 0.10 * s, runner_y_inline, intake_z_inline),
                Vec3::new(cx, runner_y_inline, intake_z_inline),
                intake_port,
                bore_top,
            ],
            _ => vec![
                Vec3::new(x_min - atmo_dist_x, runner_y_v, 0.0),
                Vec3::new(x_min - 0.10 * s, runner_y_v, 0.0),
                Vec3::new(cx, runner_y_v, 0.0),
                intake_port,
                bore_top,
            ],
        };

        let exhaust_path = match cfg.layout {
            EngineLayout::Inline => vec![
                bore_top,
                exhaust_port,
                Vec3::new(cx, runner_y_inline, exhaust_z_inline),
                Vec3::new(x_max + 0.10 * s, runner_y_inline, exhaust_z_inline),
                Vec3::new(x_max + tail_dist_x * 0.5, 0.30 * s, exhaust_z_inline + 0.10 * s),
                Vec3::new(x_max + tail_dist_x, -0.10 * s, exhaust_z_inline + 0.30 * s),
            ],
            _ => {
                let exh_local_z = 0.14 * s;
                let exh_runner_pt = tilt_position(cx, runner_y_v, exh_local_z, tilt);
                let exh_runner_far = tilt_position(x_max, runner_y_v, exh_local_z, tilt);
                let bank_dir = if tilt >= 0.0 { 1.0 } else { -1.0 };
                vec![
                    bore_top,
                    exhaust_port,
                    exh_runner_pt,
                    exh_runner_far,
                    Vec3::new(x_max + tail_dist_x * 0.5, 0.30 * s, bank_dir * 0.6 * s),
                    Vec3::new(x_max + tail_dist_x, -0.10 * s, bank_dir * 0.95 * s),
                ]
            }
        };

        intake_paths.push(intake_path);
        exhaust_paths.push(exhaust_path);
        bore_tops.push(bore_top);
        bore_tilts.push(tilt);
    }

    flow.generation = core.config_generation;
    flow.valid = true;
    flow.intake_paths = intake_paths;
    flow.exhaust_paths = exhaust_paths;
    flow.bore_tops = bore_tops;
    flow.bore_tilts = bore_tilts;
    flow.bore_radius = cfg.bore * 0.5 * s;
}

// ── Spawn ────────────────────────────────────────────────────────────────────

fn spawn_particles(
    time: Res<Time>,
    core: Res<EngineCore>,
    flow: Res<FlowGeometry>,
    assets: Option<Res<ParticleAssets>>,
    mut accum: ResMut<ParticleSpawnAccum>,
    mut rng: ResMut<ParticleRng>,
    mut count: ResMut<ParticleCount>,
    mut commands: Commands,
) {
    let Some(assets) = assets else { return; };
    if !flow.valid || !core.particles_enabled { return; }
    let dt = time.delta_seconds();
    if dt <= 0.0 { return; }
    let n = core.config.num_cylinders;
    if n == 0 || flow.intake_paths.is_empty() { return; }

    let intake_weights: Vec<f32> = (0..n)
        .map(|i| (core.cylinders[i].intake_lift / core.config.intake_peak_lift).clamp(0.05, 1.0))
        .collect();
    let exhaust_weights: Vec<f32> = (0..n)
        .map(|i| (core.cylinders[i].exhaust_lift / core.config.exhaust_peak_lift).clamp(0.0, 1.0))
        .collect();

    // ── Intake ────────────────────────────────────────────────────────────
    let intake_rate = (core.intake.flow_signal * INTAKE_PER_KGS).clamp(0.0, 1500.0);
    accum.intake = (accum.intake + intake_rate * dt).min(150.0);
    while accum.intake >= 1.0 && count.0 < MAX_PARTICLES {
        accum.intake -= 1.0;
        let Some(cyl) = rng.weighted_pick(&intake_weights) else { break };
        let path = &flow.intake_paths[cyl];
        if path.len() < 2 { continue; }
        let jitter = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 0.05 * VIS_SCALE;
        let pos = path[0] + jitter;
        let dir = (path[1] - path[0]).normalize_or_zero();
        let vel = dir * PARTICLE_BASE_SPEED * (0.85 + 0.30 * rng.unit());
        let lifetime = PARTICLE_LIFETIME * (0.75 + 0.40 * rng.unit());
        spawn_path_particle(&mut commands, &assets, ParticleKind::Intake, pos, vel, lifetime, path.clone());
        count.0 += 1;
    }

    // ── Exhaust ───────────────────────────────────────────────────────────
    let exh_rate = (core.exhaust.flow_signal * EXHAUST_PER_KGS).clamp(0.0, 1500.0);
    accum.exhaust = (accum.exhaust + exh_rate * dt).min(150.0);
    while accum.exhaust >= 1.0 && count.0 < MAX_PARTICLES {
        accum.exhaust -= 1.0;
        let Some(cyl) = rng.weighted_pick(&exhaust_weights) else { break };
        let path = &flow.exhaust_paths[cyl];
        if path.len() < 2 { continue; }
        let jitter = Vec3::new(rng.signed(), rng.signed(), rng.signed()) * 0.04 * VIS_SCALE;
        let pos = path[0] + jitter;
        let dir = (path[1] - path[0]).normalize_or_zero();
        let vel = dir * PARTICLE_BASE_SPEED * EXHAUST_SPEED_MULT * (0.85 + 0.40 * rng.unit());
        let lifetime = PARTICLE_LIFETIME * (0.75 + 0.50 * rng.unit());
        spawn_path_particle(&mut commands, &assets, ParticleKind::Exhaust, pos, vel, lifetime, path.clone());
        count.0 += 1;
    }

    // ── Combustion bursts ─────────────────────────────────────────────────
    if accum.combustion.len() != n {
        accum.combustion = vec![0.0; n];
    }
    for i in 0..n {
        let flash = core.cylinders[i].flash;
        if flash < 0.04 {
            // decay any leftover so we don't dump a burst when flash returns
            accum.combustion[i] *= (1.0 - 4.0 * dt).max(0.0);
            continue;
        }
        accum.combustion[i] = (accum.combustion[i] + COMBUSTION_PER_FLASH_PER_SEC * flash * dt).min(40.0);
        while accum.combustion[i] >= 1.0 && count.0 < MAX_PARTICLES {
            accum.combustion[i] -= 1.0;
            let bore_top = flow.bore_tops[i];
            let tilt = flow.bore_tilts[i];
            let theta = rng.unit() * TAU;
            let radial_dir = bore_radial_dir(theta, tilt);
            let axial_dir = bore_axial_dir(tilt);
            let r0 = flow.bore_radius * 0.10 * rng.unit();
            let pos = bore_top + radial_dir * r0;
            let radial_speed = 1.0 + 2.0 * rng.unit() * flash.sqrt();
            let axial_speed = 0.1 + 0.3 * rng.unit();
            let vel = radial_dir * radial_speed + axial_dir * axial_speed;
            let lifetime = COMBUSTION_LIFETIME * (0.45 + 0.40 * rng.unit());
            spawn_burst_particle(&mut commands, &assets, pos, vel, lifetime);
            count.0 += 1;
        }
    }
}

fn spawn_path_particle(
    commands: &mut Commands,
    assets: &ParticleAssets,
    kind: ParticleKind,
    pos: Vec3,
    velocity: Vec3,
    lifetime: f32,
    waypoints: Vec<Vec3>,
) {
    let material = match kind {
        ParticleKind::Intake => assets.intake_material.clone(),
        ParticleKind::Exhaust => assets.exhaust_material.clone(),
        ParticleKind::Combustion => assets.combustion_material.clone(),
    };
    commands.spawn((
        ParticleVisual,
        Particle { kind, waypoints, cursor: 1, velocity, age: 0.0, lifetime, base_scale: 1.0 },
        PbrBundle {
            mesh: assets.mesh.clone(),
            material,
            transform: Transform::from_translation(pos),
            ..default()
        },
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

fn spawn_burst_particle(
    commands: &mut Commands,
    assets: &ParticleAssets,
    pos: Vec3,
    velocity: Vec3,
    lifetime: f32,
) {
    commands.spawn((
        ParticleVisual,
        Particle {
            kind: ParticleKind::Combustion,
            waypoints: Vec::new(),
            cursor: 0,
            velocity,
            age: 0.0,
            lifetime,
            base_scale: 1.8,
        },
        PbrBundle {
            mesh: assets.mesh.clone(),
            material: assets.combustion_material.clone(),
            transform: Transform::from_translation(pos).with_scale(Vec3::splat(1.8)),
            ..default()
        },
        NotShadowCaster,
        NotShadowReceiver,
    ));
}

// ── Movement ─────────────────────────────────────────────────────────────────

fn advance_particles(
    time: Res<Time>,
    mut q: Query<(Entity, &mut Particle, &mut Transform)>,
    mut count: ResMut<ParticleCount>,
    mut commands: Commands,
) {
    let dt = time.delta_seconds().min(1.0 / 30.0);
    if dt <= 0.0 { return; }

    for (e, mut p, mut t) in &mut q {
        p.age += dt;
        if p.age > p.lifetime {
            commands.entity(e).despawn();
            count.0 = count.0.saturating_sub(1);
            continue;
        }

        match p.kind {
            ParticleKind::Combustion => {
                // Burst mode: strong dampening; expanding cloud that fades out quickly in place.
                p.velocity *= (1.0 - 8.0 * dt).max(0.0);
            }
            ParticleKind::Exhaust => {
                if p.cursor < p.waypoints.len() {
                    advance_along_path(&mut p, &t, dt);
                } else {
                    // Trail: drift past the tailpipe with light dampening.
                    p.velocity *= (1.0 - 0.6 * dt).max(0.0);
                }
            }
            ParticleKind::Intake => {
                if p.cursor < p.waypoints.len() {
                    advance_along_path(&mut p, &t, dt);
                } else {
                    // Reached the bore — absorbed.  Despawn cleanly.
                    commands.entity(e).despawn();
                    count.0 = count.0.saturating_sub(1);
                    continue;
                }
            }
        }

        t.translation += p.velocity * dt;

        let life_t = (p.age / p.lifetime).clamp(0.0, 1.0);
        let fade = (1.0 - life_t * life_t * life_t).max(0.05);
        t.scale = Vec3::splat(p.base_scale * fade);
    }
}

fn advance_along_path(p: &mut Particle, t: &Transform, dt: f32) {
    let target = p.waypoints[p.cursor];
    let to = target - t.translation;
    let dist = to.length();
    if dist > 1e-6 {
        let desired_dir = to / dist;
        let speed = p.velocity.length().max(PARTICLE_BASE_SPEED * 0.4);
        // Time-scaled steering (approx 15.0 rad/s)
        let steer = (15.0 * dt).clamp(0.0, 1.0);
        p.velocity = p.velocity.lerp(desired_dir * speed, steer);
    }
    
    // Increase reach radius slightly to prevent orbiting on fast particles/short segments
    let reach = WAYPOINT_REACH * 2.0; 
    
    // Check if we reached the waypoint OR if we passed its plane (dot product < 0)
    let passed_plane = if dist > 1e-6 {
        p.velocity.dot(to) < 0.0 && dist < reach * 2.0
    } else { false };

    if dist < reach || passed_plane {
        p.cursor += 1;
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[inline]
fn tilt_position(x: f32, y_local: f32, z_local: f32, tilt: f32) -> Vec3 {
    let cos_t = tilt.cos();
    let sin_t = tilt.sin();
    Vec3::new(
        x,
        y_local * cos_t - z_local * sin_t,
        y_local * sin_t + z_local * cos_t,
    )
}

/// Radial direction in the bore's cross-section plane (perpendicular to bore axis).
#[inline]
fn bore_radial_dir(theta: f32, tilt: f32) -> Vec3 {
    let st = tilt.sin();
    let ct = tilt.cos();
    Vec3::new(theta.cos(), -theta.sin() * st, theta.sin() * ct)
}

/// Axial direction along the bore (toward the head).
#[inline]
fn bore_axial_dir(tilt: f32) -> Vec3 {
    Vec3::new(0.0, tilt.cos(), tilt.sin())
}
