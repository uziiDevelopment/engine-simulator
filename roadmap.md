# Engine Simulator Roadmap

## 1. Forced Induction (Turbochargers & Superchargers)
Currently, your engine breathes at atmospheric pressure. Adding forced induction is a great thermodynamics challenge:
*   **Superchargers:** Model a compressor driven directly by the crankshaft via a belt (drains crank torque to increase intake manifold pressure).
*   **Turbochargers:** Model an exhaust turbine driven by exhaust gas pressure/temperature, which spins an intake compressor. You'll need to model the inertia of the turbine wheel, wastegates to prevent over-boosting, and blow-off valves.

## 2. Variable Valve Timing (VVT) & VTEC
Your `EngineConfig` currently has fixed `intake_open_deg`, `intake_close_deg`, and lift parameters. You could implement:
*   **Continuous VVT:** Slowly shift the cam phase degrees based on RPM and load to optimize volumetric efficiency across the rev range.
*   **Variable Lift (VTEC):** Define two different cam profiles (e.g., "economy" and "power") and dynamically switch between them when a certain RPM threshold is reached.

## 3. Ignition Timing & Spark Advance
Instead of combustion happening at a fixed ideal time, add a programmable spark map.
*   Allow the user (or an automated ECU) to adjust spark advance (e.g., 20° Before Top Dead Center).
*   *Bonus:* Implement "engine knock" (detonation) if the ignition is advanced too far for the current cylinder pressure and fuel octane rating, causing negative torque spikes or engine damage.

## 4. Drivetrain & Vehicle Load (Dyno Mode)
Right now, the engine only spins against its own internal friction and inertia.
*   Add a **Dynamometer Mode** that applies a variable load to hold the engine at specific RPMs, allowing you to plot a real Torque/Horsepower curve on a graph in the UI.
*   Add a **Vehicle Simulation**: Connect the crankshaft to a simulated and visual clutch, gearbox, differential, and vehicle mass. This will let you simulate 0-60 pulls, shift gears, and see how the engine handles actual mechanical load.
*   Add a **Gearbox**: Add a manual gearbox with any number of gears to the drivetrain simulation. Include options to:
    *   Shift gears manually
    *   Simulate gear changes (clutch disengagement, gear engagement, clutch re-engagement)
    *   Show current gear in UI
    *   Provide visual feedback for gear changes (visual gear shifter)
    *   Implement gear ratios for each gear
    *   Implement reverse gear
    *   Implement neutral gear
    *   Implement automatic gear shifting based on engine RPM and load
    *   Implement launch control

## 5. Procedural Engine Audio
Since you are modeling the exhaust pressure pulses out of the tailpipe in real-time, you can use those exact pressure spikes to procedurally generate engine audio!
*   Feed the `exhaust.pressure()` changes directly into an audio buffer. As the RPM increases and the firing order interleaves, it will naturally produce the exact roar of an Inline-4, V8, or Flat-6 without needing a single pre-recorded sound sample.

## 6. Heat Transfer & Cooling System
You are calculating thermodynamics, but the cylinder blocks don't get hot yet.
*   Model heat transfer from the combusting gas to the cylinder walls.
*   Add a coolant loop with a radiator and thermostat. If the engine overheats, increase the friction coefficient or reduce power to simulate thermal expansion and binding.

## 7. UI Enhancements
*   **P-V Diagrams:** Draw a real-time Pressure-Volume diagram in the UI for one of the cylinders. This is a classic thermodynamics visualization that shows the "work" being done by the engine cycle.
*   **Engine Builder UI:** Since you have a `build_engine` function for dynamic generation, add a UI panel where the user can drag sliders to change bore, stroke, cylinder count, and layout, and instantly see the engine rebuild and behave differently.

exhaust headers and intake manifolds with dynamic pathing