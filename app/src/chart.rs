//! The Continuous screen's measurement chart, with one plot per unit.
//!
//! `None` is deliberately carried from storage to the path builder. A
//! missing measurement starts a new sub-path instead of becoming a zero at
//! the floor of the chart.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alidade_store::RoundRow;
use design_tokens::scale::{RADIUS, SPACE, TYPE};
use iced::alignment;
use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, LineDash, Path, Stroke, Text};
use iced::{Color, Point as CanvasPoint, Rectangle, Renderer, Size};

use crate::theme::Palette;

const DASH_PATTERNS: [&[f32]; 6] = [
    &[],
    &[8.0, 5.0],
    &[1.5, 4.0],
    &[8.0, 4.0, 1.5, 4.0],
    &[16.0, 6.0],
    &[8.0, 4.0, 1.5, 4.0, 1.5, 4.0],
];

#[cfg(test)]
#[path = "../tests/support/canvas_pixels.rs"]
pub(crate) mod canvas_pixels;

#[derive(Clone, Copy)]
pub struct SeriesSpec {
    pub label: &'static str,
    pub panel: usize,
    pub stroke: Stroke<'static>,
    metric: fn(&Point) -> Option<f64>,
}

/// One source for plot strokes and the labelled line samples below each plot.
pub fn series_specs(palette: Palette) -> [SeriesSpec; 6] {
    let labels = [
        "Download",
        "Upload",
        "Idle",
        "Load",
        "Jitter",
        "Packet loss",
    ];
    let panels = [0, 0, 1, 1, 1, 2];
    let metrics: [fn(&Point) -> Option<f64>; 6] = [
        |p| p.download_mbit,
        |p| p.upload_mbit,
        |p| p.ping_idle_ms,
        |p| p.ping_load_ms,
        |p| p.jitter_ms,
        |p| p.loss_pct,
    ];
    std::array::from_fn(|index| SeriesSpec {
        label: labels[index],
        panel: panels[index],
        metric: metrics[index],
        stroke: Stroke {
            line_dash: LineDash {
                segments: DASH_PATTERNS[index],
                offset: 0,
            },
            ..Stroke::default()
                .with_width(2.0)
                .with_color(palette.theme.chart[index].into())
        },
    })
}

/// The labelled samples consumed by a panel's key renderer.
pub fn key_entries(palette: Palette, panel: usize) -> Vec<(&'static str, Stroke<'static>)> {
    series_specs(palette)
        .into_iter()
        .filter(|series| series.panel == panel)
        .map(|series| (series.label, series.stroke))
        .collect()
}

/// The time-range chips used by the Continuous screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimeRange {
    #[default]
    Hour,
    SixHours,
    Day,
    Week,
}

impl TimeRange {
    fn duration(self) -> Duration {
        match self {
            TimeRange::Hour => Duration::from_secs(60 * 60),
            TimeRange::SixHours => Duration::from_secs(6 * 60 * 60),
            TimeRange::Day => Duration::from_secs(24 * 60 * 60),
            TimeRange::Week => Duration::from_secs(7 * 24 * 60 * 60),
        }
    }
}

/// One round as chart-ready units. Throughput is converted from storage's
/// bits/s to Mbit/s; all optional values remain optional.
#[derive(Debug, Clone)]
pub struct Point {
    pub id: i64,
    pub at: SystemTime,
    pub download_mbit: Option<f64>,
    pub upload_mbit: Option<f64>,
    pub ping_idle_ms: Option<f64>,
    pub ping_load_ms: Option<f64>,
    pub jitter_ms: Option<f64>,
    pub loss_pct: Option<f64>,
}

impl Point {
    fn from_row(row: &RoundRow) -> Self {
        let ping_load_ms = match (row.ping_down_ms, row.ping_up_ms) {
            (Some(down), Some(up)) => Some((down + up) / 2.0),
            (Some(value), None) | (None, Some(value)) => Some(value),
            (None, None) => None,
        };

        Self {
            id: row.id,
            at: row.started_at,
            download_mbit: row.down_bps.map(|bps| bps / 1_000_000.0),
            upload_mbit: row.up_bps.map(|bps| bps / 1_000_000.0),
            ping_idle_ms: row.ping_idle_ms,
            ping_load_ms,
            jitter_ms: row.jitter_ms,
            loss_pct: row.loss_pct,
        }
    }
}

/// Keeps the selected range in chronological order. It deliberately does not
/// fill metric gaps: a ping-only row stays on the x-axis while throughput is
/// `None`, allowing the renderer to break that line honestly.
pub fn bucket(rows: &[RoundRow], range: TimeRange) -> Vec<Point> {
    let Some(latest) = rows.iter().map(|row| row.started_at).max() else {
        return Vec::new();
    };
    let cutoff = match latest.checked_sub(range.duration()) {
        Some(cutoff) => cutoff,
        None => UNIX_EPOCH,
    };

    let mut points: Vec<_> = rows
        .iter()
        .filter(|row| row.started_at >= cutoff)
        .map(Point::from_row)
        .collect();
    points.sort_by_key(|point| point.at);
    points
}

/// Returns a readable y range for finite values.
///
/// Throughput receives at least five percent headroom on each side. This
/// gives a near-flat 312 Mbit/s line more than a ten-percent total band,
/// preventing harmless jitter from looking like a severe swing. Latency and
/// loss pin their low edge to zero so their baseline remains truthful.
pub fn y_scale(values: &[f64], include_zero: bool) -> (f64, f64) {
    let finite: Vec<f64> = values
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .collect();
    if finite.is_empty() {
        return (0.0, 1.0);
    }

    let min = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let max = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if include_zero {
        let high = max.max(0.0);
        let headroom = (high * 0.05).max(1.0);
        return (0.0, high + headroom);
    }

    let value_floor = min.abs().max(max.abs()) * 0.05;
    let variation = (max - min) * 0.08;
    let padding = value_floor.max(variation).max(1.0);
    (min - padding, max + padding)
}

/// Canvas program for the line chart. [`Self::new`] starts in the app's
/// default light palette; screens that already hold the active palette can
/// select it with [`Self::with_palette`].
#[derive(Clone)]
pub struct LineChart {
    points: Vec<Point>,
    palette: Palette,
}

impl LineChart {
    pub fn new(rows: &[RoundRow], range: TimeRange) -> Self {
        Self::with_palette(rows, range, Palette::light())
    }

    pub fn with_palette(rows: &[RoundRow], range: TimeRange, palette: Palette) -> Self {
        Self {
            points: bucket(rows, range),
            palette,
        }
    }

    fn plots(&self, size: Size) -> [Plot; 3] {
        let domains = chart_domains(&self.points);
        std::array::from_fn(|index| {
            let bounds = panel_bounds(size)[index];
            let mut plot = plot_area(bounds.size(), domains[index], (0.0, 0.0));
            plot.x += bounds.x;
            plot
        })
    }
}

/// Throughput, latency and loss domains are computed from their own units only.
pub fn chart_domains(points: &[Point]) -> [(f64, f64); 3] {
    let speeds: Vec<_> = points
        .iter()
        .flat_map(|p| [p.download_mbit, p.upload_mbit])
        .flatten()
        .collect();
    let latency: Vec<_> = points
        .iter()
        .flat_map(|p| [p.ping_idle_ms, p.ping_load_ms, p.jitter_ms])
        .flatten()
        .collect();
    let loss: Vec<_> = points.iter().filter_map(|p| p.loss_pct).collect();
    [
        y_scale(&speeds, false),
        y_scale(&latency, true),
        y_scale(&loss, true),
    ]
}

/// Three disjoint plots keep the existing chart height and table space.
pub fn panel_bounds(size: Size) -> [Rectangle; 3] {
    let width = ((size.width - SPACE.s6 * 2.0) / 3.0).max(0.0);
    std::array::from_fn(|index| {
        Rectangle::new(
            CanvasPoint::new(index as f32 * (width + SPACE.s6), 0.0),
            Size::new(width, size.height),
        )
    })
}

#[derive(Debug, Default)]
pub struct ChartState {
    hovered: Option<usize>,
    panel: usize,
}

#[derive(Clone, Copy)]
struct Plot {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Plot {
    fn rectangle(self) -> Rectangle {
        Rectangle::new(
            CanvasPoint::new(self.x, self.y),
            Size::new(self.width, self.height),
        )
    }
}

impl<Message> canvas::Program<Message> for LineChart {
    type State = ChartState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if !matches!(
            event,
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. })
        ) {
            return None;
        }

        let hit = cursor.position_in(bounds).and_then(|position| {
            self.plots(bounds.size())
                .into_iter()
                .enumerate()
                .find_map(|(index, plot)| {
                    plot.rectangle()
                        .contains(position)
                        .then_some((index, plot, position.x))
                })
        });
        let hovered = hit.and_then(|(_, plot, x)| nearest_point(&self.points, plot, x));
        let panel = hit.map_or(state.panel, |(index, _, _)| index);
        if state.hovered == hovered && state.panel == panel {
            None
        } else {
            state.hovered = hovered;
            state.panel = panel;
            Some(Action::request_redraw())
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let plots = self.plots(frame.size());
        let domains = chart_domains(&self.points);
        for index in 0..3 {
            self.draw_panel(&mut frame, plots[index], domains[index], index);
        }
        if let Some(index) = state.hovered.filter(|index| *index < self.points.len()) {
            let point = &self.points[index];
            draw_hover(
                &mut frame,
                point,
                index,
                &self.points,
                plots[state.panel],
                (
                    domains[state.panel],
                    [point.download_mbit, point.ping_idle_ms, point.loss_pct][state.panel],
                ),
                self.palette,
            );
        }

        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.position_in(bounds).is_some_and(|position| {
            self.plots(bounds.size())
                .iter()
                .any(|plot| plot.rectangle().contains(position))
        }) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

impl LineChart {
    fn draw_panel(
        &self,
        frame: &mut Frame<Renderer>,
        plot: Plot,
        domain: (f64, f64),
        panel: usize,
    ) {
        frame.fill_rectangle(
            CanvasPoint::new(plot.x, plot.y),
            Size::new(plot.width, plot.height),
            self.palette.well(),
        );
        frame.stroke_rectangle(
            CanvasPoint::new(plot.x, plot.y),
            Size::new(plot.width, plot.height),
            Stroke::default()
                .with_width(1.0)
                .with_color(self.palette.border()),
        );
        draw_grid(frame, plot, self.palette.theme.chart_grid.into());
        draw_axes(
            frame,
            plot,
            domain,
            ["Throughput (Mbit/s)", "Latency (ms)", "Loss (%)"][panel],
            self.palette.muted(),
        );
        for series in series_specs(self.palette)
            .into_iter()
            .filter(|s| s.panel == panel)
        {
            draw_series(
                frame,
                &self.points,
                plot,
                domain,
                series.metric,
                series.stroke,
            );
        }
        let mut key_x = plot.x;
        let mut key_y = plot.y + plot.height + SPACE.s4;
        for (label, stroke) in key_entries(self.palette, panel) {
            let entry_width =
                SPACE.s6 * 2.0 + SPACE.s1 + label.len() as f32 * TYPE.xs * LABEL_EM_PER_CHAR;
            if key_x > plot.x && key_x + entry_width > plot.x + plot.width {
                key_x = plot.x;
                key_y += SPACE.s4 + SPACE.s1;
            }
            frame.stroke(
                &Path::line(
                    CanvasPoint::new(key_x, key_y),
                    CanvasPoint::new(key_x + SPACE.s6 * 2.0, key_y),
                ),
                stroke,
            );
            draw_text(
                frame,
                label,
                CanvasPoint::new(key_x + SPACE.s6 * 2.0 + SPACE.s1, key_y),
                self.palette.muted(),
                TYPE.xs,
                alignment::Horizontal::Left,
            );
            key_x += entry_width + SPACE.s3;
        }
    }
}

/// Text width is estimated at 0.62 em per digit. The numeric face is
/// tabular, so one estimate per character holds for every digit.
const LABEL_EM_PER_CHAR: f32 = 0.62;

fn axis_label(value: f64, scale: (f64, f64)) -> String {
    if scale.1 - scale.0 < 4.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.0}")
    }
}

/// The widest of the five tick labels of `scale`, in characters.
fn widest_label_chars(scale: (f64, f64)) -> usize {
    (0..=4)
        .map(|step| {
            axis_label(
                scale.0 + (scale.1 - scale.0) * (f64::from(step) / 4.0),
                scale,
            )
        })
        .map(|label| label.chars().count())
        .max()
        .unwrap_or(1)
}

/// Side margin that holds the widest label of `scale` beside the plot well.
fn label_margin(scale: (f64, f64)) -> f32 {
    let text = widest_label_chars(scale) as f32 * TYPE.xs * LABEL_EM_PER_CHAR;
    (text + SPACE.s2 + SPACE.s1).max(SPACE.s6)
}

fn plot_area(size: Size, left: (f64, f64), right: (f64, f64)) -> Plot {
    let x = label_margin(left);
    let y = SPACE.s4;
    let width = (size.width - x - label_margin(right)).max(0.0);
    let height = (size.height - (SPACE.s4 + SPACE.s6 * 2.0 + SPACE.s2)).max(0.0);
    Plot {
        x,
        y,
        width,
        height,
    }
}

fn draw_grid(frame: &mut Frame<Renderer>, plot: Plot, color: Color) {
    for step in 1..4 {
        let y = plot.y + (plot.height * step as f32 / 4.0);
        frame.stroke(
            &Path::line(
                CanvasPoint::new(plot.x, y),
                CanvasPoint::new(plot.x + plot.width, y),
            ),
            Stroke::default().with_width(1.0).with_color(color),
        );
    }
}

fn draw_axes(
    frame: &mut Frame<Renderer>,
    plot: Plot,
    domain: (f64, f64),
    unit: &str,
    color: Color,
) {
    for step in 0..=4 {
        let fraction = step as f64 / 4.0;
        let y = plot.y + plot.height * (1.0 - fraction) as f32;
        let left_value = domain.0 + ((domain.1 - domain.0) * fraction);
        draw_text(
            frame,
            axis_label(left_value, domain),
            CanvasPoint::new(plot.x - SPACE.s2, y),
            color,
            TYPE.xs,
            alignment::Horizontal::Right,
        );
    }
    draw_text(
        frame,
        unit,
        CanvasPoint::new(plot.x, plot.y - SPACE.s2),
        color,
        TYPE.xs,
        alignment::Horizontal::Left,
    );
}

fn draw_series(
    frame: &mut Frame<Renderer>,
    points: &[Point],
    plot: Plot,
    scale: (f64, f64),
    metric: fn(&Point) -> Option<f64>,
    stroke: Stroke<'static>,
) {
    let path = Path::new(|builder| {
        let mut connected = false;
        for (index, point) in points.iter().enumerate() {
            let Some(value) = metric(point) else {
                connected = false;
                continue;
            };
            let position = CanvasPoint::new(x_for(points, index, plot), y_for(value, scale, plot));
            if connected {
                builder.line_to(position);
            } else {
                builder.move_to(position);
                connected = true;
            }
        }
    });
    frame.stroke(&path, stroke);
}

fn draw_hover(
    frame: &mut Frame<Renderer>,
    point: &Point,
    index: usize,
    points: &[Point],
    plot: Plot,
    metric: ((f64, f64), Option<f64>),
    palette: Palette,
) {
    let x = x_for(points, index, plot);
    let crosshair = palette.theme.focus.into();
    frame.stroke(
        &Path::line(
            CanvasPoint::new(x, plot.y),
            CanvasPoint::new(x, plot.y + plot.height),
        ),
        Stroke::default().with_width(1.0).with_color(crosshair),
    );
    if let Some(value) = metric.1 {
        let y = y_for(value, metric.0, plot);
        frame.stroke(
            &Path::line(
                CanvasPoint::new(plot.x, y),
                CanvasPoint::new(plot.x + plot.width, y),
            ),
            Stroke::default().with_width(1.0).with_color(crosshair),
        );
    }

    let card_width = SPACE.s6 * 11.0;
    let card_height = SPACE.s6 * 7.0;
    let card_x = if x + card_width + SPACE.s3 <= plot.x + plot.width {
        x + SPACE.s3
    } else {
        (x - card_width - SPACE.s3).max(plot.x)
    };
    let card_y = (plot.y + SPACE.s3).min(plot.y + plot.height - card_height);
    let radius = iced::border::radius(RADIUS.md);
    let card = Path::rounded_rectangle(
        CanvasPoint::new(card_x, card_y),
        Size::new(card_width, card_height),
        radius,
    );
    frame.fill(&card, palette.raised());
    frame.stroke(
        &card,
        Stroke::default()
            .with_width(1.0)
            .with_color(palette.theme.border.into()),
    );

    let lines = [
        timestamp_label(point.at),
        format!("Down  {}", optional_value(point.download_mbit, "Mbit/s")),
        format!("Up    {}", optional_value(point.upload_mbit, "Mbit/s")),
        format!("Idle  {}", optional_value(point.ping_idle_ms, "ms")),
        format!("Load  {}", optional_value(point.ping_load_ms, "ms")),
        format!("Jitter {}", optional_value(point.jitter_ms, "ms")),
        format!("Loss  {}", optional_value(point.loss_pct, "%")),
    ];
    for (line, text) in lines.into_iter().enumerate() {
        let color = if line == 0 {
            palette.text()
        } else {
            palette.muted()
        };
        draw_text(
            frame,
            text,
            CanvasPoint::new(
                card_x + SPACE.s3,
                card_y + SPACE.s2 + (TYPE.xs * (line + 1) as f32),
            ),
            color,
            TYPE.xs,
            alignment::Horizontal::Left,
        );
    }
}

fn draw_text(
    frame: &mut Frame<Renderer>,
    content: impl Into<String>,
    position: CanvasPoint,
    color: Color,
    size: f32,
    align_x: alignment::Horizontal,
) {
    frame.fill_text(Text {
        content: content.into(),
        position,
        color,
        size: size.into(),
        font: crate::ui::font_numeric(),
        align_x: align_x.into(),
        align_y: alignment::Vertical::Center,
        ..Text::default()
    });
}

fn x_for(points: &[Point], index: usize, plot: Plot) -> f32 {
    if points.len() < 2 {
        return plot.x + (plot.width / 2.0);
    }
    let start = unix_seconds(points[0].at);
    let end = unix_seconds(points[points.len() - 1].at);
    if end <= start {
        return plot.x + (plot.width * index as f32 / (points.len() - 1) as f32);
    }
    let current = unix_seconds(points[index].at);
    let fraction = ((current - start) / (end - start)).clamp(0.0, 1.0) as f32;
    plot.x + (plot.width * fraction)
}

fn nearest_point(points: &[Point], plot: Plot, x: f32) -> Option<usize> {
    points
        .iter()
        .enumerate()
        .min_by(|(left, _), (right, _)| {
            let left_distance = (x_for(points, *left, plot) - x).abs();
            let right_distance = (x_for(points, *right, plot) - x).abs();
            left_distance.total_cmp(&right_distance)
        })
        .map(|(index, _)| index)
}

fn y_for(value: f64, scale: (f64, f64), plot: Plot) -> f32 {
    let span = (scale.1 - scale.0).max(f64::EPSILON);
    let fraction = ((value - scale.0) / span).clamp(0.0, 1.0) as f32;
    plot.y + (plot.height * (1.0 - fraction))
}

fn unix_seconds(time: SystemTime) -> f64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn optional_value(value: Option<f64>, unit: &str) -> String {
    value.map_or_else(|| "—".to_owned(), |number| format!("{number:>7.2} {unit}"))
}

fn timestamp_label(time: SystemTime) -> String {
    let seconds = time
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs());
    let days = seconds / (24 * 60 * 60);
    let clock = seconds % (24 * 60 * 60);
    let hour = clock / (60 * 60);
    let minute = (clock % (60 * 60)) / 60;
    let (year, month, day) = civil_from_days(days as i64);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02} UTC")
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

#[cfg(test)]
mod canvas_render_tests {
    use super::*;
    use canvas::Program;

    #[tokio::test]
    async fn continuous_draw_keeps_all_six_key_labels_and_sample_strokes() {
        // R3-F1/M1: an empty, wide chart still draws all six labelled key samples.
        // Oracle: the p3 series contract, independent of key_entries/series_specs.
        let chart = LineChart::new(&[], TimeRange::Hour);
        let size = Size::new(2400.0, 240.0);
        let mut actual_renderer = canvas_pixels::renderer().await;
        let geometry = <LineChart as Program<()>>::draw(
            &chart,
            &ChartState::default(),
            &actual_renderer,
            &iced::Theme::Light,
            Rectangle::with_size(size),
            mouse::Cursor::Unavailable,
        );
        let actual = canvas_pixels::render(&mut actual_renderer, geometry, Size::new(2400, 240));
        let mut expected_renderer = canvas_pixels::renderer().await;
        let mut frame = Frame::new(&expected_renderer, size);
        let panels: [&[(&str, &[f32], usize)]; 3] = [
            &[("Download", &[], 0), ("Upload", &[8.0, 5.0], 1)],
            &[
                ("Idle", &[1.5, 4.0], 2),
                ("Load", &[8.0, 4.0, 1.5, 4.0], 3),
                ("Jitter", &[16.0, 6.0], 4),
            ],
            &[("Packet loss", &[8.0, 4.0, 1.5, 4.0, 1.5, 4.0], 5)],
        ];
        for (plot, entries) in chart.plots(size).into_iter().zip(panels) {
            let mut x = plot.x;
            let y = plot.y + plot.height + SPACE.s4;
            for &(label, segments, color) in entries {
                frame.stroke(
                    &Path::line(CanvasPoint::new(x, y), CanvasPoint::new(x + 48.0, y)),
                    Stroke {
                        line_dash: LineDash {
                            segments,
                            offset: 0,
                        },
                        ..Stroke::default()
                            .with_width(2.0)
                            .with_color(Palette::light().theme.chart[color].into())
                    },
                );
                frame.fill_text(Text {
                    content: label.into(),
                    position: CanvasPoint::new(x + 52.0, y),
                    color: Palette::light().muted(),
                    size: TYPE.xs.into(),
                    font: crate::ui::font_numeric(),
                    align_y: alignment::Vertical::Center,
                    ..Text::default()
                });
                x += 52.0 + label.len() as f32 * TYPE.xs * 0.62 + SPACE.s3;
            }
        }
        let expected = canvas_pixels::render(
            &mut expected_renderer,
            vec![frame.into_geometry()],
            Size::new(2400, 240),
        );
        canvas_pixels::assert_region_eq(
            &actual,
            &expected,
            2400,
            [0, 196, 2400, 240],
            "Continuous key",
        );
    }
}

#[cfg(test)]
mod axis_margin_tests {
    use super::*;

    #[test]
    fn the_loss_axis_keeps_fractional_percent_ticks_distinct() {
        // R34(e): the empty-loss domain is 0..1%, so quarter-percent ticks must stay distinct.
        assert_eq!(axis_label(0.25, (0.0, 1.0)), "0.25");
        assert_eq!(axis_label(0.5, (0.0, 1.0)), "0.50");
        assert_eq!(axis_label(0.75, (0.0, 1.0)), "0.75");
    }

    #[test]
    fn the_left_margin_grows_with_the_widest_label() {
        let three = label_margin((0.0, 109.0));
        let four = label_margin((0.0, 1_090.0));
        assert!(
            four > three,
            "a four-digit label needs more room than three"
        );
        assert!(three >= SPACE.s6, "the margin never drops below the base");
    }

    #[test]
    fn the_widest_label_counts_a_minus_sign() {
        assert_eq!(widest_label_chars((-5.0, 95.0)), 2);
        assert_eq!(widest_label_chars((-15.0, 95.0)), 3);
    }

    #[test]
    fn the_plot_keeps_room_for_a_four_digit_top_label() {
        let size = Size::new(800.0, 400.0);
        let plot = plot_area(size, (0.0, 1_090.0), (0.0, 120.0));
        let label_width = 4.0 * TYPE.xs * LABEL_EM_PER_CHAR;
        assert!(
            plot.x >= label_width + SPACE.s2,
            "left labels fit left of the well"
        );
        assert!(plot.width > 0.0 && plot.x + plot.width <= size.width);
    }
}
