pub mod left_panel;
pub mod map_overlay;
pub mod plugin_panel;
pub mod right_panel;
pub mod style_panel;

use crate::style_config::MapStyleConfig;
use parking_lot::RwLock;

use crate::map_renderer::{
    fetch_region_data, load_custom_pois, load_gps_tracks, render_gps_tracks_on_layer,
    replace_layer_features, CustomPoi, GpsTrack, MapGeomFeature, MapLabel, MapLayerBundle,
};
use crate::map_renderer::{replace_colored_polygons, ColoredPolygonFeature};
use crate::plugin::{PluginManager, PluginRenderPacket, PluginVisualItem};
use galileo_types::impls::ClosedContour;

use crate::note_manager::{NoteManager, TripStopNote};
use crate::osm_index::MapBounds;
use crate::pathfinder::{PathFinder, RoutePoint, TravelMode};
use crate::photo_manager::PhotoManager;
use eframe;
use galileo::symbol::Symbol;
use galileo_egui::{EguiMap, EguiMapState};
use galileo_types::geo::impls::GeoPoint2d;
use galileo_types::geometry::Geom;
use galileo_types::impls::Contour;
use galileo_types::latlon;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const DB_PATH: &str = "offline_map.sqlite";

pub fn mercator_to_wgs84(x: f64, y: f64) -> (f64, f64) {
    let lon = (x / 20037508.34) * 180.0;
    let lat_rad = (y / 20037508.34) * std::f64::consts::PI;
    let lat = (2.0 * lat_rad.exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
    (lon.clamp(-180.0, 180.0), lat.clamp(-85.05, 85.05))
}

pub fn wgs84_to_mercator(lon: f64, lat: f64) -> (f64, f64) {
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
    pub map_state: EguiMapState,
    pub layers: MapLayerBundle<S1, S2, Space>,
    pub visible_labels: Arc<Mutex<Vec<MapLabel>>>,
    pub fonts_loaded: bool,
    pub status_msg: Arc<Mutex<String>>,
    pub is_importing: Arc<Mutex<bool>>,
    pub visible_count: Arc<Mutex<usize>>,
    pub last_query_ms: Arc<Mutex<u128>>,
    pub last_bounds: Option<MapBounds>,
    pub last_trigger: Instant,
    pub pending_recenter: Arc<Mutex<Option<(f64, f64)>>>,
    pub needs_redraw: Arc<Mutex<bool>>,

    pub custom_pois: Vec<CustomPoi>,
    pub gps_tracks: Vec<GpsTrack>,
    pub new_poi_name: String,
    pub pending_poi_coord: Option<(f64, f64)>,

    pub pathfinder: Arc<Mutex<Option<Arc<PathFinder>>>>,
    pub pf_loading: Arc<Mutex<bool>>,
    pub travel_mode: TravelMode,
    pub route_dirty: bool,

    pub photo_mgr: PhotoManager,
    pub note_mgr: NoteManager,
    pub stops: Vec<TripStopNote>,
    pub selected_day: i32,
    pub selected_stop_id: Option<i64>,
    pub day_start_time: String,
    pub style: Arc<RwLock<MapStyleConfig>>,

    pub plugin_mgr: PluginManager,
    pub active_plugin_packets: Vec<PluginRenderPacket>,
    pub plugin_dirty: bool,
}
impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn new(
	map_state: EguiMapState,
        layers: MapLayerBundle<S1, S2, Space>,
        style: Arc<RwLock<MapStyleConfig>>,
    ) -> Self {
    // ... 在 Self { ... } 构造中加上 `style,` ...
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
	    style,
            plugin_mgr: PluginManager::new_with_builtins(),
            active_plugin_packets: Vec::new(),
            plugin_dirty: true,
        };
        app.recompute_routes_and_timeline();
        app.reload_pathfinder_async(None);
        app
    }

    pub fn reload_pathfinder_async(&self, ctx: Option<egui::Context>) {
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

    pub fn recompute_routes_and_timeline(&mut self) {
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

     let bounds_changed = self.last_bounds != Some(bounds);
     if self.plugin_dirty || bounds_changed {
         self.plugin_dirty = false;
         self.active_plugin_packets = self.plugin_mgr.collect_active_packets(bounds);

         let mut poly_feats = Vec::new();
         for pkt in &self.active_plugin_packets {
             for item in &pkt.items {
                 if let PluginVisualItem::ChoroplethPolygon {
                     rings,
                     fill_color,
                     stroke_color,
                     ..
                 } = item
                 {
                     let pts: Vec<GeoPoint2d> =
                         rings.iter().map(|&(lon, lat)| latlon!(lat, lon)).collect();
                     if pts.len() >= 3 {
                         poly_feats.push(ColoredPolygonFeature {
                             geometry: Geom::Polygon(galileo_types::impls::Polygon::new(
                                 ClosedContour::new(pts),
                                 vec![],
                             )),
                             fill_color: *fill_color,
                             stroke_color: *stroke_color,
                         });
                     }
                 }
             }
         }
         replace_colored_polygons(&self.layers.plugin_polygons, poly_feats);
         self.map_state.map_mut().redraw();
     }

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
}

impl<S1, S2, Space> eframe::App for OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.sync_viewport_by_bounds(ctx);
        self.render_left_panel(ctx);
        self.render_right_panel(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(egui::Color32::from_rgb(165, 215, 255)))
            .show(ctx, |ui| {
                let map_rect = ui.available_rect_before_wrap();
                EguiMap::new(&mut self.map_state).show_ui(ui);
                self.draw_overlays(ui, map_rect);
            });
    }
}

