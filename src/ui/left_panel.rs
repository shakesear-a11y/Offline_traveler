use super::{OfflineMapApp, DB_PATH};
use crate::map_renderer::{
    add_custom_poi, clear_gps_tracks, delete_custom_poi, import_gpx_track,
    render_gps_tracks_on_layer, MapGeomFeature,
};
use crate::osm_index::import_pbf_to_sqlite;
use galileo::symbol::Symbol;
use galileo_types::latlon;
use std::sync::Arc;
use std::time::Instant;

impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn render_left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("control_panel")
            .resizable(true)
            .default_width(265.0)
            .show(ctx, |ui| {
                ui.heading("Offline Vector Map");
                ui.separator();

                let importing = *self.is_importing.lock().unwrap();
                if ui
                    .add_enabled(!importing, egui::Button::new("1. Import .osm.pbf Index..."))
                    .clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("OSM PBF", &["pbf"])
                        .pick_file()
                    {
                        *self.is_importing.lock().unwrap() = true;
                        *self.status_msg.lock().unwrap() =
                            format!("Indexing {:?}...", path.file_name().unwrap_or_default());

                        let status_clone = Arc::clone(&self.status_msg);
                        let importing_clone = Arc::clone(&self.is_importing);
                        let recenter_clone = Arc::clone(&self.pending_recenter);
                        let ctx_clone = ctx.clone();

                        tokio::task::spawn_blocking(move || {
                            let start = Instant::now();
                            match import_pbf_to_sqlite(&path, DB_PATH) {
                                Ok((total, center)) => {
                                    *status_clone.lock().unwrap() = format!(
                                        "Indexed {} items ({:.1}s)",
                                        total,
                                        start.elapsed().as_secs_f32()
                                    );
                                    *recenter_clone.lock().unwrap() = Some(center);
                                }
                                Err(e) => {
                                    *status_clone.lock().unwrap() = format!("Index failed: {}", e);
                                }
                            }
                            *importing_clone.lock().unwrap() = false;
                            ctx_clone.request_repaint();
                        });
                    }
                }
                ui.small(format!("Status: {}", self.status_msg.lock().unwrap()));

                let view = self.map_state.map().view();
                let zoom = (156543.03 / view.resolution()).log2().round() as i32;
                ui.horizontal(|ui| {
                    ui.small(format!(
                        "Zoom: {} | Items: {}",
                        zoom,
                        *self.visible_count.lock().unwrap()
                    ));
                    if ui.small_button("City Zoom").clicked() {
                        let new_view = self.map_state.map().view().clone().with_resolution(30.0);
                        self.map_state.map_mut().set_view(new_view);
                        self.last_bounds = None;
                    }
                });

                ui.separator();

                ui.heading("2a. Custom POIs");
                if let Some((lon, lat)) = self.pending_poi_coord {
                    ui.small(format!("Selected: {:.4}, {:.4}", lon, lat));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.new_poi_name)
                                .hint_text("POI name...")
                                .desired_width(140.0),
                        );
                        if ui.button("Save POI").clicked() && !self.new_poi_name.trim().is_empty() {
                            if let Ok(poi) =
                                add_custom_poi(DB_PATH, self.new_poi_name.trim(), "Custom", lon, lat)
                            {
                                self.custom_pois.push(poi);
                                self.new_poi_name.clear();
                            }
                        }
                    });
                } else {
                    ui.small("Right-click map to pick POI location.");
                }

                let mut poi_to_del = None;
                for poi in &self.custom_pois {
                    ui.horizontal(|ui| {
                        if ui.small_button(format!("★ {}", poi.name)).clicked() {
                            let new_view = self
                                .map_state
                                .map()
                                .view()
                                .clone()
                                .with_position(&latlon!(poi.lat, poi.lon));
                            self.map_state.map_mut().set_view(new_view);
                            self.last_bounds = None;
                        }
                        if ui.small_button("x").clicked() {
                            poi_to_del = Some(poi.id);
                        }
                    });
                }
                if let Some(pid) = poi_to_del {
                    let _ = delete_custom_poi(DB_PATH, pid);
                    self.custom_pois.retain(|p| p.id != pid);
                }

                ui.separator();

                ui.heading("2b. GPS Tracks (.gpx)");
                ui.horizontal(|ui| {
                    if ui.button("Import GPX...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("GPX Track", &["gpx"])
                            .pick_file()
                        {
                            if let Ok(trk) = import_gpx_track(DB_PATH, &path) {
                                if let Some(&(lon, lat)) = trk.points.first() {
                                    *self.pending_recenter.lock().unwrap() = Some((lon, lat));
                                }
                                self.gps_tracks.push(trk);
                                render_gps_tracks_on_layer(
                                    &self.layers.gps_track_layer,
                                    &self.gps_tracks,
                                );
                            }
                        }
                    }
                    if !self.gps_tracks.is_empty() && ui.button("Clear").clicked() {
                        let _ = clear_gps_tracks(DB_PATH);
                        self.gps_tracks.clear();
                        render_gps_tracks_on_layer(&self.layers.gps_track_layer, &self.gps_tracks);
                        self.map_state.map_mut().redraw();
                    }
                });
                for trk in &self.gps_tracks {
                    ui.small(format!("• {} ({} pts)", trk.name, trk.points.len()));
                }

                ui.separator();
                self.render_style_section(ui);

                ui.separator();
                self.render_plugin_section(ui);
            });
    }
}

