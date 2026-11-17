use crate::osm_index::{query_bounds, GeomType, MapBounds, RawFeature};
use crate::style_config::{MapStyleConfig, RgbaColor};
use galileo::layer::feature_layer::{Feature, FeatureLayer};
use galileo::render::render_bundle::RenderBundle;
use galileo::render::{LineCap, LinePaint, PolygonPaint};
use galileo::symbol::Symbol;
use galileo::{Map, MapBuilder};
use galileo_types::cartesian::Point3;
use galileo_types::geo::impls::GeoPoint2d;
use galileo_types::geo::Crs;
use galileo_types::geometry::Geom;
use galileo_types::impls::{ClosedContour, Contour, Polygon};
use galileo_types::latlon;
use gpx::read as read_gpx;
use parking_lot::RwLock;
use rusqlite::{params, Connection};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct MapGeomFeature {
    pub geometry: Geom<GeoPoint2d>,
}

impl Feature for MapGeomFeature {
    type Geom = Geom<GeoPoint2d>;
    fn geometry(&self) -> &Self::Geom {
        &self.geometry
    }
}

/// 动态面符号：实时从共享样式配置中读取颜色
#[derive(Clone)]
pub struct DynamicPolygonSymbol {
    pub style: Arc<RwLock<MapStyleConfig>>,
    pub selector: fn(&MapStyleConfig) -> RgbaColor,
}

impl Symbol<MapGeomFeature> for DynamicPolygonSymbol {
    fn render(
        &self,
        _feature: &MapGeomFeature,
        geometry: &Geom<Point3>,
        _min_resolution: f64,
        bundle: &mut RenderBundle,
    ) {
        let color = (self.selector)(&self.style.read()).to_galileo();
        if let Geom::Polygon(poly) = geometry {
            bundle.add_polygon(poly, &PolygonPaint { color }, _min_resolution);
        }
    }
}

/// 动态线符号：实时从共享样式配置中读取颜色与线宽
#[derive(Clone)]
pub struct DynamicContourSymbol {
    pub style: Arc<RwLock<MapStyleConfig>>,
    pub selector: fn(&MapStyleConfig) -> (RgbaColor, f64),
}

impl Symbol<MapGeomFeature> for DynamicContourSymbol {
    fn render(
        &self,
        _feature: &MapGeomFeature,
        geometry: &Geom<Point3>,
        _min_resolution: f64,
        bundle: &mut RenderBundle,
    ) {
        let (rgba, width) = (self.selector)(&self.style.read());
        let paint = LinePaint {
            color: rgba.to_galileo(),
            width,
            offset: 0.0,
            line_cap: LineCap::Round,
        };
        if let Geom::Contour(contour) = geometry {
            bundle.add_line(contour, &paint, _min_resolution);
        }
    }
}
/// 专用于插件系统的带独立颜色属性的多边形要素
#[derive(Clone)]
pub struct ColoredPolygonFeature {
    pub geometry: Geom<GeoPoint2d>,
    pub fill_color: RgbaColor,
    pub stroke_color: RgbaColor,
}

impl Feature for ColoredPolygonFeature {
    type Geom = Geom<GeoPoint2d>;
    fn geometry(&self) -> &Self::Geom {
        &self.geometry
    }
}

/// 按要素自身颜色独立渲染的专题地图符号
#[derive(Clone, Default)]
pub struct PerFeaturePolygonSymbol;

impl Symbol<ColoredPolygonFeature> for PerFeaturePolygonSymbol {
    fn render(
        &self,
        feature: &ColoredPolygonFeature,
        geometry: &Geom<Point3>,
        min_resolution: f64,
        bundle: &mut RenderBundle,
    ) {
        if let Geom::Polygon(poly) = geometry {
            bundle.add_polygon(
                poly,
                &PolygonPaint {
                    color: feature.fill_color.to_galileo(),
                },
                min_resolution,
            );
            let contour = &poly.outer_contour;

            bundle.add_line(
                contour,
                &LinePaint {
                    color: feature.stroke_color.to_galileo(),
                    width: 1.5,
                    offset: 0.0,
                    line_cap: LineCap::Round,
                },
                min_resolution,
            );
        }
    }
}

pub fn replace_colored_polygons<Space>(
    layer: &Arc<RwLock<FeatureLayer<GeoPoint2d, ColoredPolygonFeature, PerFeaturePolygonSymbol, Space>>>,
    items: Vec<ColoredPolygonFeature>,
) {
    let mut guard = layer.write();
    let store = guard.features_mut();
    let ids: Vec<_> = store.iter().map(|(id, _)| id).collect();
    for id in ids {
        store.remove(id);
    }
    for item in items {
        store.add(item);
    }
    guard.update_all_features();
}
/// 插件结束

#[derive(Clone, Debug)]
pub struct MapLabel {
    pub text: String,
    pub category: String,
    pub priority: i32,
    pub lon: f64,
    pub lat: f64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct CustomPoi {
    pub id: i64,
    pub name: String,
    pub category: String,
    pub lon: f64,
    pub lat: f64,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct GpsTrack {
    pub id: i64,
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

pub struct MapLayerBundle<S1, S2, Space> {
    pub ocean_bg: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S1, Space>>>,
    pub land_base: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S1, Space>>>,
    pub water: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S1, Space>>>,
    pub water_line: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S2, Space>>>,
    pub park: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S1, Space>>>,
    pub building: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S1, Space>>>,
    pub major_road: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S2, Space>>>,
    pub minor_road: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S2, Space>>>,
    pub gps_track_layer: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S2, Space>>>,
    pub route_line: Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S2, Space>>>,
    pub plugin_polygons: Arc<RwLock<FeatureLayer<GeoPoint2d, ColoredPolygonFeature, PerFeaturePolygonSymbol, Space>>>,
}

impl<S1, S2, Space> Clone for MapLayerBundle<S1, S2, Space> {
    fn clone(&self) -> Self {
        Self {
            ocean_bg: Arc::clone(&self.ocean_bg),
            land_base: Arc::clone(&self.land_base),
            water: Arc::clone(&self.water),
            water_line: Arc::clone(&self.water_line),
            park: Arc::clone(&self.park),
            building: Arc::clone(&self.building),
            major_road: Arc::clone(&self.major_road),
            minor_road: Arc::clone(&self.minor_road),
            gps_track_layer: Arc::clone(&self.gps_track_layer),
            route_line: Arc::clone(&self.route_line),
	    plugin_polygons: Arc::clone(&self.plugin_polygons),
        }
    }
}

pub fn replace_layer_features<S, Space>(
    layer: &Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S, Space>>>,
    items: Vec<MapGeomFeature>,
) where
    S: Symbol<MapGeomFeature>,
{
    let mut guard = layer.write();
    let store = guard.features_mut();
    let ids: Vec<_> = store.iter().map(|(id, _)| id).collect();
    for id in ids {
        store.remove(id);
    }
    for item in items {
        store.add(item);
    }
    guard.update_all_features();
}

pub fn init_overlay_tables(db_path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS custom_pois (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT NOT NULL,
             category TEXT NOT NULL DEFAULT 'Custom',
             lon REAL NOT NULL,
             lat REAL NOT NULL
         );

         CREATE TABLE IF NOT EXISTS gps_tracks (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT NOT NULL,
             points_blob BLOB NOT NULL
         );",
    )?;
    Ok(())
}

pub fn add_custom_poi(
    db_path: &str,
    name: &str,
    category: &str,
    lon: f64,
    lat: f64,
) -> rusqlite::Result<CustomPoi> {
    init_overlay_tables(db_path)?;
    let conn = Connection::open(db_path)?;
    conn.execute(
        "INSERT INTO custom_pois (name, category, lon, lat) VALUES (?1, ?2, ?3, ?4)",
        params![name, category, lon, lat],
    )?;
    Ok(CustomPoi {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        category: category.to_string(),
        lon,
        lat,
    })
}

pub fn delete_custom_poi(db_path: &str, id: i64) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute("DELETE FROM custom_pois WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn load_custom_pois(db_path: &str) -> rusqlite::Result<Vec<CustomPoi>> {
    init_overlay_tables(db_path)?;
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT id, name, category, lon, lat FROM custom_pois ORDER BY id ASC")?;
    let rows = stmt.query_map([], |r| {
        Ok(CustomPoi {
            id: r.get(0)?,
            name: r.get(1)?,
            category: r.get(2)?,
            lon: r.get(3)?,
            lat: r.get(4)?,
        })
    })?;
    Ok(rows.flatten().collect())
}
pub fn import_gpx_track<P: AsRef<Path>>(
    db_path: &str,
    gpx_path: P,
) -> Result<GpsTrack, Box<dyn std::error::Error>> {
    init_overlay_tables(db_path)?;
    let file = File::open(&gpx_path)?;
    let reader = BufReader::new(file);
    let gpx_data = read_gpx(reader)?;

    let mut points: Vec<(f64, f64)> = Vec::new();
    for track in &gpx_data.tracks {
        for segment in &track.segments {
            for pt in &segment.points {
                let p = pt.point();
                points.push((p.x(), p.y()));
            }
        }
    }
    if points.is_empty() {
        for route in &gpx_data.routes {
            for pt in &route.points {
                let p = pt.point();
                points.push((p.x(), p.y()));
            }
        }
    }

    if points.len() < 2 {
        return Err("GPX 文件中未包含有效轨迹坐标点".into());
    }

    let name = gpx_data
        .tracks
        .first()
        .and_then(|t| t.name.clone())
        .or_else(|| {
            gpx_path
                .as_ref()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "GPS Track".to_string());

    let blob = bincode::serialize(&points)?;
    let conn = Connection::open(db_path)?;
    conn.execute(
        "INSERT INTO gps_tracks (name, points_blob) VALUES (?1, ?2)",
        params![name, blob],
    )?;

    Ok(GpsTrack {
        id: conn.last_insert_rowid(),
        name,
        points,
    })
}

pub fn load_gps_tracks(db_path: &str) -> rusqlite::Result<Vec<GpsTrack>> {
    init_overlay_tables(db_path)?;
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare("SELECT id, name, points_blob FROM gps_tracks ORDER BY id ASC")?;
    let rows = stmt.query_map([], |r| {
        let id: i64 = r.get(0)?;
        let name: String = r.get(1)?;
        let blob: Vec<u8> = r.get(2)?;
        let points: Vec<(f64, f64)> = bincode::deserialize(&blob).unwrap_or_default();
        Ok(GpsTrack { id, name, points })
    })?;
    Ok(rows.flatten().collect())
}

pub fn clear_gps_tracks(db_path: &str) -> rusqlite::Result<()> {
    let conn = Connection::open(db_path)?;
    conn.execute("DELETE FROM gps_tracks", [])?;
    Ok(())
}

pub fn render_gps_tracks_on_layer<S, Space>(
    layer: &Arc<RwLock<FeatureLayer<GeoPoint2d, MapGeomFeature, S, Space>>>,
    tracks: &[GpsTrack],
) where
    S: Symbol<MapGeomFeature>,
{
    let mut feats = Vec::new();
    for trk in tracks {
        if trk.points.len() >= 2 {
            let pts: Vec<GeoPoint2d> = trk
                .points
                .iter()
                .map(|&(lon, lat)| latlon!(lat, lon))
                .collect();
            feats.push(MapGeomFeature {
                geometry: Geom::Contour(Contour::open(pts)),
            });
        }
    }
    replace_layer_features(layer, feats);
}

#[derive(Default)]
pub struct RegionRenderData {
    pub total_raw: usize,
    pub water: Vec<MapGeomFeature>,
    pub water_line: Vec<MapGeomFeature>,
    pub park: Vec<MapGeomFeature>,
    pub building: Vec<MapGeomFeature>,
    pub major_road: Vec<MapGeomFeature>,
    pub minor_road: Vec<MapGeomFeature>,
    pub labels: Vec<MapLabel>,
}

pub fn fetch_region_data(db_path: &str, bounds: MapBounds) -> rusqlite::Result<RegionRenderData> {
    let raw: Vec<RawFeature> = query_bounds(db_path, bounds)?;
    let mut out = RegionRenderData {
        total_raw: raw.len(),
        ..Default::default()
    };

    for r in raw {
        match r.geom {
            GeomType::Point(lon, lat) => {
                if let Some(text) = r.name {
                    out.labels.push(MapLabel {
                        text,
                        category: r.layer,
                        priority: r.min_zoom,
                        lon,
                        lat,
                    });
                }
            }
            GeomType::LineString(pts) => {
                let points: Vec<GeoPoint2d> =
                    pts.into_iter().map(|(lon, lat)| latlon!(lat, lon)).collect();
                let feat = MapGeomFeature {
                    geometry: Geom::Contour(Contour::open(points)),
                };
                match r.layer.as_str() {
                    "coastline" | "water_line" | "water_stream" => out.water_line.push(feat),
                    "motorway" | "trunk" | "primary" => out.major_road.push(feat),
                    _ => out.minor_road.push(feat),
                }
            }
            GeomType::Polygon(pts) => {
                let points: Vec<GeoPoint2d> =
                    pts.into_iter().map(|(lon, lat)| latlon!(lat, lon)).collect();
                let feat = MapGeomFeature {
                    geometry: Geom::Polygon(Polygon::new(ClosedContour::new(points), vec![])),
                };
                match r.layer.as_str() {
                    "water" => out.water.push(feat),
                    "park" => out.park.push(feat),
                    "building" => out.building.push(feat),
                    _ => {}
                }
            }
        }
    }

    out.labels.sort_by_key(|l| l.priority);
    Ok(out)
}
pub fn load_land_geojson<P: AsRef<Path>>(path: P) -> Vec<MapGeomFeature> {
    let Ok(raw_str) = std::fs::read_to_string(&path) else {
        eprintln!("警告: 未找到陆地底图文件 {:?}", path.as_ref());
        return Vec::new();
    };
    let Ok(gj) = raw_str.parse::<geojson::GeoJson>() else {
        eprintln!("警告: 解析陆地底图 GeoJSON 失败");
        return Vec::new();
    };

    let to_geo_pts = |ring: &Vec<Vec<f64>>| -> Option<Vec<GeoPoint2d>> {
        let mut in_asia = false;
        let mut pts: Vec<GeoPoint2d> = ring
            .iter()
            .filter_map(|p| {
                let lon = *p.first()?;
                let lat = *p.get(1)?;
                if (65.0..=150.0).contains(&lon) && (-10.0..=65.0).contains(&lat) {
                    in_asia = true;
                }
                Some(latlon!(lat.clamp(-82.0, 82.0), lon.clamp(-179.9, 179.9)))
            })
            .collect();

        if !in_asia || pts.len() < 3 {
            return None;
        }
        if pts.len() >= 4 && pts.first() == pts.last() {
            pts.pop();
        }
        Some(pts)
    };

    let mut features = Vec::new();
    let mut extract_geom = |geom: &geojson::Geometry| match &geom.value {
        geojson::Value::Polygon(rings) => {
            if let Some(outer) = rings.first().and_then(|r| to_geo_pts(r)) {
                let holes: Vec<ClosedContour<GeoPoint2d>> = rings
                    .iter()
                    .skip(1)
                    .filter_map(|h| to_geo_pts(h).map(ClosedContour::new))
                    .collect();
                features.push(MapGeomFeature {
                    geometry: Geom::Polygon(Polygon::new(ClosedContour::new(outer), holes)),
                });
            }
        }
        geojson::Value::MultiPolygon(polys) => {
            for rings in polys {
                if let Some(outer) = rings.first().and_then(|r| to_geo_pts(r)) {
                    let holes: Vec<ClosedContour<GeoPoint2d>> = rings
                        .iter()
                        .skip(1)
                        .filter_map(|h| to_geo_pts(h).map(ClosedContour::new))
                        .collect();
                    features.push(MapGeomFeature {
                        geometry: Geom::Polygon(Polygon::new(ClosedContour::new(outer), holes)),
                    });
                }
            }
        }
        _ => {}
    };

    match gj {
        geojson::GeoJson::FeatureCollection(fc) => {
            for feat in &fc.features {
                if let Some(ref g) = feat.geometry {
                    extract_geom(g);
                }
            }
        }
        geojson::GeoJson::Feature(feat) => {
            if let Some(ref g) = feat.geometry {
                extract_geom(g);
            }
        }
        geojson::GeoJson::Geometry(g) => extract_geom(&g),
    }

    features
}

pub fn create_map_with_layers(
    center_lat: f64,
    center_lon: f64,
    style: Arc<RwLock<MapStyleConfig>>,
) -> (Map, MapLayerBundle<DynamicPolygonSymbol, DynamicContourSymbol, impl Send + Sync + 'static>) {
    let land_feats = load_land_geojson("china_land_hd.geojson");

    let ocean_poly = MapGeomFeature {
        geometry: Geom::Polygon(Polygon::new(
            ClosedContour::new(vec![
                latlon!(-15.0, 60.0),
                latlon!(-15.0, 155.0),
                latlon!(70.0, 155.0),
                latlon!(70.0, 60.0),
            ]),
            vec![],
        )),
    };

    let layers = MapLayerBundle {
        ocean_bg: Arc::new(RwLock::new(FeatureLayer::new(
            vec![ocean_poly],
            DynamicPolygonSymbol {
                style: Arc::clone(&style),
                selector: |s| s.ocean_color,
            },
            Crs::WGS84,
        ))),
        land_base: Arc::new(RwLock::new(FeatureLayer::new(
            land_feats,
            DynamicPolygonSymbol {
                style: Arc::clone(&style),
                selector: |s| s.land_color,
            },
            Crs::WGS84,
        ))),
        water: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicPolygonSymbol {
                style: Arc::clone(&style),
                selector: |s| s.water_fill,
            },
            Crs::WGS84,
        ))),
        water_line: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicContourSymbol {
                style: Arc::clone(&style),
                selector: |s| (s.water_line_color, s.water_line_width),
            },
            Crs::WGS84,
        ))),
        park: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicPolygonSymbol {
                style: Arc::clone(&style),
                selector: |s| s.park_fill,
            },
            Crs::WGS84,
        ))),
        building: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicPolygonSymbol {
                style: Arc::clone(&style),
                selector: |s| s.building_fill,
            },
            Crs::WGS84,
        ))),
        minor_road: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicContourSymbol {
                style: Arc::clone(&style),
                selector: |s| (s.minor_road_color, s.minor_road_width),
            },
            Crs::WGS84,
        ))),
        major_road: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicContourSymbol {
                style: Arc::clone(&style),
                selector: |s| (s.major_road_color, s.major_road_width),
            },
            Crs::WGS84,
        ))),
        gps_track_layer: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicContourSymbol {
                style: Arc::clone(&style),
                selector: |s| (s.gps_track_color, s.gps_track_width),
            },
            Crs::WGS84,
        ))),
        route_line: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            DynamicContourSymbol {
                style: Arc::clone(&style),
                selector: |s| (s.route_color, s.route_width),
            },
            Crs::WGS84,
        ))),
        plugin_polygons: Arc::new(RwLock::new(FeatureLayer::new(
            vec![],
            PerFeaturePolygonSymbol,
            Crs::WGS84,
        ))),
    };

    let mut map = MapBuilder::default()
        .with_layer(layers.ocean_bg.clone())
        .with_layer(layers.land_base.clone())
        .with_layer(layers.water.clone())
        .with_layer(layers.water_line.clone())
        .with_layer(layers.park.clone())
        .with_layer(layers.building.clone())
        .with_layer(layers.minor_road.clone())
        .with_layer(layers.major_road.clone())
        .with_layer(layers.plugin_polygons.clone())
        .with_layer(layers.gps_track_layer.clone())
        .with_layer(layers.route_line.clone())
        .build();

    let view = map
        .view()
        .clone()
        .with_position(&latlon!(center_lat, center_lon))
        .with_resolution(30.0);
    map.set_view(view);

    (map, layers)
}

