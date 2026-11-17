use galileo::Color;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

pub const STYLE_CONFIG_PATH: &str = "map_style.json";

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct RgbaColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl RgbaColor {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub fn to_galileo(self) -> Color {
        Color::rgba(self.r, self.g, self.b, self.a)
    }

    pub fn to_egui_array(self) -> [u8; 4] {
        [self.r, self.g, self.b, self.a]
    }

    pub fn from_egui_array(arr: [u8; 4]) -> Self {
        Self {
            r: arr[0],
            g: arr[1],
            b: arr[2],
            a: arr[3],
        }
    }
}

/// 完整的地图视觉样式配置 (支持 map_style.json 读写与 GUI 实时调节)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MapStyleConfig {
    pub theme_name: String,
    pub ocean_color: RgbaColor,
    pub land_color: RgbaColor,
    pub water_fill: RgbaColor,
    pub water_line_color: RgbaColor,
    pub water_line_width: f64,
    pub park_fill: RgbaColor,
    pub building_fill: RgbaColor,
    pub minor_road_color: RgbaColor,
    pub minor_road_width: f64,
    pub major_road_color: RgbaColor,
    pub major_road_width: f64,
    pub gps_track_color: RgbaColor,
    pub gps_track_width: f64,
    pub route_color: RgbaColor,
    pub route_width: f64,
}

impl Default for MapStyleConfig {
    fn default() -> Self {
        Self::preset_light()
    }
}

impl MapStyleConfig {
    /// 预设 1：经典日间浅色
    pub fn preset_light() -> Self {
        Self {
            theme_name: "Classic Light".to_string(),
            ocean_color: RgbaColor::new(165, 215, 255, 255),
            land_color: RgbaColor::new(245, 243, 238, 255),
            water_fill: RgbaColor::new(165, 215, 255, 255),
            water_line_color: RgbaColor::new(145, 200, 250, 255),
            water_line_width: 2.5,
            park_fill: RgbaColor::new(195, 245, 200, 255),
            building_fill: RgbaColor::new(218, 213, 204, 255),
            minor_road_color: RgbaColor::new(185, 180, 172, 255),
            minor_road_width: 2.0,
            major_road_color: RgbaColor::new(248, 172, 85, 255),
            major_road_width: 3.2,
            gps_track_color: RgbaColor::new(170, 50, 230, 220),
            gps_track_width: 4.0,
            route_color: RgbaColor::new(35, 115, 245, 235),
            route_width: 5.5,
        }
    }

    /// 预设 2：暗夜深色模式
    pub fn preset_dark() -> Self {
        Self {
            theme_name: "Dark Night".to_string(),
            ocean_color: RgbaColor::new(20, 35, 55, 255),
            land_color: RgbaColor::new(34, 38, 44, 255),
            water_fill: RgbaColor::new(22, 42, 68, 255),
            water_line_color: RgbaColor::new(35, 70, 110, 255),
            water_line_width: 2.5,
            park_fill: RgbaColor::new(32, 58, 45, 255),
            building_fill: RgbaColor::new(48, 53, 62, 255),
            minor_road_color: RgbaColor::new(72, 78, 90, 255),
            minor_road_width: 1.8,
            major_road_color: RgbaColor::new(210, 130, 50, 255),
            major_road_width: 3.2,
            gps_track_color: RgbaColor::new(215, 95, 255, 230),
            gps_track_width: 4.0,
            route_color: RgbaColor::new(65, 165, 255, 245),
            route_width: 5.5,
        }
    }

    /// 预设 3：复古羊皮纸地形风
    pub fn preset_retro() -> Self {
        Self {
            theme_name: "Retro Paper".to_string(),
            ocean_color: RgbaColor::new(175, 205, 212, 255),
            land_color: RgbaColor::new(238, 228, 205, 255),
            water_fill: RgbaColor::new(170, 202, 210, 255),
            water_line_color: RgbaColor::new(135, 175, 185, 255),
            water_line_width: 2.5,
            park_fill: RgbaColor::new(198, 212, 168, 255),
            building_fill: RgbaColor::new(212, 198, 172, 255),
            minor_road_color: RgbaColor::new(185, 170, 145, 255),
            minor_road_width: 1.8,
            major_road_color: RgbaColor::new(205, 110, 75, 255),
            major_road_width: 3.2,
            gps_track_color: RgbaColor::new(145, 45, 160, 220),
            gps_track_width: 4.0,
            route_color: RgbaColor::new(30, 110, 155, 240),
            route_width: 5.5,
        }
    }

    /// 从 JSON 配置文件加载样式；若文件不存在则自动生成默认配置文件
    pub fn load_or_create<P: AsRef<Path>>(path: P) -> Self {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(cfg) = serde_json::from_str::<MapStyleConfig>(&content) {
                return cfg;
            }
        }
        let default_cfg = Self::default();
        let _ = default_cfg.save_to_file(path);
        default_cfg
    }

    /// 保存当前样式到 JSON 配置文件
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string_pretty(self)?;
        fs::write(path, json)?;
        Ok(())
    }
}

