mod map_renderer;
mod note_manager;
mod osm_index;
mod pathfinder;
mod photo_manager;
mod plugin;
mod style_config;
mod ui;

use galileo_egui::InitBuilder;
use map_renderer::create_map_with_layers;
use osm_index::{import_pbf_to_sqlite, init_db, seed_demo_data_if_empty};
use parking_lot::RwLock;
use rusqlite::Connection;
use std::path::Path;
use std::sync::Arc;
use style_config::{MapStyleConfig, STYLE_CONFIG_PATH};
use ui::{OfflineMapApp, DB_PATH};

const DEFAULT_PBF_PATH: &str = "default.osm.pbf";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let target_pbf = std::env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PBF_PATH.to_string());

    let conn = init_db(DB_PATH)?;
    let existing_count: i64 = conn.query_row("SELECT COUNT(*) FROM features", [], |r| r.get(0))?;
    drop(conn);

    let mut initial_center = (26.893, 112.612);

    if existing_count == 0 {
        if Path::new(&target_pbf).exists() {
            println!("正在从 '{}' 构建空间索引...", target_pbf);
            if let Ok((total, (lon, lat))) = import_pbf_to_sqlite(&target_pbf, DB_PATH) {
                println!("导入完成，共 {} 条要素！", total);
                initial_center = (lat, lon);
            }
        } else {
            let mut conn = init_db(DB_PATH)?;
            seed_demo_data_if_empty(&mut conn)?;
        }
    } else if let Ok(conn) = Connection::open(DB_PATH) {
        if let Ok((min_lon, max_lon, min_lat, max_lat)) = conn.query_row(
            "SELECT min_lon, max_lon, min_lat, max_lat FROM features_rtree LIMIT 1",
            [],

            |r| Ok((r.get::<_, f64>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?, r.get::<_, f64>(3)?)),
        ) {
            initial_center = ((min_lat + max_lat) / 2.0, (min_lon + max_lon) / 2.0);
        }
    }

    // 加载或初始化独立的 map_style.json 样式配置
    let style = Arc::new(RwLock::new(MapStyleConfig::load_or_create(STYLE_CONFIG_PATH)));

    let (map, layers) = create_map_with_layers(initial_center.0, initial_center.1, Arc::clone(&style));

    InitBuilder::new(map)
        .with_app_builder(move |egui_map_state| {
            Box::new(OfflineMapApp::new(egui_map_state, layers, style))
        })
        .init()?;

    Ok(())
}

