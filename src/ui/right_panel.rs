use super::OfflineMapApp;
use crate::map_renderer::MapGeomFeature;
use crate::pathfinder::TravelMode;
use galileo::symbol::Symbol;

impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn render_right_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("itinerary_panel")
            .resizable(true)
            .default_width(350.0)
            .show(ctx, |ui| {
                ui.heading("Itinerary, Notes & Photos");
                ui.separator();

                ui.horizontal(|ui| {
                    ui.label("Day:");
                    for d in 1..=5 {
                        if ui.selectable_value(&mut self.selected_day, d, format!("D{}", d)).changed() {
                            self.route_dirty = true;
                        }
                    }
                });

                ui.horizontal(|ui| {
                    ui.label("Start:");
                    if ui.add(egui::TextEdit::singleline(&mut self.day_start_time).desired_width(48.0)).changed() {
                        self.route_dirty = true;
                    }
                    if ui.selectable_value(&mut self.travel_mode, TravelMode::Driving, "Drive").changed() {
                        self.route_dirty = true;
                    }
                    if ui.selectable_value(&mut self.travel_mode, TravelMode::Walking, "Walk").changed() {
                        self.route_dirty = true;
                    }
                });

                ui.separator();

                let day_indices: Vec<usize> = self
                    .stops
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.day_index == self.selected_day)
                    .map(|(idx, _)| idx)
                    .collect();

                if day_indices.is_empty() {
                    ui.colored_label(egui::Color32::GRAY, "Right-click map or click a POI to add stops.");
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut action_del = None;
                    let mut action_swap = None;

                    for (seq, &idx) in day_indices.iter().enumerate() {
                        if seq > 0 {
                            ui.small(format!(
                                "   ↓ {:.2} km (~{:.0} min)",
                                self.stops[idx].travel_km_from_prev, self.stops[idx].travel_min_from_prev
                            ));
                        }

                        let is_sel = self.selected_stop_id == Some(self.stops[idx].id);
                        ui.group(|ui| {
                            let title = format!(
                                "#{} [{} - {}] {}",
                                seq + 1,
                                self.stops[idx].computed_arrival,
                                self.stops[idx].computed_departure,
                                self.stops[idx].name
                            );
                            if ui.selectable_label(is_sel, title).clicked() {
                                self.selected_stop_id = Some(self.stops[idx].id);
                            }

                            ui.horizontal(|ui| {
                                ui.label("Stay:");
                                if ui
                                    .add(egui::DragValue::new(&mut self.stops[idx].stay_minutes).range(0..=720).speed(5))
                                    .changed()
                                {
                                    let _ = self.note_mgr.update_stop(&self.stops[idx]);
                                    self.route_dirty = true;
                                }
                                if seq > 0 && ui.small_button("↑").clicked() {
                                    action_swap = Some((day_indices[seq - 1], idx));
                                }
                                if seq + 1 < day_indices.len() && ui.small_button("↓").clicked() {
                                    action_swap = Some((idx, day_indices[seq + 1]));
                                }
                                if ui.small_button("Del").clicked() {
                                    action_del = Some(idx);
                                }
                            });
                        });
                    }

                    if let Some((a, b)) = action_swap {
                        let tmp = self.stops[a].order_idx;
                        self.stops[a].order_idx = self.stops[b].order_idx;
                        self.stops[b].order_idx = tmp;
                        self.stops.swap(a, b);
                        let _ = self.note_mgr.update_stop(&self.stops[a]);
                        let _ = self.note_mgr.update_stop(&self.stops[b]);
                        self.route_dirty = true;
                    }

                    if let Some(del_idx) = action_del {
                        let sid = self.stops[del_idx].id;
                        let _ = self.note_mgr.remove_stop(sid, &self.photo_mgr);
                        self.stops.remove(del_idx);
                        if self.selected_stop_id == Some(sid) {
                            self.selected_stop_id = None;
                        }
                        self.route_dirty = true;
                    }

                    // 选中站点的手记与照片管理详情区
                    if let Some(sel_id) = self.selected_stop_id {
                        if let Some(s_idx) = self.stops.iter().position(|s| s.id == sel_id) {
                            ui.separator();
                            ui.heading("Notes & Photos");
                            if ui.text_edit_singleline(&mut self.stops[s_idx].name).changed() {
                                let _ = self.note_mgr.update_stop(&self.stops[s_idx]);
                            }
                            if ui
                                .add(
                                    egui::TextEdit::multiline(&mut self.stops[s_idx].notes)
                                        .desired_rows(4)
                                        .desired_width(f32::INFINITY),
                                )
                                .changed()
                            {
                                let _ = self.note_mgr.update_stop(&self.stops[s_idx]);
                            }
                            if ui.button("+ Add Photos...").clicked() {
                                if let Some(files) = rfd::FileDialog::new()
                                    .add_filter("Images", &["jpg", "jpeg", "png", "webp"])
                                    .pick_files()
                                {
                                    let sid = self.stops[s_idx].id;
                                    for f in files {
                                        if let Ok(rec) = self.photo_mgr.import_photo(sid, &f) {
                                            self.stops[s_idx].photos.push(rec);
                                        }
                                    }
                                }
                            }
                            let photos = self.stops[s_idx].photos.clone();
                            for p in photos {
                                if !p.taken_at.is_empty() {
                                    ui.small(format!("EXIF: {}", p.taken_at));
                                }
                                if let Some(tex) = self.photo_mgr.get_thumbnail(ctx, &p.file_path) {
                                    let sz = tex.size_vec2();
                                    let scale = (280.0 / sz.x).min(1.0);
                                    ui.image((tex.id(), sz * scale));
                                }
                            }
                        }
                    }
                });
            });
    }
}

