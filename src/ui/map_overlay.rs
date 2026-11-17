use crate::plugin::PluginVisualItem;
use super::{mercator_to_wgs84, wgs84_to_mercator, OfflineMapApp};
use crate::map_renderer::MapGeomFeature;
use galileo::symbol::Symbol;
use std::collections::HashSet;

impl<S1, S2, Space> OfflineMapApp<S1, S2, Space>
where
    S1: Symbol<MapGeomFeature> + Send + Sync + 'static,
    S2: Symbol<MapGeomFeature> + Send + Sync + 'static,
    Space: Send + Sync + 'static,
{
    pub fn draw_overlays(&mut self, ui: &mut egui::Ui, map_rect: egui::Rect) {
        let view = self.map_state.map().view();
        let Some(bbox) = view.get_bbox() else { return };
        let bbox_w = bbox.x_max() - bbox.x_min();
        let bbox_h = bbox.y_max() - bbox.y_min();
        if bbox_w <= 0.0 || bbox_h <= 0.0 {
            return;
        }

        let right_click_pos = ui.input(|i| {
            if i.pointer.secondary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        let mut new_stop_to_insert: Option<(String, f64, f64)> = None;

        // 捕获右键点击：添加到当日行程，同时记录坐标供左侧“自定义 POI”保存
        if let Some(pos) = right_click_pos {
            if map_rect.contains(pos) {
                let norm_x = ((pos.x - map_rect.left()) / map_rect.width()) as f64;
                let norm_y = 1.0 - ((pos.y - map_rect.top()) / map_rect.height()) as f64;
                let mx = bbox.x_min() + norm_x * bbox_w;
                let my = bbox.y_min() + norm_y * bbox_h;
                let (lon, lat) = mercator_to_wgs84(mx, my);

                self.pending_poi_coord = Some((lon, lat));
                let day_count = self
                    .stops
                    .iter()
                    .filter(|s| s.day_index == self.selected_day)
                    .count();
                new_stop_to_insert = Some((
                    format!("Day{} Stop #{}", self.selected_day, day_count + 1),
                    lon,
                    lat,
                ));
            }
        }

        let left_click_pos = ui.input(|i| {
            if i.pointer.primary_clicked() {
                i.pointer.interact_pos()
            } else {
                None
            }
        });

        let painter = ui.painter_at(map_rect);
        let mut occupied_cells: HashSet<(i32, i32)> = HashSet::new();
        let mut drawn_roads: HashSet<String> = HashSet::new();

        // 1. 绘制 OSM 地名、景点与去重道路名称
        if let Ok(labels) = self.visible_labels.lock() {
            for lbl in labels.iter() {
                if lbl.category == "label_road" && !drawn_roads.insert(lbl.text.clone()) {
                    continue;
                }
                let (mx, my) = wgs84_to_mercator(lbl.lon, lbl.lat);
                let norm_x = (mx - bbox.x_min()) / bbox_w;
                let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
                if !(0.0..=1.0).contains(&norm_x) || !(0.0..=1.0).contains(&norm_y) {
                    continue;
                }
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let cell = ((sx / 115.0) as i32, (sy / 32.0) as i32);
                if !occupied_cells.insert(cell) {
                    continue;
                }

                let (font_size, text_color, dot_color) = match lbl.category.as_str() {
                    "label_city" => (16.0, egui::Color32::from_rgb(30, 30, 35), egui::Color32::from_rgb(220, 60, 60)),
                    "label_town" => (13.5, egui::Color32::from_rgb(50, 50, 60), egui::Color32::from_rgb(230, 120, 40)),
                    "label_poi" => (12.5, egui::Color32::from_rgb(25, 95, 160), egui::Color32::from_rgb(30, 130, 220)),
                    _ => (11.5, egui::Color32::from_rgb(85, 85, 90), egui::Color32::TRANSPARENT),
                };

                if dot_color != egui::Color32::TRANSPARENT {
                    let center_pt = egui::pos2(sx, sy);
                    painter.circle_filled(center_pt, 4.0, dot_color);
                    painter.circle_stroke(center_pt, 4.0, egui::Stroke::new(1.0_f32, egui::Color32::WHITE));
                    if let Some(cpos) = left_click_pos {
                        if cpos.distance(center_pt) <= 12.0 && new_stop_to_insert.is_none() {
                            new_stop_to_insert = Some((lbl.text.clone(), lbl.lon, lbl.lat));
                        }
                    }
                }

                let text_pos = egui::pos2(sx + 6.0, sy - font_size * 0.5);
                let font = egui::FontId::proportional(font_size);
                for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    painter.text(
                        egui::pos2(text_pos.x + dx, text_pos.y + dy),
                        egui::Align2::LEFT_TOP,
                        &lbl.text,
                        font.clone(),
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 225),
                    );
                }
                painter.text(text_pos, egui::Align2::LEFT_TOP, &lbl.text, font, text_color);
            }
        }

        // 2. 绘制用户设置的自定义 POI（金色圆标）
        for poi in &self.custom_pois {
            let (mx, my) = wgs84_to_mercator(poi.lon, poi.lat);
            let norm_x = (mx - bbox.x_min()) / bbox_w;
            let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
            if (0.0..=1.0).contains(&norm_x) && (0.0..=1.0).contains(&norm_y) {
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let pt = egui::pos2(sx, sy);

                painter.circle_filled(pt, 6.5, egui::Color32::from_rgb(245, 180, 0));
                painter.circle_stroke(pt, 6.5, egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(120, 60, 0)));
                painter.text(
                    egui::pos2(sx + 9.0, sy - 7.0),
                    egui::Align2::LEFT_TOP,
                    format!("★ {}", poi.name),
                    egui::FontId::proportional(13.0),
                    egui::Color32::from_rgb(160, 80, 0),
                );

                if let Some(cpos) = left_click_pos {
                    if cpos.distance(pt) <= 12.0 && new_stop_to_insert.is_none() {
                        new_stop_to_insert = Some((poi.name.clone(), poi.lon, poi.lat));
                    }
                }
            }
        }

        if let Some((name, lon, lat)) = new_stop_to_insert {
            let order_idx = self
                .stops
                .iter()
                .filter(|s| s.day_index == self.selected_day)
                .count() as i32;
            if let Ok(new_stop) = self.note_mgr.create_stop(self.selected_day, order_idx, &name, lon, lat) {
                self.selected_stop_id = Some(new_stop.id);
                self.stops.push(new_stop);
                self.route_dirty = true;
            }
        }

        // 3. 绘制当日行程序号图钉
        let mut day_seq = 0;
        for stop in &self.stops {
            if stop.day_index != self.selected_day {
                continue;
            }
            day_seq += 1;
            let (mx, my) = wgs84_to_mercator(stop.lon, stop.lat);
            let norm_x = (mx - bbox.x_min()) / bbox_w;
            let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
            if (0.0..=1.0).contains(&norm_x) && (0.0..=1.0).contains(&norm_y) {
                let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                let pin_pos = egui::pos2(sx, sy);

                let fill = if self.selected_stop_id == Some(stop.id) {
                    egui::Color32::from_rgb(20, 160, 90)
                } else {
                    egui::Color32::from_rgb(230, 55, 55)
                };
                painter.circle_filled(pin_pos, 11.5, fill);
                painter.circle_stroke(pin_pos, 11.5, egui::Stroke::new(2.0_f32, egui::Color32::WHITE));
                painter.text(
                    pin_pos,
                    egui::Align2::CENTER_CENTER,
                    format!("{}", day_seq),
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        }
        // 4. 绘制插件统计气泡 (StatBubble) 与悬停详情 Tooltip
        let hover_pos = ui.input(|i| i.pointer.hover_pos());
        let mut active_tooltip: Option<String> = None;

        for packet in &self.active_plugin_packets {
            for item in &packet.items {
                if let PluginVisualItem::StatBubble {
                    lon,
                    lat,
                    radius_px,
                    fill_color,
                    label,
                    tooltip,
                } = item
                {
                    let (mx, my) = wgs84_to_mercator(*lon, *lat);
                    let norm_x = (mx - bbox.x_min()) / bbox_w;
                    let norm_y = 1.0 - (my - bbox.y_min()) / bbox_h;
                    if !(0.0..=1.0).contains(&norm_x) || !(0.0..=1.0).contains(&norm_y) {
                        continue;
                    }

                    let sx = map_rect.left() + (norm_x as f32) * map_rect.width();
                    let sy = map_rect.top() + (norm_y as f32) * map_rect.height();
                    let center = egui::pos2(sx, sy);

                    let egui_fill = egui::Color32::from_rgba_unmultiplied(
                        fill_color.r,
                        fill_color.g,
                        fill_color.b,
                        fill_color.a,
                    );
                    painter.circle_filled(center, *radius_px, egui_fill);
                    painter.circle_stroke(
                        center,
                        *radius_px,
                        egui::Stroke::new(1.5_f32, egui::Color32::WHITE),
                    );

                    // 气泡中心数值标签
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        label,
                        egui::FontId::proportional(11.0),
                        egui::Color32::WHITE,
                    );

                    if let Some(hpos) = hover_pos {
                        if hpos.distance(center) <= *radius_px {
                            active_tooltip = Some(tooltip.clone());
                        }
                    }
                }
            }
        }

        if let Some(tip_text) = active_tooltip {
            if let Some(hpos) = hover_pos {
                let tip_rect = egui::Rect::from_min_size(
                    egui::pos2(hpos.x + 14.0, hpos.y + 14.0),
                    egui::vec2(155.0, 46.0),
                );
                painter.rect_filled(
                    tip_rect,
                    6.0,
                    egui::Color32::from_rgba_unmultiplied(28, 32, 38, 235),
                );
                painter.text(
                    tip_rect.left_top() + egui::vec2(8.0, 6.0),
                    egui::Align2::LEFT_TOP,
                    tip_text,
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
        }

        // 5. 在地图右下角绘制当前启用插件的专题图例卡片 (Legend Card)
        let mut cursor_bottom = map_rect.bottom() - 16.0;
        for packet in self.active_plugin_packets.iter().rev() {
            if packet.legend.is_empty() {
                continue;
            }
            let card_w = 165.0;
            let card_h = 28.0 + (packet.legend.len() as f32) * 20.0;
            let card_rect = egui::Rect::from_min_size(
                egui::pos2(map_rect.right() - card_w - 16.0, cursor_bottom - card_h),
                egui::vec2(card_w, card_h),
            );

            painter.rect_filled(
                card_rect,
                8.0,
                egui::Color32::from_rgba_unmultiplied(255, 255, 255, 238),
            );
            painter.rect_stroke(
                card_rect,
                8.0,
                egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(200, 205, 212)),
                egui::StrokeKind::Inside,
            );

            painter.text(
                card_rect.left_top() + egui::vec2(10.0, 7.0),
                egui::Align2::LEFT_TOP,
                &packet.legend_title,
                egui::FontId::proportional(12.5),
                egui::Color32::from_rgb(30, 35, 42),
            );

            for (idx, item) in packet.legend.iter().enumerate() {
                let row_y = card_rect.top() + 28.0 + (idx as f32) * 20.0;
                let swatch_rect = egui::Rect::from_min_size(
                    egui::pos2(card_rect.left() + 10.0, row_y + 2.0),
                    egui::vec2(14.0, 12.0),
                );
                let swatch_color = egui::Color32::from_rgba_unmultiplied(
                    item.color.r,
                    item.color.g,
                    item.color.b,
                    item.color.a,
                );
                painter.rect_filled(swatch_rect, 2.0, swatch_color);
                painter.text(
                    egui::pos2(swatch_rect.right() + 8.0, row_y),
                    egui::Align2::LEFT_TOP,
                    &item.label,
                    egui::FontId::proportional(11.5),
                    egui::Color32::from_rgb(55, 60, 68),
                );
            }

            cursor_bottom = card_rect.top() - 10.0;
        }

    }
}

