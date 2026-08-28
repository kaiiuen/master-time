//! Reusable egui renderer for offset and round-trip measurement histories.
//!
//! The renderer consumes the UI-independent [`ChartModel`] and owns only the
//! presentation details: plot geometry, axes, labels, and series styling.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Vec2};

use crate::history_view::{ChartModel, NormalizedPoint, ValueRange};

const DEFAULT_HEIGHT: f32 = 240.0;
const MIN_WIDTH: f32 = 280.0;
const LEFT_MARGIN: f32 = 54.0;
const RIGHT_MARGIN: f32 = 62.0;
const TOP_MARGIN: f32 = 30.0;
const BOTTOM_MARGIN: f32 = 28.0;

const OFFSET_COLOR: Color32 = Color32::from_rgb(55, 140, 235);
const ROUND_TRIP_COLOR: Color32 = Color32::from_rgb(235, 145, 55);
const AXIS_COLOR: Color32 = Color32::from_gray(150);
const GRID_COLOR: Color32 = Color32::from_gray(70);

/// A normalized chart area in screen coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlotGeometry {
    pub rect: Rect,
}

impl PlotGeometry {
    /// Converts a normalized point to a bounded screen position.
    pub fn point(&self, point: NormalizedPoint) -> Pos2 {
        normalized_to_screen(self.rect, point)
    }

    /// Returns the screen y coordinate corresponding to zero, if represented
    /// by the supplied range.
    pub fn zero_y(&self, range: ValueRange) -> Option<f32> {
        zero_line_y(self.rect, range)
    }
}

/// Converts normalized coordinates into a point guaranteed to be in `plot`.
pub fn normalized_to_screen(plot: Rect, point: NormalizedPoint) -> Pos2 {
    let x = point.x.clamp(0.0, 1.0) as f32;
    let y = point.y.clamp(0.0, 1.0) as f32;
    Pos2::new(
        plot.left() + plot.width() * x,
        plot.bottom() - plot.height() * y,
    )
}

/// Converts a raw value to the model's normalized y coordinate.
pub fn normalize_value(value: f64, range: ValueRange) -> Option<f32> {
    if !value.is_finite() || !range.min.is_finite() || !range.max.is_finite() {
        return None;
    }
    let span = range.max - range.min;
    if span == 0.0 {
        Some(0.5)
    } else if span.is_finite() {
        Some(((value - range.min) / span).clamp(0.0, 1.0) as f32)
    } else {
        None
    }
}

/// Returns the screen y coordinate for zero, or `None` when zero is outside
/// the range. Constant zero-valued ranges place the line in the middle.
pub fn zero_line_y(plot: Rect, range: ValueRange) -> Option<f32> {
    if !range.min.is_finite() || !range.max.is_finite() || range.min > range.max {
        return None;
    }
    if range.min > 0.0 || range.max < 0.0 {
        return None;
    }
    let normalized = normalize_value(0.0, range)?;
    Some(plot.bottom() - plot.height() * normalized)
}

/// Computes the drawable plot rectangle inside an allocated chart rectangle.
pub fn plot_geometry(rect: Rect) -> PlotGeometry {
    let left = (rect.left() + LEFT_MARGIN).min(rect.right());
    let right = (rect.right() - RIGHT_MARGIN).max(left);
    let top = (rect.top() + TOP_MARGIN).min(rect.bottom());
    let bottom = (rect.bottom() - BOTTOM_MARGIN).max(top);
    PlotGeometry {
        rect: Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom)),
    }
}

/// A configurable renderer for [`ChartModel`].
#[derive(Debug, Clone, Copy)]
pub struct ChartRenderer {
    height: f32,
}

impl Default for ChartRenderer {
    fn default() -> Self {
        Self::new(DEFAULT_HEIGHT)
    }
}

impl ChartRenderer {
    pub fn new(height: f32) -> Self {
        Self {
            height: height.max(120.0),
        }
    }

    pub fn height(self) -> f32 {
        self.height
    }

    /// Paints both histories and returns the allocated egui response.
    pub fn show(self, ui: &mut egui::Ui, model: &ChartModel) -> egui::Response {
        let width = ui.available_width().max(MIN_WIDTH);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(width, self.height), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);

        if model.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No measurements yet",
                FontId::proportional(14.0),
                ui.visuals().weak_text_color(),
            );
            return response;
        }

        let geometry = plot_geometry(rect);
        draw_axes(&painter, geometry.rect);
        draw_grid(&painter, geometry.rect);
        draw_zero_line(&painter, geometry, model.offset_range(), OFFSET_COLOR);
        draw_zero_line(
            &painter,
            geometry,
            model.round_trip_range(),
            ROUND_TRIP_COLOR,
        );
        draw_series(&painter, geometry, model.offset_points(), OFFSET_COLOR);
        draw_series(
            &painter,
            geometry,
            model.round_trip_points(),
            ROUND_TRIP_COLOR,
        );
        draw_labels(&painter, rect, geometry.rect, model);
        response
    }
}

/// Convenience entry point using the default chart height.
pub fn show(ui: &mut egui::Ui, model: &ChartModel) -> egui::Response {
    ChartRenderer::default().show(ui, model)
}

fn draw_axes(painter: &egui::Painter, plot: Rect) {
    let stroke = Stroke::new(1.0_f32, AXIS_COLOR);
    painter.line_segment([plot.left_top(), plot.left_bottom()], stroke);
    painter.line_segment([plot.left_bottom(), plot.right_bottom()], stroke);
    painter.line_segment([plot.right_top(), plot.right_bottom()], stroke);
}

fn draw_grid(painter: &egui::Painter, plot: Rect) {
    let stroke = Stroke::new(1.0_f32, GRID_COLOR.linear_multiply(0.45));
    for fraction in [0.25, 0.5, 0.75] {
        let y = plot.top() + plot.height() * fraction;
        painter.line_segment(
            [Pos2::new(plot.left(), y), Pos2::new(plot.right(), y)],
            stroke,
        );
    }
}

fn draw_zero_line(
    painter: &egui::Painter,
    geometry: PlotGeometry,
    range: Option<ValueRange>,
    color: Color32,
) {
    let Some(range) = range else { return };
    let Some(y) = geometry.zero_y(range) else {
        return;
    };
    painter.line_segment(
        [
            Pos2::new(geometry.rect.left(), y),
            Pos2::new(geometry.rect.right(), y),
        ],
        Stroke::new(1.5_f32, color.linear_multiply(0.75)),
    );
}

fn draw_series(
    painter: &egui::Painter,
    geometry: PlotGeometry,
    points: &[NormalizedPoint],
    color: Color32,
) {
    let positions: Vec<Pos2> = points.iter().map(|point| geometry.point(*point)).collect();
    if positions.len() > 1 {
        painter.add(Shape::line(positions.clone(), Stroke::new(2.0_f32, color)));
    }
    for position in positions {
        painter.circle_filled(position, 3.0, color);
    }
}

fn draw_labels(painter: &egui::Painter, rect: Rect, plot: Rect, model: &ChartModel) {
    let font = FontId::proportional(11.0);
    let text = Color32::from_gray(190);
    painter.text(
        rect.center_top() + Vec2::new(0.0, 8.0),
        Align2::CENTER_TOP,
        "Offset / round trip",
        font.clone(),
        text,
    );
    painter.text(
        Pos2::new(plot.left() - 8.0, plot.top()),
        Align2::RIGHT_CENTER,
        range_label(model.offset_range(), "offset"),
        font.clone(),
        OFFSET_COLOR,
    );
    painter.text(
        Pos2::new(plot.left() - 8.0, plot.bottom()),
        Align2::RIGHT_CENTER,
        range_label_min(model.offset_range()),
        font.clone(),
        OFFSET_COLOR,
    );
    painter.text(
        Pos2::new(plot.right() + 8.0, plot.top()),
        Align2::LEFT_CENTER,
        range_label(model.round_trip_range(), "round trip"),
        font.clone(),
        ROUND_TRIP_COLOR,
    );
    painter.text(
        Pos2::new(plot.right() + 8.0, plot.bottom()),
        Align2::LEFT_CENTER,
        range_label_min(model.round_trip_range()),
        font,
        ROUND_TRIP_COLOR,
    );
    painter.text(
        plot.left_bottom() + Vec2::new(0.0, 8.0),
        Align2::LEFT_TOP,
        "oldest",
        FontId::proportional(10.0),
        text,
    );
    painter.text(
        plot.right_bottom() + Vec2::new(0.0, 8.0),
        Align2::RIGHT_TOP,
        "newest",
        FontId::proportional(10.0),
        text,
    );
}

fn range_label(range: Option<ValueRange>, name: &str) -> String {
    range.map_or_else(
        || name.to_owned(),
        |range| format!("{name} {:.3}", range.max),
    )
}

fn range_label_min(range: Option<ValueRange>) -> String {
    range.map_or_else(String::new, |range| format!("{:.3}", range.min))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Rect {
        Rect::from_min_max(Pos2::new(10.0, 20.0), Pos2::new(210.0, 120.0))
    }

    #[test]
    fn normalized_points_map_to_plot_bounds() {
        let rect = plot();
        assert_eq!(
            normalized_to_screen(rect, NormalizedPoint { x: 0.0, y: 0.0 }),
            rect.left_bottom()
        );
        assert_eq!(
            normalized_to_screen(rect, NormalizedPoint { x: 1.0, y: 1.0 }),
            rect.right_top()
        );
        assert_eq!(
            normalized_to_screen(rect, NormalizedPoint { x: 4.0, y: -2.0 }),
            rect.right_bottom()
        );
    }

    #[test]
    fn zero_line_is_inverted_for_screen_coordinates() {
        let rect = plot();
        assert_eq!(
            zero_line_y(
                rect,
                ValueRange {
                    min: -10.0,
                    max: 10.0
                }
            ),
            Some(70.0)
        );
        assert_eq!(zero_line_y(rect, ValueRange { min: 1.0, max: 3.0 }), None);
    }

    #[test]
    fn constant_ranges_and_invalid_values_are_safe() {
        let rect = plot();
        let range = ValueRange { min: 0.0, max: 0.0 };
        assert_eq!(normalize_value(0.0, range), Some(0.5));
        assert_eq!(zero_line_y(rect, range), Some(70.0));
        assert_eq!(normalize_value(f64::NAN, range), None);
        assert_eq!(zero_line_y(rect, ValueRange { min: 2.0, max: 1.0 }), None);
    }
}
