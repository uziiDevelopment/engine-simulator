High-impact (will visibly change torque/RPM/sound behavior)

  - No knock / end-gas autoignition. Compression ratio and fuel octane have zero effect — high-CR setups don't penalize
  low-octane fuel. (cylinder.rs, combustion path)
  - No combustion-efficiency vs AFR curve. All trapped fuel × LHV is released regardless of λ (cylinder.rs:571). Rich/lean
  mixtures should drop efficiency sharply.
  - No port flow-coefficient table. Valve discharge uses fixed Cd=0.7 (valve.rs:77); real Cd varies 0.3–0.75 with L/D. Major
  VE-curve realism hole.
  - Single 0-D plenum per side. No runner-per-cylinder, no Helmholtz/wave tuning, no inter-runner interaction
  (manifold.rs:14-22). Intake slug is per-cyl but all share one manifold pressure.
  - No exhaust runner inertance / pressure waves. Pure quasi-static orifice (cylinder.rs:492). No scavenging, no tuned
  headers.
  - Single rigid crank — no torsional vibration, no dual-mass damper, no firing-order harmonics on flywheel (engine.rs:471).
  - Ambient is hardcoded P=101325, T=295 (thermo.rs:9-10). No altitude/IAT/humidity.

  Medium-impact

  Thermodynamics
  - No residual gas fraction tracked across cycles (affects knock margin, burn rate).
  - Woschni velocity term missing the combustion (p − p_motored) component — heat transfer doesn't spike during burn.
  - No blowby gas path; oil consumption is faked from ring wear without venting cylinder mass or crankcase pressurization.
  - No charge-cooling from fuel evaporation (methanol/E85 should cool intake 20–40 K).
  - Wiebe shape constant across SI fuels; burn duration only modulated by mean piston speed — no AFR/EGR/density coupling.
  - Wall heat uses lumped block_temp only; piston-crown temp computed but unused in q_wall.

  Gas exchange / valves
  - Sinusoidal cam profile is non-physical (no ramps, no dwell). Real cams have flank acceleration limits.
  - No valve float / spring dynamics — high-RPM VE collapse comes only from runner inertia.
  - Intake uses slug-inertia model, exhaust uses quasi-static — inconsistent.
  - No manifold-wall heat transfer (intake stays cold forever; exhaust Newton-cools to 600 K floor).

  Mechanical / friction
  - FRICTION_BASE = 12 Nm constant for I4 through W16 (crank.rs:9) on the legacy path.
  - Oil-pump parasitic torque never subtracted from crank (oil.rs:202-209) — pumps consume 0.5–2 kW real.
  - No IMEP/FMEP/PMEP split telemetry; pumping work implicit only.
  - Rod force is gas + |inertia| (cylinder.rs:815) — always positive; loses tension/compression alternation and over-loads at
  TDC firing where inertia opposes gas.
  - Main bearing load distributes total torque/r evenly across mains, ignores firing order and counterweight phasing.
  - No primary/secondary balance — I4 secondary shake and V6 rocking invisible.

  Fuel / combustion
  - No injector model: no duty-cycle saturation, no dead time, no minimum pulse width.
  - No injection timing; port- vs direct-injection indistinguishable.
  - Diesel runs through SI spark+Wiebe path with spark_advance_deg=8 (fuel.rs:88); no ignition delay or premix/diffusion
  split.
  - Spark advance is RPM-only ramp; no load axis, no knock-retard, no MBT.

  Bearings / oil
  - Bearing oil temperature = bulk oil temp (bearing.rs:289); local film ΔT (30–80 K) absent — feeds back into viscosity.
  - Pump pressure = flow·µ·resistance only (oil.rs:204); no turbulent/orifice term, relief curve wrong at extremes.

  Wear coupling
  - compression_factor = 1 − 0.6·wall_wear − 0.4·ring_wear is applied post-hoc to gas torque (engine.rs:202,221), not to
  blowby mass during the cycle. Worn rings don't show up in the actual pressure trace.

  Integration
  - engine_step uses variable delta_seconds() × 80 substeps (engine.rs:104) — dynamics become frame-rate dependent under
  stutter.
  - overheat_penalty is a flat torque multiplier (engine.rs:408) instead of acting through VE/knock/friction.

  Low-impact (cosmetic or rare-condition)

  - No EGR system, no PCV, no air-filter ΔP.
  - Tailpipe is one orifice — no muffler/resonator back-pressure curve.
  - Throttle area uses square-law approximation, no proper cosine-of-plate-angle.
  - Single intake/exhaust valve per cylinder assumed; no 4-valve scaling.
  - Ring tension constant 60 N independent of bore (cylinder.rs:753).
  - Secondary inertia term cos(2θ)·r/L missing from piston side-thrust (cylinder.rs:748).
  - Starter cutoff hardcoded at 600 RPM on the legacy path, bypassing config.
  - No oil aeration/foaming; viscosity clamps at 20 Pa·s (cold-crank unrealistic).
  - Single bulk oil temp — no sump/galley/bearing-feed split.
  - Audio derives from raw manifold pressure pulses — no firing-order/harmonic-comb shaping; engines distinguishable only by
  cylinder pulse pattern.
  - No per-cylinder clearance-volume mismatch; single CR for whole engine; no head-gasket leak.
  - Temperature clamp 180–4500 K silently caps nitromethane flame temps.
  - No misfire even when lubrication and AFR are both pathological.

  Priorities if you tackle this

  If you want the biggest realism gain per line of code, in order:
  1. AFR-dependent combustion efficiency (one curve in cylinder.rs).
  2. Port flow-coefficient lift table replacing fixed Cd=0.7.
  3. Knock model coupled to CR + fuel octane + end-gas T (enables CR/fuel choices to matter).
  4. Exposing ambient T/P/altitude to UI and routing through manifolds.
  5. Fix rod-force directionality (gas vs inertia signed sum) — bearing load math is currently inflated near TDC firing.

  Everything else (runners-per-cylinder gas dynamics, torsional crank, VVT, forced induction) is bigger surgery and overlaps
  with your existing roadmap.