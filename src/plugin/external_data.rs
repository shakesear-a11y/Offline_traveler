use super::{LegendItem, MapDataPlugin, PluginParam, PluginRenderPacket, PluginVisualItem};
use crate::osm_index::MapBounds;
use crate::style_config::RgbaColor;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug)]
enum RawStatGeom {
    Point(f64, f64),
    Polygon(Vec<(f64, f64)>),
}

#[derive(Clone, Debug)]
struct StatRecord {
    name: String,
    geom: RawStatGeom,
    metrics: HashMap<String, f64>, // 字段名 -> 数值
}

/// 通用外部地理统计插件：支持无代码加载 GeoJSON 与 CSV 统计文件
pub struct ExternalDataPlugin {
    dataset_name: String,
    records: Vec<StatRecord>,
    metric_keys: Vec<String>,
    params: HashMap<String, PluginParam>,
}

impl ExternalDataPlugin {
    pub fn empty() -> Self {
        let mut params = HashMap::new();
        params.insert(
            "metric".to_string(),
            PluginParam::Select {
                label: "统计指标".to_string(),
                options: vec!["(未加载数据)".to_string()],
                selected: 0,
            },
        );
        params.insert(
            "palette".to_string(),
            PluginParam::Select {
                label: "配色方案".to_string(),
                options: vec!["暖红橙".to_string(), "海洋蓝".to_string(), "翠绿黄".to_string()],
                selected: 0,
            },
        );
        Self {
            dataset_name: "未加载外部文件".to_string(),
            records: Vec::new(),
            metric_keys: Vec::new(),
            params,
        }
    }

    /// 从外部 .geojson 或 .csv 文件加载统计数据并自动提取数值列
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, String> {
        let path_ref = path.as_ref();
        let ext = path_ref
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        let content = fs::read_to_string(path_ref).map_err(|e| format!("读取文件失败: {}", e))?;
        let parsed = match ext.as_str() {
            "geojson" | "json" => Self::parse_geojson(&content)?,
            "csv" => Self::parse_csv(&content)?,
            _ => return Err("仅支持 .geojson / .json 或 .csv 文件".to_string()),
        };

        if parsed.is_empty() {
            return Err("文件中未找到有效的地理坐标或数值字段".to_string());
        }

        // 收集所有出现过的数值字段名
        let mut keys_set = std::collections::BTreeSet::new();
        for rec in &parsed {
            for k in rec.metrics.keys() {
                keys_set.insert(k.clone());
            }
        }
        let metric_keys: Vec<String> = keys_set.into_iter().collect();
        if metric_keys.is_empty() {
            return Err("未在属性表或 CSV 列中检测到任何数值字段".to_string());
        }

        self.dataset_name = path_ref
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("External Dataset")
            .to_string();
        self.records = parsed;
        self.metric_keys = metric_keys.clone();

        self.params.insert(
            "metric".to_string(),
            PluginParam::Select {
                label: "统计指标".to_string(),
                options: metric_keys,
                selected: 0,
            },
        );

        Ok(self.records.len())
    }

    fn parse_geojson(raw: &str) -> Result<Vec<StatRecord>, String> {
        let gj: geojson::GeoJson = raw.parse().map_err(|e| format!("GeoJSON 解析失败: {}", e))?;
        let mut out = Vec::new();

        let fc = match gj {
            geojson::GeoJson::FeatureCollection(fc) => fc,
            geojson::GeoJson::Feature(f) => geojson::FeatureCollection {
                bbox: None,
                features: vec![f],
                foreign_members: None,
            },
            _ => return Err("请提供包含 FeatureCollection 的 GeoJSON".to_string()),
        };

        for (idx, feat) in fc.features.iter().enumerate() {
            let Some(ref geom) = feat.geometry else { continue };
            let mut metrics = HashMap::new();
            let mut name = format!("区域 #{}", idx + 1);

            if let Some(ref props) = feat.properties {
                for (k, v) in props {
                    if let Some(num) = v.as_f64() {
                        metrics.insert(k.clone(), num);
                    } else if let Some(s) = v.as_str() {
                        let kl = k.to_lowercase();
                        if kl == "name" || kl == "title" || kl == "city" || kl == "province" {
                            name = s.to_string();
                        } else if let Ok(num) = s.trim().parse::<f64>() {
                            metrics.insert(k.clone(), num);
                        }
                    }
                }
            }

            if metrics.is_empty() {
                continue;
            }

            match &geom.value {
                geojson::Value::Point(coords) => {
                    if let (Some(&lon), Some(&lat)) = (coords.first(), coords.get(1)) {
                        out.push(StatRecord {
                            name,
                            geom: RawStatGeom::Point(lon, lat),
                            metrics,
                        });
                    }
                }
                geojson::Value::Polygon(rings) => {
                    if let Some(outer) = rings.first() {
                        let pts: Vec<(f64, f64)> = outer
                            .iter()
                            .filter_map(|p| Some((*p.first()?, *p.get(1)?)))
                            .collect();
                        if pts.len() >= 3 {
                            out.push(StatRecord {
                                name,
                                geom: RawStatGeom::Polygon(pts),
                                metrics,
                            });
                        }
                    }
                }
                geojson::Value::MultiPolygon(polys) => {
                    for rings in polys {
                        if let Some(outer) = rings.first() {
                            let pts: Vec<(f64, f64)> = outer
                                .iter()
                                .filter_map(|p| Some((*p.first()?, *p.get(1)?)))
                                .collect();
                            if pts.len() >= 3 {
                                out.push(StatRecord {
                                    name: name.clone(),
                                    geom: RawStatGeom::Polygon(pts),
                                    metrics: metrics.clone(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// 解析带表头的 CSV 统计文件 (自动识别 lon/lng/longitude 和 lat/latitude 列)
    fn parse_csv(raw: &str) -> Result<Vec<StatRecord>, String> {
        let mut lines = raw.lines().filter(|l| !l.trim().is_empty());
        let header_line = lines.next().ok_or("CSV 文件为空")?;
        let headers: Vec<String> = header_line
            .split(',')
            .map(|h| h.trim().trim_matches('"').to_string())
            .collect();

        let lon_idx = headers
            .iter()
            .position(|h| matches!(h.to_lowercase().as_str(), "lon" | "lng" | "longitude" | "经度"))
            .ok_or("CSV 表头缺少 lon / longitude / 经度 列")?;

        let lat_idx = headers
            .iter()
            .position(|h| matches!(h.to_lowercase().as_str(), "lat" | "latitude" | "纬度"))
            .ok_or("CSV 表头缺少 lat / latitude / 纬度 列")?;

        let name_idx = headers
            .iter()
            .position(|h| matches!(h.to_lowercase().as_str(), "name" | "city" | "title" | "名称" | "城市"));

        let mut out = Vec::new();
        for (row_i, line) in lines.enumerate() {
            let cols: Vec<&str> = line.split(',').map(|c| c.trim().trim_matches('"')).collect();
            let Some(lon) = cols.get(lon_idx).and_then(|s| s.parse::<f64>().ok()) else { continue };
            let Some(lat) = cols.get(lat_idx).and_then(|s| s.parse::<f64>().ok()) else { continue };

            let name = name_idx
                .and_then(|i| cols.get(i))
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| format!("Point #{}", row_i + 1));

            let mut metrics = HashMap::new();
            for (c_idx, h_name) in headers.iter().enumerate() {
                if c_idx == lon_idx || c_idx == lat_idx || Some(c_idx) == name_idx {
                    continue;
                }
                if let Some(val) = cols.get(c_idx).and_then(|s| s.parse::<f64>().ok()) {
                    metrics.insert(h_name.clone(), val);
                }
            }

            if !metrics.is_empty() {
                out.push(StatRecord {
                    name,
                    geom: RawStatGeom::Point(lon, lat),
                    metrics,
                });
            }
        }
        Ok(out)
    }

    fn palette_colors(palette_idx: usize) -> [RgbaColor; 4] {
        match palette_idx {
            1 => [
                // 海洋蓝渐变
                RgbaColor::new(185, 225, 245, 185),
                RgbaColor::new(115, 185, 235, 190),
                RgbaColor::new(45, 130, 210, 195),
                RgbaColor::new(15, 70, 165, 205),
            ],
            2 => [
                // 翠绿黄渐变
                RgbaColor::new(235, 240, 150, 185),
                RgbaColor::new(155, 215, 110, 190),
                RgbaColor::new(75, 175, 85, 195),
                RgbaColor::new(20, 120, 55, 205),
            ],
            _ => [
                // 默认暖红橙渐变
                RgbaColor::new(250, 215, 115, 185),
                RgbaColor::new(245, 150, 65, 190),
                RgbaColor::new(225, 75, 55, 195),
                RgbaColor::new(165, 20, 35, 205),
            ],
        }
    }
}

impl MapDataPlugin for ExternalDataPlugin {
    fn id(&self) -> &'static str {
        "builtin.external_data"
    }

    fn name(&self) -> &'static str {
        "外部统计数据图层 (GeoJSON / CSV)"
    }

    fn description(&self) -> &'static str {
        "导入外部 GeoJSON 行政区划或 CSV 表格，自动分级设色并生成图例"
    }

    fn params_mut(&mut self) -> &mut HashMap<String, PluginParam> {
        &mut self.params
    }

    fn query_data(&self, bounds: MapBounds) -> PluginRenderPacket {
        if self.records.is_empty() || self.metric_keys.is_empty() {
            return PluginRenderPacket::default();
        }

        let metric_idx = match self.params.get("metric") {
            Some(PluginParam::Select { selected, .. }) => *selected,
            _ => 0,
        };
        let palette_idx = match self.params.get("palette") {
            Some(PluginParam::Select { selected, .. }) => *selected,
            _ => 0,
        };

        let metric_key = self
            .metric_keys
            .get(metric_idx)
            .cloned()
            .unwrap_or_else(|| self.metric_keys[0].clone());

        // 计算全局最大最小值用于 4 等分分级设色
        let mut min_v = f64::MAX;
        let mut max_v = f64::MIN;
        for rec in &self.records {
            if let Some(&v) = rec.metrics.get(&metric_key) {
                min_v = min_v.min(v);
                max_v = max_v.max(v);
            }
        }
        if min_v > max_v {
            return PluginRenderPacket::default();
        }
        let span = (max_v - min_v).max(1e-6);
        let step = span / 4.0;
        let colors = Self::palette_colors(palette_idx);

        let mut items = Vec::new();
        for rec in &self.records {
            let Some(&val) = rec.metrics.get(&metric_key) else { continue };
            let bin = (((val - min_v) / step).floor() as usize).min(3);
            let fill_color = colors[bin];
            let tooltip = format!("{}\n{}: {:.2}", rec.name, metric_key, val);

            match &rec.geom {
                RawStatGeom::Point(lon, lat) => {
                    if *lon < bounds.min_lon - 2.0
                        || *lon > bounds.max_lon + 2.0
                        || *lat < bounds.min_lat - 2.0
                        || *lat > bounds.max_lat + 2.0
                    {
                        continue;
                    }
                    let ratio = ((val - min_v) / span) as f32;
                    let radius_px = 9.0 + ratio * 18.0;
                    items.push(PluginVisualItem::StatBubble {
                        lon: *lon,
                        lat: *lat,
                        radius_px,
                        fill_color,
                        label: format!("{} {:.1}", rec.name, val),
                        tooltip,
                    });
                }
                RawStatGeom::Polygon(pts) => {
                    // 简单包围盒裁剪
                    let (mut r_min_x, mut r_max_x) = (180.0_f64, -180.0_f64);
                    let (mut r_min_y, mut r_max_y) = (90.0_f64, -90.0_f64);
                    for &(x, y) in pts {
                        r_min_x = r_min_x.min(x);
                        r_max_x = r_max_x.max(x);
                        r_min_y = r_min_y.min(y);
                        r_max_y = r_max_y.max(y);
                    }
                    if r_max_x < bounds.min_lon
                        || r_min_x > bounds.max_lon
                        || r_max_y < bounds.min_lat
                        || r_min_y > bounds.max_lat
                    {
                        continue;
                    }
                    items.push(PluginVisualItem::ChoroplethPolygon {
                        rings: pts.clone(),
                        fill_color,
                        stroke_color: RgbaColor::new(255, 255, 255, 210),
                        tooltip: Some(tooltip),
                    });
                }
            }
        }

        let legend = vec![
            LegendItem { color: colors[3], label: format!("{:.1} ~ {:.1}", min_v + step * 3.0, max_v) },
            LegendItem { color: colors[2], label: format!("{:.1} ~ {:.1}", min_v + step * 2.0, min_v + step * 3.0) },
            LegendItem { color: colors[1], label: format!("{:.1} ~ {:.1}", min_v + step, min_v + step * 2.0) },
            LegendItem { color: colors[0], label: format!("{:.1} ~ {:.1}", min_v, min_v + step) },
        ];

        PluginRenderPacket {
            items,
            legend_title: format!("{} [{}]", metric_key, self.dataset_name),
            legend,
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

}

