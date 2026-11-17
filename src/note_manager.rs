use crate::photo_manager::{PhotoManager, PhotoRecord};
use chrono::{Duration, NaiveTime};
use rusqlite::{params, Connection};

#[derive(Clone, Debug)]
pub struct TripStopNote {
    pub id: i64,
    pub day_index: i32,
    pub order_idx: i32,
    pub name: String,
    pub lon: f64,
    pub lat: f64,
    pub stay_minutes: i32,
    pub travel_km_from_prev: f64,
    pub travel_min_from_prev: f64,
    pub computed_arrival: String,
    pub computed_departure: String,
    pub notes: String,
    pub photos: Vec<PhotoRecord>,
}

/// 5. 笔记与多日行程管理模块：负责行程节点的 CRUD、Markdown 笔记持久化与时间顺延计算
pub struct NoteManager {
    db_path: String,
}

impl NoteManager {
    pub fn new(db_path: &str) -> rusqlite::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS itinerary_stops (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 day_index INTEGER NOT NULL DEFAULT 1,
                 order_idx INTEGER NOT NULL DEFAULT 0,
                 name TEXT NOT NULL,
                 lon REAL NOT NULL,
                 lat REAL NOT NULL,
                 stay_minutes INTEGER NOT NULL DEFAULT 60,
                 notes TEXT NOT NULL DEFAULT ''
             );",
        )?;
        Ok(Self {
            db_path: db_path.to_string(),
        })
    }

    pub fn load_all(&self, photo_mgr: &PhotoManager) -> rusqlite::Result<Vec<TripStopNote>> {
        let conn = Connection::open(&self.db_path)?;
        let mut stmt = conn.prepare(
            "SELECT id, day_index, order_idx, name, lon, lat, stay_minutes, notes
             FROM itinerary_stops
             ORDER BY day_index ASC, order_idx ASC, id ASC",
        )?;

        let mut stops: Vec<TripStopNote> = stmt
            .query_map([], |row| {
                Ok(TripStopNote {
                    id: row.get(0)?,
                    day_index: row.get(1)?,
                    order_idx: row.get(2)?,
                    name: row.get(3)?,
                    lon: row.get(4)?,
                    lat: row.get(5)?,
                    stay_minutes: row.get(6)?,
                    travel_km_from_prev: 0.0,
                    travel_min_from_prev: 0.0,
                    computed_arrival: String::new(),
                    computed_departure: String::new(),
                    notes: row.get(7)?,
                    photos: Vec::new(),
                })
            })?
            .flatten()
            .collect();

        for stop in &mut stops {
            stop.photos = photo_mgr.load_photos_for_stop(stop.id).unwrap_or_default();
        }
        Ok(stops)
    }

    pub fn create_stop(
        &self,
        day_index: i32,
        order_idx: i32,
        name: &str,
        lon: f64,
        lat: f64,
    ) -> rusqlite::Result<TripStopNote> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "INSERT INTO itinerary_stops (day_index, order_idx, name, lon, lat, stay_minutes, notes)
             VALUES (?1, ?2, ?3, ?4, ?5, 60, '')",
            params![day_index, order_idx, name, lon, lat],
        )?;
        Ok(TripStopNote {
            id: conn.last_insert_rowid(),
            day_index,
            order_idx,
            name: name.to_string(),
            lon,
            lat,
            stay_minutes: 60,
            travel_km_from_prev: 0.0,
            travel_min_from_prev: 0.0,
            computed_arrival: String::new(),
            computed_departure: String::new(),
            notes: String::new(),
            photos: Vec::new(),
        })
    }

    pub fn update_stop(&self, stop: &TripStopNote) -> rusqlite::Result<()> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute(
            "UPDATE itinerary_stops
             SET day_index = ?1, order_idx = ?2, name = ?3, stay_minutes = ?4, notes = ?5
             WHERE id = ?6",
            params![
                stop.day_index,
                stop.order_idx,
                stop.name,
                stop.stay_minutes,
                stop.notes,
                stop.id
            ],
        )?;
        Ok(())
    }

    pub fn remove_stop(&self, stop_id: i64, photo_mgr: &PhotoManager) -> rusqlite::Result<()> {
        photo_mgr.delete_photos_for_stop(stop_id)?;
        let conn = Connection::open(&self.db_path)?;
        conn.execute("DELETE FROM itinerary_stops WHERE id = ?1", params![stop_id])?;
        Ok(())
    }

    /// 根据每天起始时间 (如 "09:00")、路程耗时与停留时长，自动顺延计算全天到达/离开时刻
    pub fn recompute_timelines(stops: &mut [TripStopNote], day_start_hhmm: &str) {
        let base_time = NaiveTime::parse_from_str(day_start_hhmm, "%H:%M")
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(9, 0, 0).unwrap());

        let mut current_day = -1;
        let mut cursor_time = base_time;

        for (idx, stop) in stops.iter_mut().enumerate() {
            if stop.day_index != current_day {
                current_day = stop.day_index;
                cursor_time = base_time;
                stop.travel_km_from_prev = 0.0;
                stop.travel_min_from_prev = 0.0;
            } else if idx > 0 {
                cursor_time += Duration::minutes(stop.travel_min_from_prev.round() as i64);
            }

            stop.computed_arrival = cursor_time.format("%H:%M").to_string();
            cursor_time += Duration::minutes(stop.stay_minutes.max(0) as i64);
            stop.computed_departure = cursor_time.format("%H:%M").to_string();
        }
    }
}

