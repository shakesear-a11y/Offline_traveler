use super::{LegendItem, MapDataPlugin, PluginParam, PluginRenderPacket, PluginVisualItem};
use crate::osm_index::MapBounds;
use crate::style_config::RgbaColor;
use std::collections::HashMap;

struct CityBirthSample {
    name: &'static str,
    lon: f64,
    lat: f64,
    base_rate_2020: f64, // 2020 年出生率 (‰)
    annual_delta: f64,   // 年均变化率
}

const SAMPLE_CITIES: &[CityBirthSample] = &[
    CityBirthSample { name: "长沙", lon: 112.9388, lat: 28.2282, base_rate_2020: 9.8, annual_delta: -0.35 },
    CityBirthSample { name: "衡阳", lon: 112.5719, lat: 26.8932, base_rate_2020: 10.4, annual_delta: -0.40 },
    CityBirthSample { name: "株洲", lon: 113.1340, lat: 27.8274, base_rate_2020: 9.2, annual_delta: -0.30 },
    CityBirthSample { name: "湘潭", lon: 112.9440, lat: 27.8297, base_rate_2020: 8.6, annual_delta: -0.32 },
    CityBirthSample { name: "岳阳", lon: 113.1289, lat: 29.3571, base_rate_2020: 9.5, annual_delta: -0.38 },
    CityBirthSample { name: "常德", lon: 111.6985, lat: 29.0316, base_rate_2020: 8.1, annual_delta: -0.35 },
    CityBirthSample { name: "怀化", lon: 109.9985, lat: 27.5549, base_rate_2020: 11.2, annual_delta: -0.42 },
    CityBirthSample { name: "辰溪", lon: 110.1894, lat: 28.0041, base_rate_2020: 10.9, annual_delta: -0.38 },
    CityBirthSample { name: "广州", lon: 113.2644, lat: 23.1291, base_rate_2020: 11.8, annual_delta: -0.25 },
    CityBirthSample { name: "深圳", lon: 114.0579, lat: 22.5431, base_rate_2020: 12.6, annual_delta: -0.20 },
    CityBirthSample { name: "阳江", lon: 111.9822, lat: 21.8579, base_rate_2020: 11.5, annual_delta: -0.30 },
    CityBirthSample { name: "武汉", lon: 114.3054, lat: 30.5931, base_rate_2020: 8.9, annual_delta: -0.30 },
    CityBirthSample { name: "上海", lon: 121.4737, lat: 31.2304, base_rate_2020: 5.3, annual_delta: -0.18 },
    CityBirthSample { name: "北京", lon: 116.4074, lat: 39.9042, base_rate_2020: 6.4, annual_delta: -0.22 },
    CityBirthSample { name: "成都", lon: 104.0665, lat: 30.5728, base_rate_2020: 8.7, annual_delta: -0.25 },
    CityBirthSample { name: "贵阳", lon: 106.6302, lat: 26.6477, base_rate_2020: 12.4, annual_delta: -0.35 },
];

pub struct BirthRatePlugin {
    params: HashMap<String, PluginParam>,
}

impl BirthRatePlugin {
    pub fn new() -> Self {
        let mut params = HashMap::new();
        params.insert(
            "year".to_string(),
            PluginParam::IntSlider {
                label: "统计年份".to_string(),
                value: 2024,
                min: 2020,
                max: 2026,
            },
        );
        params.insert(
            "mode".to_string(),
            PluginParam::Select {
                label: "展示形式".to_string(),
                options: vec!["比例气泡".to_string(), "热力网格区".to_string()],
                selected: 0,
            },
        );
        Self { params }
    }

    /// 根据出生率 (‰) 映射分级色带
    fn color_for_rate(rate: f64) -> RgbaColor {
        if rate >= 10.5 {
            RgbaColor::new(220, 45, 55, 195)   // 高出生率：深珊瑚红
        } else if rate >= 8.5 {
            RgbaColor::new(245, 130, 40, 190)  // 中高：亮橙色
        } else if rate >= 6.5 {
            RgbaColor::new(245, 200, 60, 185)  // 中低：明黄色
        } else {
            RgbaColor::new(65, 155, 220, 185)  // 低出生率：冷天蓝
        }
    }
}

impl MapDataPlugin for BirthRatePlugin {
    fn id(&self) -> &'static str {
        "builtin.birth_rate"
    }

    fn name(&self) -> &'static str {
        "人口出生率专题统计"
    }

    fn description(&self) -> &'static str {
        "展示主要城市及区域 2020-2026 年人口出生率 (‰) 分级分布"
    }

    fn params_mut(&mut self) -> &mut HashMap<String, PluginParam> {
        &mut self.params
    }

    fn query_data(&self, bounds: MapBounds) -> PluginRenderPacket {
        let year = match self.params.get("year") {
            Some(PluginParam::IntSlider { value, .. }) => *value,
            _ => 2024,
        };
        let display_mode = match self.params.get("mode") {
            Some(PluginParam::Select { selected, .. }) => *selected,
            _ => 0,
        };

        let delta_years = (year - 2020) as f64;
        let mut items = Vec::new();

        for city in SAMPLE_CITIES {
            // 仅生成落入当前视野范围（含适当缓冲区）的数据
            if city.lon < bounds.min_lon - 2.0
                || city.lon > bounds.max_lon + 2.0
                || city.lat < bounds.min_lat - 2.0
                || city.lat > bounds.max_lat + 2.0
            {
                continue;
            }

            let rate = (city.base_rate_2020 + city.annual_delta * delta_years).max(3.5);
            let color = Self::color_for_rate(rate);
            let tooltip = format!("{} ({}年)\n人口出生率: {:.2}‰", city.name, year, rate);

            if display_mode == 0 {
                // 模式 0：比例气泡图
                let radius_px = (8.0 + (rate as f32) * 1.4).clamp(10.0, 28.0);
                items.push(PluginVisualItem::StatBubble {
                    lon: city.lon,
                    lat: city.lat,
                    radius_px,
                    fill_color: color,
                    label: format!("{} {:.1}‰", city.name, rate),
                    tooltip,
                });
            } else {
                // 模式 1：分级设色网格多边形
                let half_span = 0.35;
                let rings = vec![
                    (city.lon - half_span, city.lat - half_span),
                    (city.lon + half_span, city.lat - half_span),
                    (city.lon + half_span, city.lat + half_span),
                    (city.lon - half_span, city.lat + half_span),
                ];
                items.push(PluginVisualItem::ChoroplethPolygon {
                    rings,
                    fill_color: color,
                    stroke_color: RgbaColor::new(255, 255, 255, 200),
                    tooltip: Some(tooltip),
                });
            }
        }

        PluginRenderPacket {
            items,
            legend_title: format!("人口出生率 ({}年)", year),
            legend: vec![
                LegendItem { color: RgbaColor::new(220, 45, 55, 220), label: "≥ 10.5‰ (高)".to_string() },
                LegendItem { color: RgbaColor::new(245, 130, 40, 220), label: "8.5‰ ~ 10.5‰".to_string() },
                LegendItem { color: RgbaColor::new(245, 200, 60, 220), label: "6.5‰ ~ 8.5‰".to_string() },
                LegendItem { color: RgbaColor::new(65, 155, 220, 220), label: "< 6.5‰ (低)".to_string() },
            ],
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }

}

