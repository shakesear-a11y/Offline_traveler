pub mod birth_rate;
pub mod external_data;

use crate::osm_index::MapBounds;
use crate::style_config::RgbaColor;
use std::any::Any;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum PluginParam {
    IntSlider {
        label: String,
        value: i32,
        min: i32,
        max: i32,
    },
    Select {
        label: String,
        options: Vec<String>,
        selected: usize,
    },
}

#[derive(Clone, Debug)]
pub struct LegendItem {
    pub color: RgbaColor,
    pub label: String,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum PluginVisualItem {
    ChoroplethPolygon {
        rings: Vec<(f64, f64)>,
        fill_color: RgbaColor,
        stroke_color: RgbaColor,
        tooltip: Option<String>,
    },
    StatBubble {
        lon: f64,
        lat: f64,
        radius_px: f32,
        fill_color: RgbaColor,
        label: String,
        tooltip: String,
    },
}

#[derive(Default, Clone, Debug)]
pub struct PluginRenderPacket {
    pub items: Vec<PluginVisualItem>,
    pub legend_title: String,
    pub legend: Vec<LegendItem>,
}

pub trait MapDataPlugin: Send + Sync {
    #[allow(dead_code)]
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn params_mut(&mut self) -> &mut HashMap<String, PluginParam>;
    fn query_data(&self, bounds: MapBounds) -> PluginRenderPacket;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

pub struct RegisteredPlugin {
    pub enabled: bool,
    pub instance: Box<dyn MapDataPlugin>,
}

pub struct PluginManager {
    pub plugins: Vec<RegisteredPlugin>,
}

impl PluginManager {
    pub fn new_with_builtins() -> Self {
        let mut mgr = Self {
            plugins: Vec::new(),
        };
        mgr.register(Box::new(birth_rate::BirthRatePlugin::new()), false);
        mgr.register(Box::new(external_data::ExternalDataPlugin::empty()), false);
        mgr
    }

    pub fn register(&mut self, plugin: Box<dyn MapDataPlugin>, default_enabled: bool) {
        self.plugins.push(RegisteredPlugin {
            enabled: default_enabled,
            instance: plugin,
        });
    }

    pub fn collect_active_packets(&self, bounds: MapBounds) -> Vec<PluginRenderPacket> {
        let mut packets = Vec::new();
        for entry in &self.plugins {
            if entry.enabled {
                packets.push(entry.instance.query_data(bounds));
            }
        }
        packets
    }
}

