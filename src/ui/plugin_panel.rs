use super::OfflineMapApp;
use crate::map_renderer::MapGeomFeature;
use crate::plugin::external_data::ExternalDataPlugin;
use crate::plugin::PluginParam;
use galileo::symbol::Symbol;

impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn render_plugin_section(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new("4. Data Plugins (Statistics)")
            .default_open(true)
            .show(ui, |ui| {
                let mut changed = false;

                for entry in &mut self.plugin_mgr.plugins {
                    ui.group(|ui| {
                        if ui.checkbox(&mut entry.enabled, entry.instance.name()).changed() {
                            changed = true;
                        }
                        ui.small(entry.instance.description());

                        // 如果是外部数据插件，渲染“加载 GeoJSON / CSV 文件”按钮
                        if let Some(ext_plugin) = entry
                            .instance
                            .as_any_mut()
                            .downcast_mut::<ExternalDataPlugin>()
                        {
                            ui.add_space(4.0);
                            if ui.button("Load .geojson / .csv File...").clicked() {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("GeoJSON / CSV", &["geojson", "json", "csv"])
                                    .pick_file()
                                {
                                    match ext_plugin.load_from_file(&path) {
                                        Ok(count) => {
                                            entry.enabled = true;
                                            changed = true;
                                            *self.status_msg.lock().unwrap() =
                                                format!("Loaded {} stat records", count);
                                        }
                                        Err(e) => {
                                            *self.status_msg.lock().unwrap() =
                                                format!("Plugin load error: {}", e);
                                        }
                                    }
                                }
                            }
                        }

                        if entry.enabled {
                            ui.add_space(4.0);
                            let mut keys: Vec<String> =
                                entry.instance.params_mut().keys().cloned().collect();
                            keys.sort();

                            for key in keys {
                                if let Some(param) = entry.instance.params_mut().get_mut(&key) {
                                    match param {
                                        PluginParam::IntSlider {
                                            label,
                                            value,
                                            min,
                                            max,
                                        } => {
                                            ui.horizontal(|ui| {
                                                ui.small(format!("{}:", label));
                                                if ui
                                                    .add(egui::Slider::new(value, *min..=*max))
                                                    .changed()
                                                {
                                                    changed = true;
                                                }
                                            });
                                        }
                                        PluginParam::Select {
                                            label,
                                            options,
                                            selected,
                                        } => {
                                            ui.horizontal(|ui| {
                                                ui.small(format!("{}:", label));
                                                for (idx, opt_name) in options.iter().enumerate() {
                                                    if ui
                                                        .selectable_value(selected, idx, opt_name)
                                                        .changed()
                                                    {
                                                        changed = true;
                                                    }
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    });
                }

                if changed {
                    self.plugin_dirty = true;
                }
            });
    }
}

