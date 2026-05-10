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

A real-time combustion engine simulator built on **Bevy 0.14** (ECS), with procedural audio via **rodio** and an immediate-mode UI via **bevy_egui**.

`main.rs` registers five plugins in dependency order:
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

### Engine configurations (`src/engine/config.rs`)

Four presets, all dynamically switchable at runtime (entire 3D engine is despawned and rebuilt by `VisualsPlugin` on change):
- **Inline-4** — 2.0L, 86×86 mm, 8000 RPM redline
- **Inline-5** — 2.5L, 82.5×92.8 mm, 7200 RPM redline
- **V8** — 5.0L, 90° bank, 8000 RPM redline
- **Flat-6** — 3.8L, 180° boxer, 7200 RPM redline

Each `EngineConfig` includes a `MaterialsConfig` that assigns a `Material` (hardness, yield strength, friction, thermal conductivity, density, melting point) to block, cylinder wall, piston, piston rings, conrod, and each bearing class.

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

Dynamically spawns meshes for crank, connecting rods, pistons, and valves. Includes a particle system (`visuals/particles.rs`) for intake (blue), exhaust (orange), and combustion (fuel flame color) flow visualization. A damage view mode (`core.damage_view`) renders a FEA-style heatmap (blue→red) driven by wear and temperature.

## Roadmap (`roadmap.md`)

Planned features: forced induction, VVT/VTEC, ignition timing control, cooling system, and P-V diagram UI. Dyno and oil system are now implemented.
