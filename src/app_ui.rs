use crate::map_renderer::{
    add_custom_poi, clear_gps_tracks, delete_custom_poi, fetch_region_data, import_gpx_track,
    load_custom_pois, load_gps_tracks, render_gps_tracks_on_layer, replace_layer_features,
    CustomPoi, GpsTrack, MapGeomFeature, MapLabel, MapLayerBundle,
};
use crate::note_manager::{NoteManager, TripStopNote};
use crate::osm_index::{import_pbf_to_sqlite, MapBounds};
use crate::pathfinder::{PathFinder, RoutePoint, TravelMode};
use crate::photo_manager::PhotoManager;
use eframe;
use galileo::symbol::Symbol;
use galileo_egui::{EguiMap, EguiMapState};
use galileo_types::geo::impls::GeoPoint2d;
use galileo_types::geometry::Geom;
use galileo_types::impls::Contour;
use galileo_types::latlon;
use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DB_PATH: &str = "offline_map.sqlite";

fn mercator_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let lon = (x / 20037508.34) * 180.0;
    let lat_rad = (y / 20037508.34) * std::f64::consts::PI;
    let lat = (2.0 * lat_rad.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    (lon.clamp(-180.0, 180.0), lat.clamp(-85.05, 85.05))
}

fn wgs84_to_mercator(lon: f64, lat: f64) -> (f64, f64) {
    let x = (lon / 180.0) * 20037508.34;
    let lat_clamped = lat.clamp(-85.05, 85.05).to_radians();
    let y = ((std::f64::consts::FRAC_PI_4 + lat_clamped / 2.0).tan().ln() / std::f64::consts::PI)
        * 20037508.34;
    (x, y)
}

fn setup_chinese_fonts(ctx: &egui::Context) {
    let font_candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
    ];

    for path in font_candidates {
        if Path::new(path).exists() {
            if let Ok(font_data) = std::fs::read(path) {
                let mut fonts = egui::FontDefinitions::default();
                fonts.font_data.insert(
                    "cjk_font".to_owned(),
                    egui::FontData::from_owned(font_data).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "cjk_font".to_owned());
                ctx.set_fonts(fonts);
                break;
            }
        }
    }
}

pub struct OfflineMapApp<S1, S2, Space> {
    map_state: EguiMapState,
    layers: MapLayerBundle<S1, S2, Space>,
    visible_labels: Arc<Mutex<Vec<MapLabel>>>,
    fonts_loaded: bool,
    status_msg: Arc<Mutex<String>>,
    is_importing: Arc<Mutex<bool>>,
    visible_count: Arc<Mutex<usize>>,
    last_query_ms: Arc<Mutex<u128>>,
    last_bounds: Option<MapBounds>,
    last_trigger: Instant,
    pending_recenter: Arc<Mutex<Option<(f64, f64)>>>,
    needs_redraw: Arc<Mutex<bool>>,

    // 模块 2a & 2b：自定义 POI 与 GPS 轨迹
    custom_pois: Vec<CustomPoi>,
    gps_tracks: Vec<GpsTrack>,
    new_poi_name: String,
    pending_poi_coord: Option<(f64, f64)>,

    // 模块 3：A* 路径规划器
    pathfinder: Arc<Mutex<Option<Arc<PathFinder>>>>,
    pf_loading: Arc<Mutex<bool>>,
    travel_mode: TravelMode,
    route_dirty: bool,

    // 模块 4 & 5：图片管理与笔记行程管理
    photo_mgr: PhotoManager,
    note_mgr: NoteManager,
    stops: Vec<TripStopNote>,
    selected_day: i32,
    selected_stop_id: Option<i64>,
    day_start_time: String,
}
impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn new(map_state: EguiMapState, layers: MapLayerBundle<S1, S2, Space>) -> Self {
        let photo_mgr = PhotoManager::new(DB_PATH).expect("Failed to init PhotoManager");
        let note_mgr = NoteManager::new(DB_PATH).expect("Failed to init NoteManager");
        let stops = note_mgr.load_all(&photo_mgr).unwrap_or_default();
        let selected_stop_id = stops.first().map(|s| s.id);
        let custom_pois = load_custom_pois(DB_PATH).unwrap_or_default();
        let gps_tracks = load_gps_tracks(DB_PATH).unwrap_or_default();

        render_gps_tracks_on_layer(&layers.gps_track_layer, &gps_tracks);

        let mut app = Self {
            map_state,
            layers,
            visible_labels: Arc::new(Mutex::new(Vec::new())),
            fonts_loaded: false,
            status_msg: Arc::new(Mutex::new("Loading A* pathfinder...".to_string())),
            is_importing: Arc::new(Mutex::new(false)),
            visible_count: Arc::new(Mutex::new(0)),
            last_query_ms: Arc::new(Mutex::new(0)),
            last_bounds: None,
            last_trigger: Instant::now() - Duration::from_secs(1),
            pending_recenter: Arc::new(Mutex::new(None)),
            needs_redraw: Arc::new(Mutex::new(false)),
            custom_pois,
            gps_tracks,
            new_poi_name: String::new(),
            pending_poi_coord: None,
            pathfinder: Arc::new(Mutex::new(None)),
            pf_loading: Arc::new(Mutex::new(false)),
            travel_mode: TravelMode::Driving,
            route_dirty: true,
            photo_mgr,
            note_mgr,
            stops,
            selected_day: 1,
            selected_stop_id,
            day_start_time: "09:00".to_string(),
        };
        app.recompute_routes_and_timeline();
        app.reload_pathfinder_async(None);
        app
    }

    fn reload_pathfinder_async(&self, ctx: Option<egui::Context>) {
        if *self.pf_loading.lock().unwrap() {
            return;
        }
        *self.pf_loading.lock().unwrap() = true;
        let pf_arc = Arc::clone(&self.pathfinder);
        let loading_arc = Arc::clone(&self.pf_loading);
        let status_arc = Arc::clone(&self.status_msg);

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            match PathFinder::load_from_sqlite(DB_PATH) {
                Ok(pf) => {
                    let count = pf.node_count();
                    *pf_arc.lock().unwrap() = Some(Arc::new(pf));
                    *status_arc.lock().unwrap() = format!(
                        "A* graph ready ({} nodes, {:.2}s)",
                        count,
                        start.elapsed().as_secs_f32()
                    );
                }
                Err(e) => {
                    *status_arc.lock().unwrap() = format!("Graph error: {}", e);
                }
            }
            *loading_arc.lock().unwrap() = false;
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
    }

    fn recompute_routes_and_timeline(&mut self) {
        self.route_dirty = false;
        self.stops.sort_by_key(|s| (s.day_index, s.order_idx, s.id));

        let mut route_feats = Vec::new();
        let pf_opt = self.pathfinder.lock().unwrap().clone();

        let mut prev_day = -1;
        for i in 0..self.stops.len() {
            if self.stops[i].day_index != prev_day {
                prev_day = self.stops[i].day_index;
                self.stops[i].travel_km_from_prev = 0.0;
                self.stops[i].travel_min_from_prev = 0.0;
            } else if i > 0 {
                if let Some(ref pf) = pf_opt {
                    let p_start = RoutePoint {
                        lon: self.stops[i - 1].lon,
                        lat: self.stops[i - 1].lat,
                    };
                    let p_end = RoutePoint {
                        lon: self.stops[i].lon,
                        lat: self.stops[i].lat,
                    };
                    let res = pf.find_path(p_start, p_end, self.travel_mode);
                    self.stops[i].travel_km_from_prev = res.distance_km;
                    self.stops[i].travel_min_from_prev = res.duration_min;

                    if self.stops[i].day_index == self.selected_day && res.path_coords.len() >= 2 {
                        let pts: Vec<GeoPoint2d> = res
                            .path_coords
                            .iter()
                            .map(|&(lon, lat)| latlon!(lat, lon))
                            .collect();
                        route_feats.push(MapGeomFeature {
                            geometry: Geom::Contour(Contour::open(pts)),
                        });
                    }
                }
            }
        }

        NoteManager::recompute_timelines(&mut self.stops, &self.day_start_time);
        replace_layer_features(&self.layers.route_line, route_feats);
        self.map_state.map_mut().redraw();
    }

    /// 2. 计算当前视野范围参数 (MapBounds) 并异步调用地图渲染模块
    fn sync_viewport_by_bounds(&mut self, ctx: &egui::Context) {
        if !self.fonts_loaded {
            setup_chinese_fonts(ctx);
            self.fonts_loaded = true;
        }

        if let Ok(mut redraw) = self.needs_redraw.lock() {
            if *redraw {
                *redraw = false;
                self.map_state.map_mut().redraw();
            }
        }

        if let Ok(mut recenter) = self.pending_recenter.lock() {
            if let Some((lon, lat)) = recenter.take() {
                let new_view = self
                    .map_state
                    .map()
                    .view()
                    .clone()
                    .with_position(&latlon!(lat, lon))
                    .with_resolution(30.0);
                self.map_state.map_mut().set_view(new_view);
                self.last_bounds = None;
                self.route_dirty = true;
                self.reload_pathfinder_async(Some(ctx.clone()));
            }
        }

        if self.route_dirty {
            self.recompute_routes_and_timeline();
        }

        let view = self.map_state.map().view();
        let Some(bbox) = view.get_bbox() else { return };

        let (min_lon, min_lat) = mercator_to_wgs84(bbox.x_min(), bbox.y_min());
        let (max_lon, max_lat) = mercator_to_wgs84(bbox.x_max(), bbox.y_max());
        let zoom = (156543.03 / view.resolution()).log2().round().clamp(1.0, 20.0) as i32;

        let bounds = MapBounds {
            min_lon,
            min_lat,
            max_lon,
            max_lat,
            zoom,
        };

        if let Some(prev) = self.last_bounds {
            if (prev.min_lon - bounds.min_lon).abs() < 1e-6
                && (prev.min_lat - bounds.min_lat).abs() < 1e-6
                && (prev.max_lon - bounds.max_lon).abs() < 1e-6
                && (prev.max_lat - bounds.max_lat).abs() < 1e-6
                && prev.zoom == bounds.zoom
            {
                return;
            }
        }

        if self.last_trigger.elapsed() < Duration::from_millis(40) {
            return;
        }

        self.last_bounds = Some(bounds);
        self.last_trigger = Instant::now();

        let layers_clone = self.layers.clone();
        let labels_clone = Arc::clone(&self.visible_labels);
        let count_clone = Arc::clone(&self.visible_count);
        let time_clone = Arc::clone(&self.last_query_ms);
        let redraw_clone = Arc::clone(&self.needs_redraw);
        let ctx_clone = ctx.clone();

        tokio::task::spawn_blocking(move || {
            let start = Instant::now();
            if let Ok(region) = fetch_region_data(DB_PATH, bounds) {
                let elapsed = start.elapsed().as_millis();

                replace_layer_features(&layers_clone.water, region.water);
                replace_layer_features(&layers_clone.water_line, region.water_line);
                replace_layer_features(&layers_clone.park, region.park);
                replace_layer_features(&layers_clone.building, region.building);
                replace_layer_features(&layers_clone.major_road, region.major_road);
                replace_layer_features(&layers_clone.minor_road, region.minor_road);

                if let Ok(mut lbl) = labels_clone.lock() {
                    *lbl = region.labels;
                }
                if let Ok(mut c) = count_clone.lock() {
                    *c = region.total_raw;
                }
                if let Ok(mut t) = time_clone.lock() {
                    *t = elapsed;
                }
                if let Ok(mut r) = redraw_clone.lock() {
                    *r = true;
                }
                ctx_clone.request_repaint();
            }
        });
    }
    fn draw_overlays(&mut self, ui: &mut egui::Ui, map_rect: egui::Rect) {
        let view = self.map_state.map().view();
        let Some(bbox) = view.get_bbox() else { return };
        let bbox_w = bbox.x_max() - bbox.x_min();
        let bbox_h = bbox.y_max() - bbox.y_min();
        if bbox_w <= 0.0 || bbox_h <= 0.0 {
            return;
        }

        let right_click_pos = ui.input(|i| {
            if i.pointer.secondary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        let mut new_stop_to_insert: Option<(String, f64, f64)> = None;

        // 右键点击地图：直接添加到当日行程，同时记录坐标供左侧“添加自定义 POI”使用
        if let Some(pos) = right_click_pos {
            if map_rect.contains(pos) {
                let norm_x = ((pos.x - map_rect.left()) / map_rect.width()) as f64;
                let norm_y = 1.0 - ((pos.y - map_rect.top()) / map_rect.height()) as f64;
                let mx = bbox.x_min() + norm_x * bbox_w;
                let my = bbox.y_min() + norm_y * bbox_h;
                let (lon, lat) = mercator_to_wgs84(mx, my);

                self.pending_poi_coord = Some((lon, lat));
                let day_count = self
                    .stops
                    .iter()
                    .filter(|s| s.day_index == self.selected_day)
                    .count();
                new_stop_to_insert = Some((
                    format!("Day{} Stop #{}", self.selected_day, day_count + 1),
                    lon,
                    lat,
                ));
            }
        }

        let left_click_pos = ui.input(|i| {
            if i.pointer.primary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        let painter = ui.painter_at(map_rect);
        let mut occupied_cells: HashSet<(i32, i32)> = HashSet::new();
        let mut drawn_roads: HashSet<String> = HashSet::new();

        // 1. 绘制 OSM 底图地名与系统 POI
        if let Ok(labels) = self.visible_labels.lock() {
            for lbl in labels.iter() {
                if lbl.category == "label_road" && !drawn_roads.insert(lbl.text.clone()) {
                    continue;
                }
                let (mx, my) = wgs84_to_mercator(lbl.lon, lbl.lat);
                let norm_x = (mx - bbox.x_min()) / bbox_w;
                let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
                if !(0.0..=1.0).contains(&norm_x) || !(0.0..=1.0).contains(&norm_y) {
                    continue;
                }
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let cell = ((sx / 115.0) as i32, (sy / 32.0) as i32);
                if !occupied_cells.insert(cell) {
                    continue;
                }

                let (font_size, text_color, dot_color) = match lbl.category.as_str() {
                    "label_city" => (16.0, egui::Color32::from_rgb(30, 30, 35), egui::Color32::from_rgb(220, 60, 60)),
                    "label_town" => (13.5, egui::Color32::from_rgb(50, 50, 60), egui::Color32::from_rgb(230, 120, 40)),
                    "label_poi" => (12.5, egui::Color32::from_rgb(25, 95, 160), egui::Color32::from_rgb(30, 130, 220)),
                    _ => (11.5, egui::Color32::from_rgb(85, 85, 90), egui::Color32::TRANSPARENT),
                };

                if dot_color != egui::Color32::TRANSPARENT {
                    let center_pt = egui::pos2(sx, sy);
                    painter.circle_filled(center_pt, 4.0, dot_color);
                    painter.circle_stroke(center_pt, 4.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
                    if let Some(cpos) = left_click_pos {
                        if cpos.distance(center_pt) <= 12.0 && new_stop_to_insert.is_none() {
                            new_stop_to_insert = Some((lbl.text.clone(), lbl.lon, lbl.lat));
                        }
                    }
                }

                let text_pos = egui::pos2(sx + 6.0, sy - font_size * 0.5);
                let font = egui::FontId::proportional(font_size);
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    painter.text(
                        egui::pos2(text_pos.x + dx, text_pos.y + dy),
                        egui::Align2::LEFT_TOP,
                        &lbl.text,
                        font.clone(),
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 225),
                    );
                }
                painter.text(text_pos, egui::Align2::LEFT_TOP, &lbl.text, font, text_color);
            }
        }

        // 2a. 绘制用户设置的自定义 POI（金色五角星/菱形标志，左键点击可加入行程）
        for poi in &self.custom_pois {
            let (mx, my) = wgs84_to_mercator(poi.lon, poi.lat);
            let norm_x = (mx - bbox.x_min()) / bbox_w;
            let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
            if (0.0..=1.0).contains(&norm_x) && (0.0..=1.0).contains(&norm_y) {
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let pt = egui::pos2(sx, sy);

                painter.circle_filled(pt, 6.5, egui::Color32::from_rgb(245, 180, 0));
                painter.circle_stroke(pt, 6.5, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(120, 60, 0)));
                painter.text(
                    egui::pos2(sx + 9.0, sy - 7.0),
                    egui::Align2::LEFT_TOP,
                    format!("★ {}", poi.name),
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(160, 80, 0),
                );

                if let Some(cpos) = left_click_pos {
                    if cpos.distance(pt) <= 12.0 && new_stop_to_insert.is_none() {
                        new_stop_to_insert = Some((poi.name.clone(), poi.lon, poi.lat));
                    }
                }
            }
        }

        if let Some((name, lon, lat)) = new_stop_to_insert {
            let order_idx = self
                .stops
                .iter()
                .filter(|s| s.day_index == self.selected_day)
                .count() as i32;
            if let Ok(new_stop) = self.note_mgr.create_stop(self.selected_day, order_idx, &name, lon, lat) {
                self.selected_stop_id = Some(new_stop.id);
                self.stops.push(new_stop);
                self.route_dirty = true;
            }
        }

        // 3. 绘制当日行程序号图钉
        let mut day_seq = 0;
        for stop in &self.stops {
            if stop.day_index != self.selected_day {
                continue;
            }
            day_seq += 1;
            let (mx, my) = wgs84_to_mercator(stop.lon, stop.lat);
            let norm_x = (mx - bbox.x_min()) / bbox_w;
            let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
            if (0.0..=1.0).contains(&norm_x) && (0.0..=1.0).contains(&norm_y) {
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let pin_pos = egui::pos2(sx, sy);

                let fill = if self.selected_stop_id == Some(stop.id) {
                    egui::Color32::from_rgb(20, 160, 90)
                } else {
                    egui::Color32::from_rgb(230, 55, 55)
                };
                painter.circle_filled(pin_pos, 11.5, fill);
                painter.circle_stroke(pin_pos, 11.5, egui::Stroke::new(2.0_f32, egui::Color32::WHITE));
                painter.text(
                    pin_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("{}", day_seq),
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        }
    }
}
impl<S1, S2, Space> eframe::App for OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_viewport_by_bounds(ctx);

        // 左侧面板：地图索引 (1)、自定义 POI (2a) 与 GPS 轨迹 (2b)
        egui::SidePanel::left("control_panel")
            .resizable(true)
            .default_width(265.0)
            .show(ctx, |ui| {
                ui.heading("Offline Vector Map");
                ui.separator();

                let importing = *self.is_importing.lock().unwrap();
                if ui.add_enabled(!importing, egui::Button::new("1. Import .osm.pbf Index...")).clicked() {
                    if let Some(path) = rfd::FileDialog::new().add_filter("OSM PBF", &["pbf"]).pick_file() {
                        *self.is_importing.lock().unwrap() = true;
                        *self.status_msg.lock().unwrap() = format!("Indexing {:?}...", path.file_name().unwrap_or_default());

                        let status_clone = Arc::clone(&self.status_msg);
                        let importing_clone = Arc::clone(&self.is_importing);
                        let recenter_clone = Arc::clone(&self.pending_recenter);
                        let ctx_clone = ctx.clone();

                        tokio::task::spawn_blocking(move || {
                            let start = Instant::now();
                            match import_pbf_to_sqlite(&path, DB_PATH) {
                                Ok((total, center)) => {
                                    *status_clone.lock().unwrap() = format!("Indexed {} items ({:.1}s)", total, start.elapsed().as_secs_f32());
                                    *recenter_clone.lock().unwrap() = Some(center);
                                }
                                Err(e) => *status_clone.lock().unwrap() = format!("Index failed: {}", e),
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
                    ui.small(format!("Zoom: {} | Items: {}", zoom, *self.visible_count.lock().unwrap()));
                    if ui.small_button("City Zoom").clicked() {
                        let new_view = self.map_state.map().view().clone().with_resolution(30.0);
                        self.map_state.map_mut().set_view(new_view);
                        self.last_bounds = None;
                    }
                });

                ui.separator();

                // 2a. 自定义 POI 管理
                ui.heading("2a. Custom POIs");
                if let Some((lon, lat)) = self.pending_poi_coord {
                    ui.small(format!("Selected: {:.4}, {:.4}", lon, lat));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.new_poi_name).hint_text("POI name...").desired_width(140.0));
                        if ui.button("Save POI").clicked() && !self.new_poi_name.trim().is_empty() {
                            if let Ok(poi) = add_custom_poi(DB_PATH, self.new_poi_name.trim(), "Custom", lon, lat) {
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
                            let new_view = self.map_state.map().view().clone().with_position(&latlon!(poi.lat, poi.lon));
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

                // 2b. GPS 轨迹管理
                ui.heading("2b. GPS Tracks (.gpx)");
                ui.horizontal(|ui| {
                    if ui.button("Import GPX...").clicked() {
                        if let Some(path) = rfd::FileDialog::new().add_filter("GPX Track", &["gpx"]).pick_file() {
                            if let Ok(trk) = import_gpx_track(DB_PATH, &path) {
                                if let Some(&(lon, lat)) = trk.points.first() {
                                    *self.pending_recenter.lock().unwrap() = Some((lon, lat));
                                }
                                self.gps_tracks.push(trk);
                                render_gps_tracks_on_layer(&self.layers.gps_track_layer, &self.gps_tracks);
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
            });

        // 右侧面板：A* 路径规划 (3)、照片管理 (4) 与 笔记行程 (5)
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
                } else {
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
                                    if ui.add(egui::DragValue::new(&mut self.stops[idx].stay_minutes).range(0..=720).speed(5)).changed() {
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

                        if let Some(sel_id) = self.selected_stop_id {
                            if let Some(s_idx) = self.stops.iter().position(|s| s.id == sel_id) {
                                ui.separator();
                                ui.heading("Notes & Photos");
                                if ui.text_edit_singleline(&mut self.stops[s_idx].name).changed() {
                                    let _ = self.note_mgr.update_stop(&self.stops[s_idx]);
                                }
                                if ui.add(egui::TextEdit::multiline(&mut self.stops[s_idx].notes).desired_rows(4).desired_width(f32::INFINITY)).changed() {
                                    let _ = self.note_mgr.update_stop(&self.stops[s_idx]);
                                }
                                if ui.button("+ Add Photos...").clicked() {
                                    if let Some(files) = rfd::FileDialog::new().add_filter("Images", &["jpg", "jpeg", "png", "webp"]).pick_files() {
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
                }
            });

        // 中央地图渲染区
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(245, 243, 238)))
            .show(ctx, |ui| {
                let map_rect = ui.available_rect_before_wrap();
                EguiMap::new(&mut self.map_state).show_ui(ui);
                self.draw_overlays(ui, map_rect);
            });
    }
}

