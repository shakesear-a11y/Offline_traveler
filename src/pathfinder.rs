use crate::osm_index::GeomType;
use rusqlite::Connection;
use rustc_hash::FxHashMap;
use std::cmp::Ordering;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TravelMode {
    Driving,
    Walking,
}

#[derive(Clone, Debug)]
pub struct RoutePoint {
    pub lon: f64,
    pub lat: f64,
}

#[derive(Clone, Debug)]
pub struct RouteResult {
    pub distance_km: f64,
    pub duration_min: f64,
    pub path_coords: Vec<(f64, f64)>, // (lon, lat)
}

#[derive(Clone)]
struct Edge {
    to: usize,
    drive_sec: f64,
    walk_sec: f64,
}

pub struct PathFinder {
    nodes: Vec<(f64, f64)>,
    adj: Vec<Vec<Edge>>,
    grid: FxHashMap<(i32, i32), Vec<usize>>,
}

pub fn haversine_meters(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let r = 6371000.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

fn road_speed_kmh(layer: &str) -> f64 {
    match layer {
        "motorway" => 100.0,
        "trunk" => 80.0,
        "primary" => 60.0,
        "secondary" => 50.0,
        "tertiary" => 40.0,
        "residential" | "living_street" | "unclassified" => 30.0,
        _ => 20.0,
    }
}

impl PathFinder {
    pub fn load_from_sqlite(db_path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        let mut stmt = conn.prepare(
            "SELECT layer, geom FROM features 
             WHERE layer NOT IN ('water', 'water_line', 'water_stream', 'park', 'building')
               AND layer NOT LIKE 'label_%'",
        )?;

        let mut coord_to_id: FxHashMap<(i64, i64), usize> = FxHashMap::default();
        let mut nodes: Vec<(f64, f64)> = Vec::new();
        let mut adj: Vec<Vec<Edge>> = Vec::new();
        let mut grid: FxHashMap<(i32, i32), Vec<usize>> = FxHashMap::default();

        let rows = stmt.query_map([], |row| {
            let layer: String = row.get(0)?;
            let blob: Vec<u8> = row.get(1)?;
            Ok((layer, blob))
        })?;

        for row in rows.flatten() {
            let (layer, blob) = row;
            if let Ok(GeomType::LineString(pts)) = bincode::deserialize::<GeomType>(&blob) {
                let speed_mps = (road_speed_kmh(&layer) * 1000.0) / 3600.0;
                let walk_mps = (5.0 * 1000.0) / 3600.0;

                for win in pts.windows(2) {
                    let (lon1, lat1) = win[0];
                    let (lon2, lat2) = win[1];

                    let u = Self::get_or_insert_node(
                        lon1,
                        lat1,
                        &mut coord_to_id,
                        &mut nodes,
                        &mut adj,
                        &mut grid,
                    );
                    let v = Self::get_or_insert_node(
                        lon2,
                        lat2,
                        &mut coord_to_id,
                        &mut nodes,
                        &mut adj,
                        &mut grid,
                    );

                    if u == v {
                        continue;
                    }
                    let dist = haversine_meters(lon1, lat1, lon2, lat2).max(0.5);
                    let drive_sec = dist / speed_mps;
                    let walk_sec = dist / walk_mps;

                    adj[u].push(Edge { to: v, drive_sec, walk_sec });
                    adj[v].push(Edge { to: u, drive_sec, walk_sec });
                }
            }
        }

        Ok(Self { nodes, adj, grid })
    }

    fn get_or_insert_node(
        lon: f64,
        lat: f64,
        coord_to_id: &mut FxHashMap<(i64, i64), usize>,
        nodes: &mut Vec<(f64, f64)>,
        adj: &mut Vec<Vec<Edge>>,
        grid: &mut FxHashMap<(i32, i32), Vec<usize>>,
    ) -> usize {
        let key = (
            (lon * 1_000_000.0).round() as i64,
            (lat * 1_000_000.0).round() as i64,
        );
        if let Some(&id) = coord_to_id.get(&key) {
            id
        } else {
            let id = nodes.len();
            nodes.push((lon, lat));
            adj.push(Vec::new());
            coord_to_id.insert(key, id);
            let cell = ((lon * 100.0).floor() as i32, (lat * 100.0).floor() as i32);
            grid.entry(cell).or_default().push(id);
            id
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn snap_nearest(&self, lon: f64, lat: f64) -> Option<usize> {
        let cx = (lon * 100.0).floor() as i32;
        let cy = (lat * 100.0).floor() as i32;

        let mut best_id = None;
        let mut best_dist = f64::MAX;

        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(bucket) = self.grid.get(&(cx + dx, cy + dy)) {
                    for &nid in bucket {
                        let (nlon, nlat) = self.nodes[nid];
                        let d = haversine_meters(lon, lat, nlon, nlat);
                        if d < best_dist {
                            best_dist = d;
                            best_id = Some(nid);
                        }
                    }
                }
            }
        }
        best_id
    }

    /// A* 最短时间寻路算法
    pub fn find_path(&self, start: RoutePoint, end: RoutePoint, mode: TravelMode) -> RouteResult {
        let max_speed_mps = match mode {
            TravelMode::Driving => (100.0 * 1000.0) / 3600.0,
            TravelMode::Walking => (5.0 * 1000.0) / 3600.0,
        };

        let fallback = || {
            let dist_m = haversine_meters(start.lon, start.lat, end.lon, end.lat);
            RouteResult {
                distance_km: dist_m / 1000.0,
                duration_min: (dist_m / max_speed_mps) / 60.0,
                path_coords: vec![(start.lon, start.lat), (end.lon, end.lat)],
            }
        };

        let (Some(start_id), Some(goal_id)) = (
            self.snap_nearest(start.lon, start.lat),
            self.snap_nearest(end.lon, end.lat),
        ) else {
            return fallback();
        };

        if start_id == goal_id {
            return RouteResult {
                distance_km: 0.0,
                duration_min: 0.0,
                path_coords: vec![(start.lon, start.lat), (end.lon, end.lat)],
            };
        }

        #[derive(Copy, Clone, PartialEq)]
        struct State {
            f_score: f64,
            node: usize,
        }
        impl Eq for State {}
        impl Ord for State {
            fn cmp(&self, other: &Self) -> Ordering {
                other.f_score.partial_cmp(&self.f_score).unwrap_or(Ordering::Equal)
            }
        }
        impl PartialOrd for State {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        let (goal_lon, goal_lat) = self.nodes[goal_id];
        let mut g_score: FxHashMap<usize, f64> = FxHashMap::default();
        let mut came_from: FxHashMap<usize, usize> = FxHashMap::default();
        let mut open_set = BinaryHeap::new();

        g_score.insert(start_id, 0.0);
        let h0 = haversine_meters(self.nodes[start_id].0, self.nodes[start_id].1, goal_lon, goal_lat) / max_speed_mps;
        open_set.push(State { f_score: h0, node: start_id });

        let mut found = false;
        let mut visited = 0usize;

        while let Some(State { node: current, .. }) = open_set.pop() {
            if current == goal_id {
                found = true;
                break;
            }
            visited += 1;
            if visited > 300_000 {
                break;
            }

            let cur_g = *g_score.get(&current).unwrap_or(&f64::MAX);
            for edge in &self.adj[current] {
                let weight = match mode {
                    TravelMode::Driving => edge.drive_sec,
                    TravelMode::Walking => edge.walk_sec,
                };
                let tentative_g = cur_g + weight;
                if tentative_g < *g_score.get(&edge.to).unwrap_or(&f64::MAX) {
                    came_from.insert(edge.to, current);
                    g_score.insert(edge.to, tentative_g);
                    let (nlon, nlat) = self.nodes[edge.to];
                    let h = haversine_meters(nlon, nlat, goal_lon, goal_lat) / max_speed_mps;
                    open_set.push(State {
                        f_score: tentative_g + h,
                        node: edge.to,
                    });
                }
            }
        }

        if !found {
            return fallback();
        }

        let mut path_nodes = vec![goal_id];
        let mut curr = goal_id;
        while let Some(&prev) = came_from.get(&curr) {
            path_nodes.push(prev);
            curr = prev;
        }
        path_nodes.reverse();

        let mut total_dist_m = 0.0;
        let mut path_coords = Vec::with_capacity(path_nodes.len() + 2);
        path_coords.push((start.lon, start.lat));

        for win in path_nodes.windows(2) {
            let p1 = self.nodes[win[0]];
            let p2 = self.nodes[win[1]];
            total_dist_m += haversine_meters(p1.0, p1.1, p2.0, p2.1);
            path_coords.push(p1);
        }
        if let Some(&last_nid) = path_nodes.last() {
            path_coords.push(self.nodes[last_nid]);
        }
        path_coords.push((end.lon, end.lat));

        let total_sec = *g_score.get(&goal_id).unwrap_or(&0.0);
        RouteResult {
            distance_km: total_dist_m / 1000.0,
            duration_min: total_sec / 60.0,
            path_coords,
        }
    }
}

