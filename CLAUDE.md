# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
cargo build           # debug build
cargo build --release # optimized build
cargo run             # build and launch simulator
```

No test suite exists — validation is done by running the simulator.

## Architecture

A real-time combustion engine simulator built on **Bevy 0.14** (ECS, Edition 2024), with procedural audio via **rodio 0.18** and an immediate-mode UI via **bevy_egui 0.30**.

`main.rs` registers plugins in dependency order:
1. `EnginePlugin` — physics simulation
2. `VisualsPlugin` — 3D rendering and animation
3. `CameraPlugin` — orbit camera
4. `UiPlugin` — egui control panel
5. `EngineAudioPlugin` — procedural audio from exhaust pressure

### Physics loop (`src/engine.rs`)

Each Bevy frame runs 80 substeps for numerical stability at high RPM. The single degree of freedom is the crankshaft (`omega`, `angle`, `fourstroke_angle`). `fourstroke_angle` is crank angle mod 4π — necessary to distinguish intake/exhaust strokes on the same cylinder within a 720° cycle. Everything else is derived each substep in this order:

1. Throttle + manifold plumbing (atmosphere ↔ intake manifold ↔ cylinders ↔ exhaust manifold ↔ atmosphere)
2. Per-cylinder gas thermodynamics + mechanical wear/stress (`cylinder.rs`)
3. Rod-bearing, main-bearing, and cam-bearing journal physics (`bearing.rs`)
4. Oil system update — viscosity, pressure, temperature, consumption (`oil.rs`)
5. Dyno absorption torque subtracted if active (`dyno.rs`)
6. Crank torque integration → angular acceleration → updated `omega`

Run-state machine lives in `engine/state.rs`: Off → Cranking (E key) → Running (self-sustains above ~220 RPM stall threshold).

### Engine configurations (`src/engine/config.rs` + `src/engine/config/`)

Nine presets, all dynamically switchable at runtime (entire 3D engine is despawned and rebuilt by `VisualsPlugin` on change). Each preset lives in its own file under `src/engine/config/`:

- **Inline-4** — 2.0L, 86×86 mm, 10.5:1 CR, 8000 RPM redline
- **Inline-5** — 2.5L, 82.5×92.8 mm, 7200 RPM redline
- **Inline-6** — straight-six variant
- **V8** — 5.0L, 90° bank angle, 8000 RPM redline
- **V10** — V-layout ten-cylinder
- **V12** — V-layout twelve-cylinder
- **W16** — Bugatti-style narrow-angle W layout
- **Flat-6** — 3.8L, 180° boxer, 7200 RPM redline
- **F1 V6** — Formula 1 spec turbocharged V6

Each `EngineConfig` includes a `MaterialsConfig` that assigns a `Material` to block, cylinder wall, piston, piston rings, conrod, and each bearing class. Adding a new preset means adding a `src/engine/config/name.rs` with a `preset()` function and registering it in `config.rs`.

### Thermodynamic core (`src/engine/thermo.rs`)

Primitives shared by all gas-handling code:
- Ideal gas law: P = m·R·T / V
- Atmosphere: 101,325 Pa, 295 K
- Heat capacities: CP_AIR 1005, CV_AIR 718, CV_BURNED 950, CV_FUEL 1700 J/(kg·K); γ = 1.40 (air) / 1.28 (burned)
- **Orifice flow** with choked-flow detection (critical pressure ratio = (2/(γ+1))^(γ/(γ-1)))
- **Bidirectional flow**: `flow_between()` couples manifold volumes
- **Wiebe burn**: `wiebe(delta, duration, a=5, m=2)` — SI combustion shape

### Manifold and plumbing (`src/engine/manifold.rs`)

- Intake plenum: 2.0 L; exhaust plenum: 1.5 L
- Throttle: max area 0.0014 m²; idle bleed 1.2% of max at 0% throttle
- Tailpipe: 0.0010 m² (10 cm²)
- Mass-weighted enthalpy mixing on intake; Newton cooling on exhaust (1.6 K/s)
- Flow signal is exponentially smoothed (0.6 decay + 0.4 current) for VFX

### Crankshaft dynamics (`src/engine/crank.rs`)

- Friction model: `FRICTION_BASE` (12.0 Nm) + viscous (0.045 Nm·s/rad) + windage (0.00012 Nm·s²/rad²)
- Starter motor: 80 Nm peak, linear falloff; disengages at 600 RPM
- Flywheel inertia: 0.18 kg·m²

### Valve timing (`src/engine/valve.rs`)

- Fixed sinusoidal cam profile (peak lift 10 mm): intake opens 354°/closes 580°, exhaust opens 140°/closes 366° (6° overlap)
- Valve diameters: intake 34 mm, exhaust 30 mm
- Effective discharge area: `min(curtain_area, seat_area)` — curtain area (π·D·lift) dominates at low lift

### Slider-crank kinematics (`src/engine/geometry.rs`)

- `piston_y(theta, cyl_idx)` — piston position from crank angle and cylinder index
- `dpiston_dtheta()` — derivative used for torque conversion from pressure to crank torque
- Visual scale: 8.0× for 3D rendering

### Fuel system (`src/engine/fuel.rs`)

Six presets (Gasoline, E85, Methanol, Diesel, Hydrogen, Nitromethane) with differing LHV, stoichiometric AFR, burn rate, spark advance, and flame color. Fuel composition (air%, fuel%, burned%) is tracked per cylinder.

### Oil and lubrication (`src/engine/oil.rs`)

`OilConfig` holds SAE grade (0W-20 through 20W-50), pump displacement, relief pressure, cooler, and pickup threshold. `OilState` tracks mass, temperature, pressure, and viscosity computed via the Andrade formula each substep. `lubrication_factor()` interpolates 0→1 between boundary and hydrodynamic regimes; it accounts for thermal breakdown above 155 °C and collapse when sump mass drops below `min_pickup_mass` (0.4 kg default). This factor is passed to both bearing and cylinder contact mechanics.

### Journal bearing physics (`src/engine/bearing.rs`)

Three bearing classes — main, rod, cam — each stepped every substep via `step_bearing()`:
1. Sommerfeld number S from viscosity, load, speed, and geometry
2. Film thickness target h_min = c·(1 − ε); asymmetric squeeze dynamics (slow squeeze-out 15 rad/s, fast draw-in 150 rad/s) prevent instantaneous film collapse under combustion spikes
3. Lubrication regime λ = h_min / 0.5 µm surface roughness; hydrodynamic at λ > 3
4. Friction blended between Petroff (hydrodynamic) and Coulomb (boundary) regimes
5. Archard wear accumulated only in boundary contact
6. Failure: **wipe-out** if shell temperature exceeds material melting point (Babbit @ 520 K); **spin** if shell_wear > 0.9 and load > 5000 N

Any bearing failure sets `EngineCore::engine_seized = true` with a human-readable reason. Additional seizure triggers: cylinder-wall temperature exceeding piston material melting point, and rod snap (`rod_damage` reaching 1.0 adds 150 Nm drag).

### Material system (`src/engine/material.rs`)

`Material` defines 7 physical properties (hardness, yield strength, friction coefficient, thermal conductivity, specific heat, density, melting point) for 9 presets (Cast Iron, Aluminum Alloy, Cast Aluminum, Forged Steel, Stock Steel, Tungsten, Titanium, Brass, Babbit). `ContactSurface::evaluate_with_lube()` computes friction force, heat, and Archard wear volume for any mating pair given a lubrication factor.

### Cylinder mechanical wear (`src/engine/cylinder.rs`)

Each `CylinderState` tracks `wall_wear` (0–1), `ring_wear` (0–1), and `rod_damage` (0–1). Compression factor = `(1 − 0.6·wall_wear − 0.4·ring_wear).max(0)` — worn seals reduce torque output. When `rod_damage` reaches 1.0 the cylinder is treated as dead (`cyl_alive = 0`).

### Dynamometer (`src/engine/dyno.rs`)

`DynoState` runs a linear RPM sweep (default 1000→8000 RPM at 400 RPM/s) using a PID loop to modulate `absorption_torque`. Forces WOT throttle and activates the oil cooler while active. Continuously samples torque and power, records `DynoSample` at each interval, and tracks peak HP and torque with their RPM.

### Wear time scale

`EngineCore::wear_time_scale` defaults to 1000.0. It multiplies the Archard wear constant so that wear that would take thousands of real hours is visible in minutes of simulation. Tunable in the UI.

### Audio (`src/audio.rs`)

Purely procedural — no samples. Exhaust pressure pulses from the physics loop are linearly interpolated to a 44.1 kHz stream, DC-blocked, low-pass filtered, and auto-gained via a peak envelope follower before entering rodio's lock-free ring buffer.

### Visuals (`src/visuals/`)

Three submodules: `parts.rs` (mesh spawning), `animate.rs` (per-frame transform updates for crank, rods, pistons, valves), `particles.rs` (VFX). Detects config changes via `config_generation` counter and fully rebuilds the scene on engine swap. Damage view mode (`core.damage_view`) renders a FEA-style heatmap (blue→red) driven by wear and temperature.

### Camera (`src/camera.rs`)

Orbit camera with critically-damped (no overshoot) yaw, pitch, and distance. RMB drag orbits; MMB/Shift+RMB pans; scroll zooms; F frames the engine. Pointer-over-egui check prevents input conflicts.

## Roadmap (`roadmap.md`)

Planned features: forced induction (turbo/supercharger), VVT/VTEC, ignition timing + knock, drivetrain (clutch/gearbox), cooling system, P-V diagram UI, and an engine builder UI for live parameter editing.
