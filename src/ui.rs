//! egui-based control + telemetry panel.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::engine::{
    EngineCore, RunState, BORE, CRANK_RADIUS, FUELS, NUM_CYL, P_ATM, REDLINE_RPM, ROD_LENGTH,
    TOTAL_DISPLACEMENT, fuel_count,
};

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, ui_panel);
    }
}

fn ui_panel(mut ctx: EguiContexts, mut core: ResMut<EngineCore>) {
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
            rpm_bar(ui, rpm);

            ui.add_space(8.0);
            ui.separator();

            // ── Fuel selector ────────────────────────────────────────────
            ui.add_space(4.0);
            ui.label(egui::RichText::new("Fuel").strong());
            let mut idx = core.fuel_idx;
            egui::ComboBox::from_id_source("fuel_combo")
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
                "I4  {:.1}L  bore {:.0}mm  stroke {:.0}mm",
                TOTAL_DISPLACEMENT * 1000.0, BORE * 1000.0, CRANK_RADIUS * 2000.0,
            )).small());
            ui.label(egui::RichText::new(format!(
                "rod {:.0}mm  1-3-4-2  redline {:.0}",
                ROD_LENGTH * 1000.0, REDLINE_RPM,
            )).small());
        });

    // ══════════════════════════════════════════════════════════════════════════
    // RIGHT PANEL — Telemetry
    // ══════════════════════════════════════════════════════════════════════════
    egui::SidePanel::right("right_panel")
        .resizable(true)
        .default_width(280.0)
        .width_range(220.0..=380.0)
        .show(ctx.ctx_mut(), |ui| {
            ui.add_space(6.0);
            ui.heading("Telemetry");
            ui.add_space(4.0);
            ui.separator();
            ui.add_space(4.0);

            telemetry_grid(ui, &core);

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(4.0);

            // ── Per-cylinder pressure mini-bars ──────────────────────────
            ui.label(egui::RichText::new("Cylinder Pressure").strong());
            ui.add_space(2.0);
            for i in 0..NUM_CYL {
                let cyl = &core.cylinders[i];
                let pr = cyl.last_pressure / P_ATM;
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Cyl {}", i + 1))
                        .monospace().size(11.0));
                    pressure_minibar(ui, pr, cyl.temperature, cyl.flash, &core.fuel.flame_color);
                });
            }
        });
}

// ──────────────────────────────── Widgets ───────────────────────────────────
fn rpm_bar(ui: &mut egui::Ui, rpm: f32) {
    let bar_max = 9000.0;
    let frac = (rpm / bar_max).clamp(0.0, 1.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, egui::Color32::from_gray(40));
    let bar_color = if rpm > REDLINE_RPM {
        egui::Color32::from_rgb(230, 70, 70)
    } else if rpm > REDLINE_RPM - 1500.0 {
        egui::Color32::from_rgb(230, 180, 70)
    } else {
        egui::Color32::from_rgb(80, 180, 220)
    };
    let mut filled = rect;
    filled.set_width(rect.width() * frac);
    painter.rect_filled(filled, 3.0, bar_color);
    let redline_x = rect.left() + rect.width() * (REDLINE_RPM / bar_max);
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
