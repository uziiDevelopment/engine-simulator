//! egui-based control + telemetry panel.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::engine::{
    EngineCore, RunState, ENGINES, FUELS, MATERIAL_CATALOG, OIL_GRADES, P_ATM,
    engine_count, fuel_count, BearingState,
};
// Disambiguate from `bevy::prelude::Material` (a trait) — we want our
// engine-physics `Material` struct.
use crate::engine::material::Material as PhysMaterial;
use crate::visuals::EngineVisual;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ui_panel);
    }
}

fn ui_panel(
    mut ctx: EguiContexts, 
    mut core: ResMut<EngineCore>,
    mut visual_query: Query<(Entity, &Name, Option<&Children>, Option<&Parent>, &mut Visibility), With<EngineVisual>>,
) {
    let rpm = core.rpm();
    let state_text = match core.run_state {
        RunState::Off      => "OFF",
        RunState::Cranking => "CRANKING",
        RunState::Running  => "RUNNING",
    };
    let state_color = match core.run_state {
        RunState::Off      => egui::Color32::from_rgb(190, 70, 70),
        RunState::Cranking => egui::Color32::from_rgb(220, 180, 60),
        RunState::Running  => egui::Color32::from_rgb(80, 200, 100),
    };

    // ══════════════════════════════════════════════════════════════════════════
    // LEFT PANEL — Controls
    // ══════════════════════════════════════════════════════════════════════════
    egui::SidePanel::left("left_panel")
        .resizable(true)
        .default_width(260.0)
        .width_range(200.0..=360.0)
        .show(ctx.ctx_mut(), |ui| {
            egui::ScrollArea::vertical().id_salt("left_scroll").show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("Engine Control");
                ui.add_space(4.0);
                ui.separator();

                // ── Status header ────────────────────────────────────────────
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("State").strong());
                    ui.colored_label(state_color, egui::RichText::new(state_text).strong());
                });
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("RPM").strong());
                    ui.label(egui::RichText::new(format!("{:>5.0}", rpm))
                        .monospace().size(18.0).color(state_color));
                });
                rpm_bar(ui, rpm, core.config.redline_rpm);

                ui.add_space(8.0);
                ui.separator();

                // ── Engine selector ─────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Engine").strong());
                let mut eng_idx = core.config_idx;
                egui::ComboBox::from_id_salt("engine_combo")
                    .width(ui.available_width() - 8.0)
                    .selected_text(ENGINES[eng_idx].name)
                    .show_ui(ui, |ui| {
                        for i in 0..engine_count() {
                            ui.selectable_value(&mut eng_idx, i, ENGINES[i].name);
                        }
                    });
                if eng_idx != core.config_idx {
                    core.select_engine(eng_idx);
                }

                ui.add_space(4.0);
                ui.separator();

                // ── Fuel selector ────────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Fuel").strong());
                let mut idx = core.fuel_idx;
                egui::ComboBox::from_id_salt("fuel_combo")
                    .width(ui.available_width() - 8.0)
                    .selected_text(FUELS[idx].name)
                    .show_ui(ui, |ui| {
                        for i in 0..fuel_count() {
                            ui.selectable_value(&mut idx, i, FUELS[i].name);
                        }
                    });
                if idx != core.fuel_idx {
                    core.select_fuel(idx);
                }
                let f = &core.fuel;
                ui.label(egui::RichText::new(format!(
                    "LHV {:.1} MJ/kg  AFRₛ {:.1}  burn {:.0}°  ign {:.0}° BTDC",
                    f.lhv / 1e6, f.afr_stoich, f.burn_duration_deg, f.spark_advance_deg
                )).small());

                ui.add_space(8.0);
                ui.separator();

                // ── Throttle ─────────────────────────────────────────────────
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Throttle").strong());
                ui.spacing_mut().slider_width = ui.available_width() - 60.0;
                ui.add(
                    egui::Slider::new(&mut core.throttle, 0.0..=1.0)
                        .show_value(true)
                        .custom_formatter(|v, _| format!("{:>3.0}%", v * 100.0))
                        .custom_parser(|s| s.trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)),
                );

                // ── Time scale ───────────────────────────────────────────────
                ui.add_space(6.0);
                ui.label(egui::RichText::new("Time Scale").strong());
                ui.add(
                    egui::Slider::new(&mut core.time_scale, 0.02..=1.0)
                        .show_value(true).logarithmic(true)
                        .custom_formatter(|v, _| {
                            if v >= 0.995 { "1.00×".to_string() }
                            else { format!("{:.2}×", v) }
                        })
                        .custom_parser(|s| s.trim_end_matches('×').trim().parse::<f64>().ok()),
                );
                ui.horizontal(|ui| {
                    if ui.small_button("1×").clicked()    { core.time_scale = 1.0; }
                    if ui.small_button("½×").clicked()    { core.time_scale = 0.5; }
                    if ui.small_button("¼×").clicked()    { core.time_scale = 0.25; }
                    if ui.small_button("⅒×").clicked()    { core.time_scale = 0.10; }
                    if ui.small_button("1/20×").clicked() { core.time_scale = 0.05; }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Audio ────────────────────────────────────────────────────
                ui.checkbox(&mut core.audio_enabled, "Audio Simulation");
                ui.checkbox(&mut core.particles_enabled, "Gas Flow Particles");
                ui.checkbox(&mut core.damage_view, "Damage View (FEA gradient)");

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Materials & Damage ───────────────────────────────────────
                ui.collapsing(egui::RichText::new("Materials & Damage").strong(), |ui| {
                    material_selector(ui, "Block",        &mut core.config.materials.block);
                    material_selector(ui, "Cyl. Wall",    &mut core.config.materials.cylinder_wall);
                    material_selector(ui, "Piston",       &mut core.config.materials.piston);
                    material_selector(ui, "Piston Ring",  &mut core.config.materials.piston_ring);
                    material_selector(ui, "Connecting Rod", &mut core.config.materials.conrod);

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Journal Bearings").small().strong());
                    material_selector(ui, "Main Brg Shell", &mut core.config.materials.main_bearing.shell_material);
                    material_selector(ui, "Main Brg Jrnl",  &mut core.config.materials.main_bearing.journal_material);
                    material_selector(ui, "Rod Brg Shell",  &mut core.config.materials.rod_bearing.shell_material);
                    material_selector(ui, "Rod Brg Jrnl",   &mut core.config.materials.rod_bearing.journal_material);
                    material_selector(ui, "Cam Brg Shell",  &mut core.config.materials.cam_bearing.shell_material);

                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Wear Time Scale").small());
                    ui.add(
                        egui::Slider::new(&mut core.wear_time_scale, 1.0..=1.0e6)
                            .logarithmic(true)
                            .custom_formatter(|v, _| format!("{:.0}×", v))
                    );
                    ui.label(egui::RichText::new(
                        "Multiplies the Archard wear constant. 1× = realistic-slow, \
                         1e4×+ = damage in seconds of abuse.",
                    ).small().weak());

                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Lubrication").small().strong());
                    
                    let g_idx = core.oil_config.grade_idx;
                    let selected_text = if core.oil_config.custom_grade { "Custom" } else { OIL_GRADES[g_idx].name };
                    
                    egui::ComboBox::from_id_salt("oil_grade_combo")
                        .width(ui.available_width() - 8.0)
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            for (i, grade) in OIL_GRADES.iter().enumerate() {
                                let is_selected = !core.oil_config.custom_grade && g_idx == i;
                                if ui.selectable_label(is_selected, grade.name).clicked() {
                                    core.oil_config.grade_idx = i;
                                    core.oil_config.custom_grade = false;
                                    core.oil_config.v40 = grade.v40;
                                    core.oil_config.v100 = grade.v100;
                                    core.oil_config.recalculate_constants();
                                }
                            }
                        });
                    
                    if !core.oil_config.custom_grade {
                        ui.label(egui::RichText::new(OIL_GRADES[g_idx].description).small().weak());
                    } else {
                        ui.label(egui::RichText::new("Custom viscosity profile").small().weak());
                    }

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("40°C").small());
                        if ui.add(egui::Slider::new(&mut core.oil_config.v40, 5.0..=400.0).suffix(" cSt")).changed() {
                            core.oil_config.custom_grade = true;
                            core.oil_config.recalculate_constants();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new("100°C").small());
                        if ui.add(egui::Slider::new(&mut core.oil_config.v100, 2.0..=60.0).suffix(" cSt")).changed() {
                            core.oil_config.custom_grade = true;
                            core.oil_config.recalculate_constants();
                        }
                    });

                    ui.add(
                        egui::Slider::new(&mut core.oil_config.viscosity_multiplier, 0.5..=2.0)
                            .text("Tuning Mult.")
                    );

                    // Update current state viscosity immediately so telemetry stays in sync
                    core.oil.viscosity = crate::engine::viscosity_for(
                        core.oil.temperature,
                        core.oil_config.viscosity_a,
                        core.oil_config.viscosity_b,
                        core.oil_config.viscosity_multiplier,
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Drain Oil").clicked() {
                            core.oil.drain();
                        }
                        if ui.button("Refill Oil").clicked() {
                            let cfg = core.oil_config.clone();
                            core.oil.refill(&cfg);
                        }
                    });
                    if ui.button("Reset Damage (heal everything)").clicked() {
                        for c in core.cylinders.iter_mut() {
                            c.wall_wear = 0.0;
                            c.ring_wear = 0.0;
                            c.rod_damage = 0.0;
                            c.block_temp = 290.0;
                            c.piston_temp = 290.0;
                        }
                        for b in core.main_bearings.iter_mut() { *b = BearingState::fresh(); }
                        for b in core.rod_bearings.iter_mut()  { *b = BearingState::fresh(); }
                        for b in core.cam_bearings.iter_mut()  { *b = BearingState::fresh(); }
                        core.engine_seized = false;
                        let cfg = core.oil_config.clone();
                        core.oil.refill(&cfg);
                    }
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Controls + spec ──────────────────────────────────────────
                ui.label(egui::RichText::new("Controls").strong());
                ui.label("E — starter motor");
                ui.label("RMB — orbit");
                ui.label("MMB — pan");
                ui.label("Scroll — zoom");
                ui.label("F — frame engine");

                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new(format!(
                    "{:.1}L  bore {:.0}mm  stroke {:.0}mm",
                    core.config.total_displacement() * 1000.0,
                    core.config.bore * 1000.0,
                    core.config.stroke * 1000.0,
                )).small());
                ui.label(egui::RichText::new(format!(
                    "rod {:.0}mm  {} cyl  redline {:.0}",
                    core.config.rod_length * 1000.0,
                    core.config.num_cylinders,
                    core.config.redline_rpm,
                )).small());

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Render Tree").strong());
                ui.add_space(2.0);

                // Build a temporary tree to avoid borrow checker issues with recursion + &mut Query
                let mut roots = Vec::new();
                for (entity, _, _, parent, _) in visual_query.iter() {
                    let is_root = parent.map_or(true, |p| !visual_query.contains(p.get()));
                    if is_root {
                        roots.push(entity);
                    }
                }
                roots.sort_by_cached_key(|e| visual_query.get(*e).map(|(_, n, _, _, _)| n.to_string()).unwrap_or_default());

                for root in roots {
                    draw_render_tree(ui, root, &mut visual_query);
                }
            });
        });

    // ══════════════════════════════════════════════════════════════════════════
    // RIGHT PANEL — Telemetry
    // ══════════════════════════════════════════════════════════════════════════
    egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(280.0)
        .width_range(220.0..=380.0)
        .show(ctx.ctx_mut(), |ui| {
            egui::ScrollArea::vertical().id_salt("right_scroll").show(ui, |ui| {
                ui.add_space(6.0);
                ui.heading("Telemetry");
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);

                telemetry_grid(ui, &core);

                ui.add_space(6.0);

                // ── Seized banner ────────────────────────────────────────────
                if core.engine_seized {
                    let rect_resp = ui.allocate_response(
                        egui::vec2(ui.available_width(), 30.0),
                        egui::Sense::hover(),
                    );
                    let painter = ui.painter();
                    painter.rect_filled(rect_resp.rect, 4.0, egui::Color32::from_rgb(170, 30, 30));
                    painter.text(
                        rect_resp.rect.center(),
                        egui::Align2::CENTER_CENTER,
                        "ENGINE SEIZED — Reset Damage to recover",
                        egui::FontId::proportional(14.0),
                        egui::Color32::WHITE,
                    );
                    ui.add_space(6.0);
                }

                // ── Oil gauges ───────────────────────────────────────────────
                ui.label(egui::RichText::new("Lubrication").strong());
                let oil_kpa = core.oil.pressure / 1000.0;
                let oil_pct_full = (core.oil.mass / core.oil_config.capacity).clamp(0.0, 1.0);
                let oil_temp_c = core.oil.temperature - 273.15;
                let lube = core.oil.lubrication_factor(&core.oil_config);
                egui::Grid::new("oil_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    cell(ui, "Oil press.", &format!("{:>5.1} kPa", oil_kpa));
                    cell(ui, "Oil temp",   &format!("{:>5.1} °C",  oil_temp_c));
                    ui.end_row();
                    cell(ui, "Sump",       &format!("{:>3.0}%",    oil_pct_full * 100.0));
                    cell(ui, "Lube",       &format!("{:>3.0}%",    lube * 100.0));
                    ui.end_row();
                    cell(ui, "Visc.",      &format!("{:.3} Pa·s",  core.oil.viscosity));
                    cell(ui, "VI Slope",   &format!("{:.0}",       core.oil_config.viscosity_b));
                    ui.end_row();
                    cell(ui, "Frict. Q",   &format!("{:>4.0} W",   core.friction_heat_smoothed));
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Per-cylinder pressure mini-bars ──────────────────────────
                ui.label(egui::RichText::new("Cylinder Pressure").strong());
                ui.add_space(2.0);
                for i in 0..core.num_cyl() {
                    let cyl = &core.cylinders[i];
                    let pr = cyl.last_pressure / P_ATM;
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cyl {}", i + 1))
                            .monospace().size(11.0));
                        pressure_minibar(ui, pr, cyl.temperature, cyl.flash, &core.fuel.flame_color);
                    });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Per-cylinder wear / temp ─────────────────────────────────
                ui.label(egui::RichText::new("Damage").strong());
                ui.add_space(2.0);
                for i in 0..core.num_cyl() {
                    let cyl = &core.cylinders[i];
                    let rod_brg_wear = core.rod_bearings.get(i)
                        .map(|b| b.shell_wear).unwrap_or(0.0);
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cyl {}", i + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Wall", cyl.wall_wear);
                        wear_minibar(ui, "Ring", cyl.ring_wear);
                        wear_minibar(ui, "Rod",  cyl.rod_damage);
                        wear_minibar(ui, "Brg",  rod_brg_wear);
                    });
                    let brg_temp = core.rod_bearings.get(i)
                        .map(|b| b.temperature).unwrap_or(290.0);
                    ui.label(egui::RichText::new(format!(
                        "    block {:.0} K   piston {:.0} K   brg {:.0} K   \u{03c3} {:.0} MPa",
                        cyl.block_temp, cyl.piston_temp, brg_temp, cyl.last_rod_stress / 1.0e6,
                    )).small().monospace());
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                // ── Main & cam bearings ──────────────────────────────────────
                ui.label(egui::RichText::new("Journal Bearings").strong());
                ui.add_space(2.0);

                for (idx, brg) in core.main_bearings.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Main {}", idx + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Shell", brg.shell_wear);
                        let status = if brg.spun { " SPUN" }
                            else if brg.wiped { " WIPED" }
                            else { "" };
                        ui.label(egui::RichText::new(format!(
                            "{:.0} K  S={:.2}{}",
                            brg.temperature, brg.sommerfeld, status,
                        )).small().monospace());
                    });
                }

                for (idx, brg) in core.cam_bearings.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("Cam  {}", idx + 1))
                            .monospace().size(11.0));
                        wear_minibar(ui, "Shell", brg.shell_wear);
                        let status = if brg.spun { " SPUN" }
                            else if brg.wiped { " WIPED" }
                            else { "" };
                        ui.label(egui::RichText::new(format!(
                            "{:.0} K  S={:.2}{}",
                            brg.temperature, brg.sommerfeld, status,
                        )).small().monospace());
                    });
                }
            });
        });
}

// ──────────────────────────────── Widgets ───────────────────────────────────
fn rpm_bar(ui: &mut egui::Ui, rpm: f32, redline: f32) {
    let bar_max = (redline * 1.15).max(1000.0);
    let frac = (rpm / bar_max).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(40));
    let bar_color = if rpm > redline {
        egui::Color32::from_rgb(230, 70, 70)
    } else if rpm > redline - 1500.0 {
        egui::Color32::from_rgb(230, 180, 70)
    } else {
        egui::Color32::from_rgb(80, 180, 220)
    };
    let mut filled = rect;
    filled.set_width(rect.width() * frac);
    painter.rect_filled(filled, 3.0, bar_color);
    let redline_x = rect.left() + rect.width() * (redline / bar_max);
    painter.line_segment(
        [egui::pos2(redline_x, rect.top()), egui::pos2(redline_x, rect.bottom())],
        egui::Stroke::new(1.5, egui::Color32::from_rgb(255, 80, 80)),
    );
}

fn telemetry_grid(ui: &mut egui::Ui, core: &EngineCore) {
    let map_kpa = core.map_smoothed / 1000.0;
    let exh_kpa = core.exhaust_pressure_smoothed / 1000.0;
    let kw = (core.power_smoothed / 1000.0).max(0.0);
    let nm = core.torque_smoothed.max(0.0);
    let afr = core.afr_smoothed;

    egui::Grid::new("telemetry").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        cell(ui, "Torque",     &format!("{:>5.1} Nm", nm));
        cell(ui, "Power",      &format!("{:>5.1} kW",  kw));
        ui.end_row();
        cell(ui, "MAP",        &format!("{:>5.1} kPa", map_kpa));
        cell(ui, "Exh. press", &format!("{:>5.1} kPa", exh_kpa));
        ui.end_row();
        cell(ui, "AFR",        &format!("{:>5.2}", afr));
        cell(ui, "Exh. temp",  &format!("{:>5.0} K", core.exhaust_temp_smoothed));
        ui.end_row();
    });
}

fn cell(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small());
        ui.label(egui::RichText::new(value).monospace());
    });
}

fn pressure_minibar(ui: &mut egui::Ui, pressure_ratio: f32, temp_k: f32, flash: f32, flame: &[f32; 3]) {
    let max_ratio = 80.0;
    let frac = (pressure_ratio / max_ratio).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(180.0, 12.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(36));

    // Heat tint scales from blue (cool) to orange (hot)
    let hot = ((temp_k - 350.0) / 2200.0).clamp(0.0, 1.0);
    let r = (60.0 + 195.0 * hot) as u8;
    let g = (140.0 - 90.0 * hot) as u8;
    let b = (210.0 - 180.0 * hot) as u8;
    let mut filled = rect;
    filled.set_width(rect.width() * frac);
    painter.rect_filled(filled, 2.0, egui::Color32::from_rgb(r, g, b));

    if flash > 0.05 {
        // Flash overlay tinted by fuel flame colour
        let a = (flash * 230.0).clamp(0.0, 255.0) as u8;
        painter.rect_filled(filled, 2.0, egui::Color32::from_rgba_unmultiplied(
            (flame[0] * 255.0) as u8, (flame[1] * 255.0) as u8, (flame[2] * 255.0) as u8, a,
        ));
    }
    ui.label(egui::RichText::new(format!("{:>5.1} bar  {:>4.0} K", pressure_ratio, temp_k)).monospace().small());
}

fn draw_render_tree(
    ui: &mut egui::Ui,
    entity: Entity,
    query: &mut Query<(Entity, &Name, Option<&Children>, Option<&Parent>, &mut Visibility), With<EngineVisual>>,
) {
    // 1. Collect info and current state, then drop the borrow immediately
    let (name, is_visible, child_ids) = {
        let Ok((_, name, children, _, vis)) = query.get_mut(entity) else { return; };
        (
            name.to_string(),
            matches!(*vis, Visibility::Inherited | Visibility::Visible),
            children.map(|c| c.iter().cloned().collect::<Vec<_>>()).unwrap_or_default(),
        )
    };

    let mut new_visible = is_visible;

    ui.horizontal(|ui| {
        if ui.checkbox(&mut new_visible, "").changed() {
            if let Ok((_, _, _, _, mut vis)) = query.get_mut(entity) {
                *vis = if new_visible { Visibility::Inherited } else { Visibility::Hidden };
            }
        }

        if !child_ids.is_empty() {
            let id = ui.make_persistent_id(entity);
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false);
            
            // We must wrap the collapsing header in a vertical layout because
            // its body (indentation) only works in vertical layouts.
            ui.vertical(|ui| {
                state.show_header(ui, |ui| {
                    ui.label(&name);
                }).body(|ui| {
                    for child in child_ids {
                        draw_render_tree(ui, child, query);
                    }
                });
            });
        } else {
            ui.label(&name);
        }
    });
}

// ──────────────── Per-part material picker ──────────────────────────────────
fn material_selector(ui: &mut egui::Ui, label: &str, current: &mut PhysMaterial) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{:<13}", label)).small().monospace());
        let combo_id = format!("mat_{}", label);
        egui::ComboBox::from_id_salt(combo_id)
            .width(150.0)
            .selected_text(current.name)
            .show_ui(ui, |ui| {
                for m in MATERIAL_CATALOG {
                    let selected = current.name == m.name;
                    if ui.selectable_label(selected, m.name).clicked() {
                        *current = (*m).clone();
                    }
                }
            });
    });
}

// ──────────────── 0..1 damage bar in the FEA gradient ───────────────────────
fn wear_minibar(ui: &mut egui::Ui, tag: &str, value: f32) {
    let v = value.clamp(0.0, 1.0);
    ui.label(egui::RichText::new(tag).small().monospace());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(48.0, 10.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(36));
    let mut filled = rect;
    filled.set_width(rect.width() * v);
    let (r, g, b) = jet_egui(v);
    painter.rect_filled(filled, 2.0, egui::Color32::from_rgb(r, g, b));
}

fn jet_egui(t: f32) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    let r = (1.5 - (4.0 * t - 3.0).abs()).clamp(0.0, 1.0);
    let g = (1.5 - (4.0 * t - 2.0).abs()).clamp(0.0, 1.0);
    let b = (1.5 - (4.0 * t - 1.0).abs()).clamp(0.0, 1.0);
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
}
