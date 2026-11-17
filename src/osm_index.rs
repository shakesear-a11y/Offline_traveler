use osmpbf::{Element, ElementReader, RelMemberType};
use rusqlite::{params, Connection};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 统一的地图渲染范围参数 (Bounding Box + 缩放层级)
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MapBounds {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
    pub zoom: i32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GeomType {
    Point(f64, f64),
    LineString(Vec<(f64, f64)>),
    Polygon(Vec<(f64, f64)>),
}

pub struct RawFeature {
    pub layer: String,
    pub name: Option<String>,
    pub min_zoom: i32,
    pub geom: GeomType,
}

pub fn init_db<P: AsRef<Path>>(db_path: P) -> rusqlite::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA cache_size = -64000;

         CREATE TABLE IF NOT EXISTS features (
             id INTEGER PRIMARY KEY,
             layer TEXT NOT NULL,
             name TEXT,
             min_zoom INTEGER NOT NULL,
             geom BLOB NOT NULL
         );

         CREATE VIRTUAL TABLE IF NOT EXISTS features_rtree USING rtree(
             id,
             min_lon, max_lon,
             min_lat, max_lat
         );

         CREATE INDEX IF NOT EXISTS idx_features_zoom ON features(min_zoom);",
    )?;
    Ok(conn)
}

pub fn seed_demo_data_if_empty(conn: &mut Connection) -> rusqlite::Result<bool> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM features", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(false);
    }

    let tx = conn.transaction()?;
    {
        let mut stmt_f = tx.prepare(
            "INSERT INTO features (id, layer, name, min_zoom, geom) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut stmt_r = tx.prepare(
            "INSERT INTO features_rtree (id, min_lon, max_lon, min_lat, max_lat) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        let water_pts = vec![
            (-79.42, 43.62),
            (-79.35, 43.62),
            (-79.35, 43.642),
            (-79.42, 43.642),
            (-79.42, 43.62),
        ];
        let blob = bincode::serialize(&GeomType::Polygon(water_pts)).unwrap();
        stmt_f.execute(params![1, "water", Some("Lake Ontario"), 3, blob])?;
        stmt_r.execute(params![1, -79.42, -79.35, 43.62, 43.642])?;

        let label_blob = bincode::serialize(&GeomType::Point(-79.3832, 43.6532)).unwrap();
        stmt_f.execute(params![10, "label_city", Some("Demo City Center"), 3, label_blob])?;
        stmt_r.execute(params![10, -79.3832, -79.3832, 43.6532, 43.6532])?;
    }
    tx.commit()?;
    Ok(true)
}

fn classify_node<'a>(tags: impl Iterator<Item = (&'a str, &'a str)>) -> Option<(String, i32, String)> {
    let mut name = None;
    let mut category = None;
    let mut min_zoom = 14;

    for (k, v) in tags {
        match k {
            "name" | "name:zh" => {
                if name.is_none() || k == "name:zh" {
                    name = Some(v.to_string());
                }
            }
            "place" => match v {
                "city" => {
                    category = Some("label_city".to_string());
                    min_zoom = 4;
                }
                "town" | "county" | "district" => {
                    category = Some("label_town".to_string());
                    min_zoom = 8;
                }
                "suburb" | "village" => {
                    category = Some("label_suburb".to_string());
                    min_zoom = 11;
                }
                _ => {}
            },
            "tourism" | "historic" => {
                if matches!(v, "attraction" | "museum" | "viewpoint" | "theme_park" | "monument" | "scenic") {
                    category = Some("label_poi".to_string());
                    min_zoom = 11;
                }
            }
            "railway" if v == "station" => {
                category = Some("label_poi".to_string());
                min_zoom = 11;
            }
            _ => {}
        }
    }

    match (category, name) {
        (Some(cat), Some(n)) if !n.trim().is_empty() => Some((cat, min_zoom, n)),
        _ => None,
    }
}

fn classify_way(tags: osmpbf::TagIter) -> Option<(String, i32, bool, Option<String>)> {
    let mut layer = None;
    let mut min_zoom = 14;
    let mut is_polygon = false;
    let mut name = None;

    for (k, v) in tags {
        match k {
            "name" => name = Some(v.to_string()),
            "highway" => {
                layer = Some(v.to_string());
                min_zoom = match v {
                    "motorway" | "trunk" => 5,
                    "primary" => 7,
                    "secondary" => 9,
                    "tertiary" => 11,
                    _ => 13,
                };
            }
            "natural" if v == "water" => {
                layer = Some("water".to_string());
                min_zoom = 5;
                is_polygon = true;
            }
            "natural" => match v {
                "water" => {
                    layer = Some("water".to_string());
                    min_zoom = 5;
                    is_polygon = true;
                }
                "coastline" => {
                    layer = Some("coastline".to_string());
                    min_zoom = 3;
                    is_polygon = false;
                }
                _ => {}
            },
            "waterway" => match v {
                "riverbank" | "dock" => {
                    layer = Some("water".to_string());
                    min_zoom = 5;
                    is_polygon = true;
                }
                "river" | "canal" => {
                    layer = Some("water_line".to_string());
                    min_zoom = 6;
                    is_polygon = false;
                }
                "stream" => {
                    layer = Some("water_stream".to_string());
                    min_zoom = 11;
                    is_polygon = false;
                }
                _ => {}
            },
            "landuse" if v == "forest" || v == "park" || v == "grass" || v == "recreation_ground" => {
                layer = Some("park".to_string());
                min_zoom = 9;
                is_polygon = true;
            }
            "leisure" if v == "park" || v == "nature_reserve" => {
                layer = Some("park".to_string());
                min_zoom = 8;
                is_polygon = true;
            }
            "building" => {
                layer = Some("building".to_string());
                min_zoom = 14;
                is_polygon = true;
            }
            _ => {}
        }
    }
    layer.map(|l| (l, min_zoom, is_polygon, name))
}

fn classify_relation(tags: osmpbf::TagIter) -> Option<(&'static str, i32, Option<String>)> {
    let mut is_multipolygon = false;
    let mut layer = None;
    let mut min_zoom = 5;
    let mut name = None;

    for (k, v) in tags {
        match k {
            "type" if v == "multipolygon" || v == "boundary" => is_multipolygon = true,
            "name" => name = Some(v.to_string()),
            "natural" | "waterway" if v == "water" || v == "riverbank" => {
                layer = Some("water");
                min_zoom = 5;
            }
            "landuse" if v == "forest" || v == "park" => {
                layer = Some("park");
                min_zoom = 8;
            }
            _ => {}
        }
    }

    if is_multipolygon {
        layer.map(|l| (l, min_zoom, name))
    } else {
        None
    }
}
fn stitch_outer_rings(mut segments: Vec<Vec<(f64, f64)>>) -> Vec<Vec<(f64, f64)>> {
    let mut rings = Vec::new();
    while let Some(mut current) = segments.pop() {
        if current.len() < 2 {
            continue;
        }
        loop {
            let first = *current.first().unwrap();
            let last = *current.last().unwrap();
            if (first.0 - last.0).abs() < 1e-7 && (first.1 - last.1).abs() < 1e-7 && current.len() >= 4 {
                break;
            }
            let mut found_idx = None;
            let mut reverse_seg = false;
            for (i, seg) in segments.iter().enumerate() {
                if seg.is_empty() {
                    continue;
                }
                let s_first = *seg.first().unwrap();
                let s_last = *seg.last().unwrap();
                if (last.0 - s_first.0).abs() < 1e-7 && (last.1 - s_first.1).abs() < 1e-7 {
                    found_idx = Some(i);
                    reverse_seg = false;
                    break;
                } else if (last.0 - s_last.0).abs() < 1e-7 && (last.1 - s_last.1).abs() < 1e-7 {
                    found_idx = Some(i);
                    reverse_seg = true;
                    break;
                }
            }
            if let Some(idx) = found_idx {
                let mut next_seg = segments.swap_remove(idx);
                if reverse_seg {
                    next_seg.reverse();
                }
                current.extend(next_seg.into_iter().skip(1));
            } else {
                break;
            }
        }
        if current.len() >= 3 {
            rings.push(current);
        }
    }
    rings
}

pub fn import_pbf_to_sqlite<P: AsRef<Path>>(
    pbf_path: P,
    db_path: &str,
) -> Result<(usize, (f64, f64)), Box<dyn std::error::Error + Send + Sync>> {
    let mut conn = init_db(db_path)?;
    conn.execute_batch("DELETE FROM features; DELETE FROM features_rtree;")?;

    let mut rel_way_ids = FxHashSet::default();
    let mut water_relations: Vec<(i64, String, i32, Option<String>, Vec<i64>)> = Vec::new();
    let reader0 = ElementReader::from_path(&pbf_path)?;
    reader0.for_each(|element| {
        if let Element::Relation(rel) = element {
            if let Some((layer, min_zoom, name)) = classify_relation(rel.tags()) {
                let mut outer_ways = Vec::new();
                for member in rel.members() {
                    if member.member_type == RelMemberType::Way
                        && (member.role().unwrap_or("") == "outer" || member.role().unwrap_or("") == "")
                    {
                        rel_way_ids.insert(member.member_id);
                        outer_ways.push(member.member_id);
                    }
                }
                if !outer_ways.is_empty() {
                    water_relations.push((rel.id(), layer.to_string(), min_zoom, name, outer_ways));
                }
            }
        }
    })?;

    let mut needed_nodes = FxHashSet::default();
    let reader1 = ElementReader::from_path(&pbf_path)?;
    reader1.for_each(|element| {
        if let Element::Way(way) = element {
            if classify_way(way.tags()).is_some() || rel_way_ids.contains(&way.id()) {
                for node_id in way.refs() {
                    needed_nodes.insert(node_id);
                }
            }
        }
    })?;

    let mut node_coords: FxHashMap<i64, (f64, f64)> = FxHashMap::default();
    let mut label_nodes: Vec<(i64, String, i32, String, f64, f64)> = Vec::new();

    let reader2 = ElementReader::from_path(&pbf_path)?;
    reader2.for_each(|element| match element {
        Element::DenseNode(n) => {
            if needed_nodes.contains(&n.id()) {
                node_coords.insert(n.id(), (n.lon(), n.lat()));
            }
            if let Some((cat, min_z, name)) = classify_node(n.tags()) {
                label_nodes.push((n.id(), cat, min_z, name, n.lon(), n.lat()));
            }
        }
        Element::Node(n) => {
            if needed_nodes.contains(&n.id()) {
                node_coords.insert(n.id(), (n.lon(), n.lat()));
            }
            if let Some((cat, min_z, name)) = classify_node(n.tags()) {
                label_nodes.push((n.id(), cat, min_z, name, n.lon(), n.lat()));
            }
        }
        _ => {}
    })?;
    drop(needed_nodes);
    let mut imported_count = 0usize;
    let mut center = (112.61, 26.89);
    let mut rel_way_geoms: FxHashMap<i64, Vec<(f64, f64)>> = FxHashMap::default();

    let tx = conn.transaction()?;
    {
        let mut stmt_f = tx.prepare(
            "INSERT OR REPLACE INTO features (id, layer, name, min_zoom, geom) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        let mut stmt_r = tx.prepare(
            "INSERT OR REPLACE INTO features_rtree (id, min_lon, max_lon, min_lat, max_lat) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;

        for (id, cat, min_z, name, lon, lat) in label_nodes {
            if cat == "label_city" {
                center = (lon, lat);
            }
            if let Ok(blob) = bincode::serialize(&GeomType::Point(lon, lat)) {
                let _ = stmt_f.execute(params![id, cat, Some(name), min_z, blob]);
                let _ = stmt_r.execute(params![id, lon, lon, lat, lat]);
                imported_count += 1;
            }
        }

        let reader3 = ElementReader::from_path(&pbf_path)?;
        reader3.for_each(|element| {
            if let Element::Way(way) = element {
                let is_rel_member = rel_way_ids.contains(&way.id());
                let way_class = classify_way(way.tags());

                if way_class.is_some() || is_rel_member {
                    let mut coords = Vec::new();
                    let (mut min_lon, mut max_lon) = (180.0_f64, -180.0_f64);
                    let (mut min_lat, mut max_lat) = (90.0_f64, -90.0_f64);

                    for node_id in way.refs() {
                        if let Some(&(lon, lat)) = node_coords.get(&node_id) {
                            coords.push((lon, lat));
                            min_lon = min_lon.min(lon);
                            max_lon = max_lon.max(lon);
                            min_lat = min_lat.min(lat);
                            max_lat = max_lat.max(lat);
                        }
                    }

                    if is_rel_member && coords.len() >= 2 {
                        rel_way_geoms.insert(way.id(), coords.clone());
                    }

                    if let Some((layer, min_zoom, is_polygon, name)) = way_class {
                        if coords.len() >= 2 {
                            let way_db_id = -way.id();
                            if let Some(ref feat_name) = name {
                                if matches!(layer.as_str(), "motorway" | "trunk" | "primary" | "water_line") {
                                    let mid = coords[coords.len() / 2];
                                    let label_id = way_db_id - 10_000_000_000;
                                    if let Ok(p_blob) = bincode::serialize(&GeomType::Point(mid.0, mid.1)) {
                                        let _ = stmt_f.execute(params![
                                            label_id,
                                            "label_road",
                                            Some(feat_name),
                                            (min_zoom + 4).min(13),
                                            p_blob
                                        ]);
                                        let _ = stmt_r.execute(params![label_id, mid.0, mid.0, mid.1, mid.1]);
                                    }
                                }
                            }

                            let geom = if is_polygon && coords.len() >= 3 {
                                GeomType::Polygon(coords)
                            } else {
                                GeomType::LineString(coords)
                            };
                            if let Ok(blob) = bincode::serialize(&geom) {
                                let _ = stmt_f.execute(params![way_db_id, layer, name, min_zoom, blob]);
                                let _ = stmt_r.execute(params![way_db_id, min_lon, max_lon, min_lat, max_lat]);
                                imported_count += 1;
                            }
                        }
                    }
                }
            }
        })?;

        let mut rel_seq = 0i64;
        for (rel_id, layer, min_zoom, name, outer_ways) in water_relations {
            let mut segs = Vec::new();
            for wid in outer_ways {
                if let Some(pts) = rel_way_geoms.get(&wid) {
                    segs.push(pts.clone());
                }
            }
            for ring in stitch_outer_rings(segs) {
                let (mut min_lon, mut max_lon) = (180.0_f64, -180.0_f64);
                let (mut min_lat, mut max_lat) = (90.0_f64, -90.0_f64);
                for &(lon, lat) in &ring {
                    min_lon = min_lon.min(lon);
                    max_lon = max_lon.max(lon);
                    min_lat = min_lat.min(lat);
                    max_lat = max_lat.max(lat);
                }
                rel_seq += 1;
                let db_id = -(rel_id * 1000 + rel_seq + 50_000_000_000);
                if let Ok(blob) = bincode::serialize(&GeomType::Polygon(ring)) {
                    let _ = stmt_f.execute(params![db_id, layer, name, min_zoom, blob]);
                    let _ = stmt_r.execute(params![db_id, min_lon, max_lon, min_lat, max_lat]);
                    imported_count += 1;
                }
            }
        }
    }
    tx.commit()?;
    Ok((imported_count, center))
}

/// 按传入的 MapBounds 范围参数查询空间索引
pub fn query_bounds(db_path: &str, bounds: MapBounds) -> rusqlite::Result<Vec<RawFeature>> {
    let conn = Connection::open(db_path)?;
    let mut stmt = conn.prepare_cached(
        "SELECT f.layer, f.name, f.min_zoom, f.geom
         FROM features_rtree r
         JOIN features f ON r.id = f.id
         WHERE r.min_lon <= ?1 AND r.max_lon >= ?2
           AND r.min_lat <= ?3 AND r.max_lat >= ?4
           AND f.min_zoom <= ?5
         ORDER BY f.min_zoom ASC
         LIMIT 25000",
    )?;

    let rows = stmt.query_map(
        params![
            bounds.max_lon,
            bounds.min_lon,
            bounds.max_lat,
            bounds.min_lat,
            bounds.zoom + 1
        ],

        |row| {
            let layer: String = row.get(0)?;
            let name: Option<String> = row.get(1)?;
            let min_zoom: i32 = row.get(2)?;
            let blob: Vec<u8> = row.get(3)?;
            let geom: GeomType = bincode::deserialize(&blob).unwrap_or(GeomType::LineString(vec![]));
            Ok(RawFeature {
                layer,
                name,
                min_zoom,
                geom,
            })
        },
    )?;

    let mut result = Vec::new();
    for item in rows.flatten() {
        result.push(item);
    }
    Ok(result)
}

