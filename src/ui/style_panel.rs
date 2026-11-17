use super::OfflineMapApp;
use crate::map_renderer::MapGeomFeature;
use crate::style_config::{MapStyleConfig, RgbaColor, STYLE_CONFIG_PATH};
use galileo::symbol::Symbol;

fn color_picker_row(ui: &mut egui::Ui, label: &str, color: &mut RgbaColor) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut arr = color.to_egui_array();
        if ui.color_edit_button_srgba_unmultiplied(&mut arr).changed() {
            *color = RgbaColor::from_egui_array(arr);
            changed = true;
        }
        ui.label(label);
    });
    changed
}

impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    fn refresh_all_layer_styles(&mut self) {
        self.layers.ocean_bg.write().update_all_features();
        self.layers.land_base.write().update_all_features();
        self.layers.water.write().update_all_features();
        self.layers.water_line.write().update_all_features();
        self.layers.park.write().update_all_features();
        self.layers.building.write().update_all_features();
        self.layers.minor_road.write().update_all_features();
        self.layers.major_road.write().update_all_features();
        self.layers.gps_track_layer.write().update_all_features();
        self.layers.route_line.write().update_all_features();
        self.map_state.map_mut().redraw();
    }

    pub fn render_style_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("3. Map Style & Theme")
            .default_open(false)
            .show(ui, |ui| {
                let mut dirty = false;

                // 一键切换预设主题
                ui.horizontal(|ui| {
                    ui.small("Presets:");
                    if ui.small_button("Light").clicked() {
                        *self.style.write() = MapStyleConfig::preset_light();
                        dirty = true;
                    }
                    if ui.small_button("Dark").clicked() {
                        *self.style.write() = MapStyleConfig::preset_dark();
                        dirty = true;
                    }
                    if ui.small_button("Retro").clicked() {
                        *self.style.write() = MapStyleConfig::preset_retro();
                        dirty = true;
                    }
                });

                ui.add_space(4.0);

                // 各图层颜色拾色器与线宽微调
                {
                    let mut cfg = self.style.write();
                    dirty |= color_picker_row(ui, "Ocean", &mut cfg.ocean_color);
                    dirty |= color_picker_row(ui, "Land Base", &mut cfg.land_color);
                    dirty |= color_picker_row(ui, "Water / Lakes", &mut cfg.water_fill);
                    dirty |= color_picker_row(ui, "Rivers / Coast", &mut cfg.water_line_color);
                    dirty |= color_picker_row(ui, "Parks / Forests", &mut cfg.park_fill);
                    dirty |= color_picker_row(ui, "Buildings", &mut cfg.building_fill);
                    dirty |= color_picker_row(ui, "Major Roads", &mut cfg.major_road_color);
                    dirty |= color_picker_row(ui, "Minor Roads", &mut cfg.minor_road_color);
                    dirty |= color_picker_row(ui, "Route Line", &mut cfg.route_color);
                    dirty |= color_picker_row(ui, "GPS Track", &mut cfg.gps_track_color);

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.small("Major Width:");
                        if ui.add(egui::Slider::new(&mut cfg.major_road_width, 1.0..=8.0)).changed() {
                            dirty = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.small("Minor Width:");
                        if ui.add(egui::Slider::new(&mut cfg.minor_road_width, 0.5..=5.0)).changed() {
                            dirty = true;
                        }
                    });
                }

                if dirty {
                    self.refresh_all_layer_styles();
                }

                ui.add_space(4.0);
                // 配置文件读写按钮 (map_style.json)
                ui.horizontal(|ui| {
                    if ui.small_button("Save JSON").clicked() {
                        let cfg = self.style.read().clone();
                        match cfg.save_to_file(STYLE_CONFIG_PATH) {
                            Ok(_) => *self.status_msg.lock().unwrap() = format!("Saved {}", STYLE_CONFIG_PATH),
                            Err(e) => *self.status_msg.lock().unwrap() = format!("Save error: {}", e),
                        }
                    }
                    if ui.small_button("Reload JSON").clicked() {
                        *self.style.write() = MapStyleConfig::load_or_create(STYLE_CONFIG_PATH);
                        self.map_state.map_mut().redraw();
                        *self.status_msg.lock().unwrap() = format!("Reloaded {}", STYLE_CONFIG_PATH);
                    }
                });
            });
    }
}

