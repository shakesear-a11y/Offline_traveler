# Rust Offline Vector Map & Travel Planner

A **100% pure Rust, zero-cloud-dependency, single-file SQLite-backed** offline vector map engine, route planner, and travel journal desktop application.

Without requiring an internet connection or any online tile servers, it directly parses raw OpenStreetMap `.osm.pbf` files to deliver millisecond-level spatial rendering, offline A* pathfinding, multi-day itinerary scheduling with automatic timeline propagation, local EXIF photo archiving, and an extensible geospatial statistics plugin system.

---

## Key Features

### 1. Offline Vector Map & Spatial Indexing Engine (`osm_index` & `map_renderer`)
* **Native `.osm.pbf` Parsing**: Import OSM PBF data packages for any city, province, or country directly inside the app without external conversion tools. Automatically extracts water bodies, coastlines, parks, multi-level road networks, building footprints, and multilingual place labels.
* **SQLite R-Tree Spatial Index**: Uses a local SQLite R-Tree virtual table in WAL mode to support millisecond-level viewport (`MapBounds`) queries and zoom-level feature filtering across millions of geographic features.
* **Land-Sea Base Layer**: Loads a global/regional land polygon GeoJSON as a permanent base layer to prevent rendering artifacts in far-zoom or coastal views.
* **Custom POIs & GPX Tracks**: Right-click anywhere on the map to bookmark custom Points of Interest (persisted to the local database), and import/render standard `.gpx` outdoor GPS tracks.

### 2. Pure Rust Offline A* Route Planner (`pathfinder`)
* **Local Road Topology Graph**: Automatically builds a directed road network graph from imported OSM road features, featuring a spatial grid index for millisecond nearest-junction snapping.
* **Multi-Mode Routing**: Supports both **Driving** (with road-class speed limits) and **Walking** modes, computing optimal routes, total distance, and estimated travel duration in real time via a Haversine-heuristic A* algorithm.

### 3. Multi-Day Itinerary & Offline Travel Journal (`note_manager` & `photo_manager`)
* **Automatic Timeline Propagation**: Switch across D1–D5 multi-day itineraries, set a daily departure time, and adjust stop stay durations (`Stay Minutes`). The system automatically calculates and cascades exact arrival and departure times for every stop based on travel durations.
* **Local Photo Archiving & EXIF Extraction**: Write multi-line travel notes for each stop and attach local photos. Automatically reads EXIF original capture timestamps (`DateTimeOriginal`), copies files to a local `trip_media/` directory for portability, and caches GPU thumbnail textures.

### 4. Dynamic Map Style Configuration (`style_config` & `style_panel`)
* **Real-Time Visual Styling**: Includes three built-in theme presets—**Classic Light**, **Dark Night**, and **Retro Paper**—alongside interactive color pickers and stroke width sliders to customize every map layer on the fly.
* **External JSON Configuration**: Bidirectional sync with `map_style.json` in the project root. Save GUI tweaks to disk or edit the JSON file manually and reload at runtime.

### 5. Extensible Geospatial Statistics Plugin System (`plugin`)
* **Unified Plugin Interface (`MapDataPlugin`)**: Overlay choropleth polygons (`ChoroplethPolygon`) and proportional symbol bubbles (`StatBubble`) onto the map, complete with an adaptive bottom-right legend card and hover tooltips.
* **Built-In & Zero-Code Data Ingestion**:
  * **Birth Rate Demo Plugin**: Interactive 2020–2026 year slider with live switching between proportional bubbles and grid choropleths.
  * **Universal External Data Plugin**: Import any **`.geojson`** regional dataset or **`.csv`** table with latitude/longitude and numeric columns directly through the UI—no Rust coding required. Automatically computes quartile bins, maps color palettes, and generates legends.

---

## Project Structure

```text
rust_map_routing/
├── Cargo.toml                 # Project dependencies
├── map_style.json             # Map visual style configuration (auto-generated)
├── ne_50m_land.geojson        # Global/regional land polygon base map (download separately)
└── src/
    ├── main.rs                # Application entry point & database initialization
    ├── osm_index.rs           # Module 1: .osm.pbf parser & SQLite R-Tree spatial indexer
    ├── map_renderer.rs        # Module 2: Bounds-based renderer, dynamic symbols, custom POIs & GPX tracks
    ├── pathfinder.rs          # Module 3: Road graph builder & A* shortest-time pathfinder
    ├── photo_manager.rs       # Module 4: EXIF metadata parser, media archiver & thumbnail texture cache
    ├── note_manager.rs        # Module 5: Multi-day itinerary manager, notes persistence & timeline calculator
    ├── style_config.rs        # Style serialization & built-in theme presets
    ├── plugin/                # Thematic data plugin system
    │   ├── mod.rs             # Plugin trait definitions & PluginManager registry
    │   ├── birth_rate.rs      # Built-in population birth rate demo plugin
    │   └── external_data.rs   # Universal GeoJSON / CSV external statistics plugin
    └── ui/                    # egui desktop GUI layer
        ├── mod.rs             # Main state container OfflineMapApp & async task coordinator
        ├── left_panel.rs      # Left control panel (PBF indexer, custom POIs, GPX tracks)
        ├── right_panel.rs     # Right panel (itinerary timeline, notes & photo editor)
        ├── map_overlay.rs     # Map canvas overlay (labels, click interactions, stat bubbles & legend)
        ├── style_panel.rs     # Map style color pickers & theme switcher
        └── plugin_panel.rs    # Plugin toggles, parameter sliders & external file loader
```

---

## Getting Started

### 1. Prerequisites
* Install the [Rust toolchain](https://rust-lang.org) (Rust 1.75 or later recommended).
* **Linux System Dependencies** (Ubuntu / Debian): Install windowing, graphics, and CJK font packages so map labels render properly:
  ```bash
  sudo apt-get update
  sudo apt-get install -y libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev libgtk-3-dev fonts-noto-cjk
  ```

### 2. Download the Base Land Polygon Layer
From below URL to download the Natural Earth 50m land polygon GeoJSON (~1.8 MB):
```URL
https://github.com/nvkelso/natural-earth-vector/blob/master/geojson/ne_10m_land.geojson
```

### 3. Prepare an Offline Map Package (`.osm.pbf`)
Download any regional or country `.osm.pbf` extract from [Geofabrik](https://geofabrik.de) (e.g., `ontario-latest.osm.pbf` or `guangdong-latest.osm.pbf`). Place it in the project root as `default.osm.pbf`, or import it manually after launch via the top-left button in the UI.

### 4. Build and Run
Always compile and run in `--release` mode for optimal PBF parsing and R-Tree rendering performance:
```bash
cargo run --release
```

---

## Usage Guide

* **Map Navigation**: Hold **Left Click + Drag** to pan; scroll the **Mouse Wheel** to zoom.
* **Add Itinerary Stops**: **Right-click** anywhere on the map, or **Left-click** any city/landmark dot, to append the location to the currently selected day (D1–D5) and trigger automatic A* routing.
* **Custom POIs**: After right-clicking a spot on the map, enter a name under `2a. Custom POIs` in the left panel and click `Save POI` to permanently pin it.
* **Import External CSV Statistics**:
  Create a `.csv` file containing `name,lon,lat` along with any numeric data columns:
  ```csv
  name,lon,lat,GDP_Billion,Birth_Rate
  Toronto,-79.3832,43.6532,452.5,9.4
  Vancouver,-123.1207,49.2827,185.3,8.1
  Montreal,-73.5673,45.5017,245.8,9.8
  ```
  Expand `4. Data Plugins (Statistics)` in the left panel and click `Load .geojson / .csv File...` to automatically generate proportional stat bubbles and a dynamic legend.

---

## Tech Stack

* **GUI Framework**: [`eframe` / `egui`](https://github.com)
* **Vector Map Engine**: [`galileo` & `galileo-egui`](https://github.com)
* **Spatial Database**: [`rusqlite`](https://github.com) (with SQLite R-Tree extension enabled)
* **Geospatial Parsing**: [`osmpbf`](https://github.com), [`geojson`](https://github.com), [`gpx`](https://github.com)
* **Image & Metadata Processing**: [`image`](https://github.com), [`kamadak-exif`](https://github.com)

## License

This project is licensed under the [MIT License](LICENSE).

