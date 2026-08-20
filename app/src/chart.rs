//! The Continuous screen's honest, dual-axis measurement chart.
//!
//! `None` is deliberately carried from storage to the path builder. A
//! missing measurement starts a new sub-path instead of becoming a zero at
//! the floor of the chart.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use alidade_store::RoundRow;
use design_tokens::scale::{RADIUS, SHADOW_RAISE_DARK, SHADOW_RAISE_LIGHT, SPACE, TYPE};
use iced::alignment;
use iced::mouse;
use iced::widget::canvas::{self, Action, Frame, Geometry, LineDash, Path, Stroke, Text};
use iced::{Color, Font, Point as CanvasPoint, Rectangle, Renderer, Size};

use crate::theme::Palette;

const DASH_PATTERNS: [&[f32]; 6] = [
    &[],
    &[8.0, 5.0],
    &[1.5, 4.0],
    &[8.0, 4.0, 1.5, 4.0],
    &[16.0, 6.0],
    &[8.0, 4.0, 1.5, 4.0, 1.5, 4.0],
];

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
}

#[derive(Debug, Default)]
pub struct ChartState {
    hovered: Option<usize>,
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

        let plot = plot_area(bounds.size());
        let hovered = cursor.position_in(bounds).and_then(|position| {
            if plot.rectangle().contains(position) {
                nearest_point(&self.points, plot, position.x)
            } else {
                None
            }
        });
        if state.hovered == hovered {
            None
        } else {
            state.hovered = hovered;
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
        let plot = plot_area(frame.size());
        let theme = *self.palette.theme;
        let throughput = self
            .points
            .iter()
            .flat_map(|point| [point.download_mbit, point.upload_mbit])
            .flatten()
            .collect::<Vec<_>>();
        let latency = self
            .points
            .iter()
            .flat_map(|point| {
                [
                    point.ping_idle_ms,
                    point.ping_load_ms,
                    point.jitter_ms,
                    point.loss_pct,
                ]
            })
            .flatten()
            .collect::<Vec<_>>();
        let throughput_scale = y_scale(&throughput, false);
        let latency_scale = y_scale(&latency, true);

        // The chart itself is a recessed well by colour alone: no shadow is
        // attached to this rectangle.
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
                .with_color(theme.border.into()),
        );

        draw_grid(&mut frame, plot, theme.chart_grid.into());
        draw_axes(
            &mut frame,
            plot,
            throughput_scale,
            latency_scale,
            self.palette.muted(),
        );

        // Series one is a min/max envelope around each available download
        // measurement. Its alpha is the token-defined chart band alpha.
        draw_download_band(
            &mut frame,
            &self.points,
            plot,
            throughput_scale,
            theme.chart[0].into(),
            theme.chart_band_alpha,
        );

        if !throughput.is_empty() {
            let average = throughput.iter().sum::<f64>() / throughput.len() as f64;
            let y = y_for(average, throughput_scale, plot);
            frame.stroke(
                &Path::line(
                    CanvasPoint::new(plot.x, y),
                    CanvasPoint::new(plot.x + plot.width, y),
                ),
                Stroke {
                    line_dash: LineDash {
                        segments: &[4.0, 3.0],
                        offset: 0,
                    },
                    ..Stroke::default()
                        .with_width(1.0)
                        .with_color(self.palette.muted())
                },
            );
        }

        let series: [fn(&Point) -> Option<f64>; 6] = [
            |point| point.download_mbit,
            |point| point.upload_mbit,
            |point| point.ping_idle_ms,
            |point| point.ping_load_ms,
            |point| point.jitter_ms,
            |point| point.loss_pct,
        ];
        for (index, metric) in series.into_iter().enumerate() {
            let scale = if index < 2 {
                throughput_scale
            } else {
                latency_scale
            };
            draw_series(
                &mut frame,
                &self.points,
                plot,
                scale,
                metric,
                theme.chart[index].into(),
                DASH_PATTERNS[index],
            );
        }

        if let Some(index) = state.hovered.filter(|index| *index < self.points.len()) {
            draw_hover(
                &mut frame,
                &self.points[index],
                index,
                &self.points,
                plot,
                throughput_scale,
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
        if cursor
            .position_in(bounds)
            .is_some_and(|position| plot_area(bounds.size()).rectangle().contains(position))
        {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

fn plot_area(size: Size) -> Plot {
    let x = SPACE.s6;
    let y = SPACE.s4;
    let width = (size.width - (SPACE.s6 * 2.0)).max(0.0);
    let height = (size.height - (SPACE.s4 + SPACE.s6)).max(0.0);
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
    left: (f64, f64),
    right: (f64, f64),
    color: Color,
) {
    for step in 0..=4 {
        let fraction = step as f64 / 4.0;
        let y = plot.y + plot.height * (1.0 - fraction) as f32;
        let left_value = left.0 + ((left.1 - left.0) * fraction);
        let right_value = right.0 + ((right.1 - right.0) * fraction);
        draw_text(
            frame,
            format!("{left_value:.0}"),
            CanvasPoint::new(plot.x - SPACE.s2, y),
            color,
            TYPE.xs,
            alignment::Horizontal::Right,
        );
        draw_text(
            frame,
            format!("{right_value:.0}"),
            CanvasPoint::new(plot.x + plot.width + SPACE.s2, y),
            color,
            TYPE.xs,
            alignment::Horizontal::Left,
        );
    }
    draw_text(
        frame,
        "Mbit/s",
        CanvasPoint::new(plot.x, plot.y - SPACE.s2),
        color,
        TYPE.xs,
        alignment::Horizontal::Left,
    );
    draw_text(
        frame,
        "ms",
        CanvasPoint::new(plot.x + plot.width, plot.y - SPACE.s2),
        color,
        TYPE.xs,
        alignment::Horizontal::Right,
    );
}

fn draw_download_band(
    frame: &mut Frame<Renderer>,
    points: &[Point],
    plot: Plot,
    scale: (f64, f64),
    color: Color,
    alpha: f32,
) {
    let mut run: Vec<(f32, f32, f32)> = Vec::new();
    for (index, point) in points.iter().enumerate() {
        if let Some(value) = point.download_mbit {
            let x = x_for(points, index, plot);
            run.push((
                x,
                y_for(value * 1.12, scale, plot),
                y_for(value * 0.88, scale, plot),
            ));
        } else {
            fill_band_run(frame, &run, color, alpha);
            run.clear();
        }
    }
    fill_band_run(frame, &run, color, alpha);
}

fn fill_band_run(frame: &mut Frame<Renderer>, run: &[(f32, f32, f32)], color: Color, alpha: f32) {
    if run.len() < 2 {
        return;
    }
    let path = Path::new(|builder| {
        builder.move_to(CanvasPoint::new(run[0].0, run[0].1));
        for (x, upper, _) in run.iter().skip(1) {
            builder.line_to(CanvasPoint::new(*x, *upper));
        }
        for (x, _, lower) in run.iter().rev() {
            builder.line_to(CanvasPoint::new(*x, *lower));
        }
        builder.close();
    });
    frame.fill(&path, Color { a: alpha, ..color });
}

fn draw_series(
    frame: &mut Frame<Renderer>,
    points: &[Point],
    plot: Plot,
    scale: (f64, f64),
    metric: fn(&Point) -> Option<f64>,
    color: Color,
    dash: &'static [f32],
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
    frame.stroke(
        &path,
        Stroke {
            line_dash: LineDash {
                segments: dash,
                offset: 0,
            },
            ..Stroke::default().with_width(2.0).with_color(color)
        },
    );
}

fn draw_hover(
    frame: &mut Frame<Renderer>,
    point: &Point,
    index: usize,
    points: &[Point],
    plot: Plot,
    throughput_scale: (f64, f64),
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
    if let Some(download) = point.download_mbit {
        let y = y_for(download, throughput_scale, plot);
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
    let shadow = if palette.dark {
        SHADOW_RAISE_DARK
    } else {
        SHADOW_RAISE_LIGHT
    };
    let card = Path::rounded_rectangle(
        CanvasPoint::new(card_x, card_y),
        Size::new(card_width, card_height),
        radius,
    );
    let shadow_path = Path::rounded_rectangle(
        CanvasPoint::new(card_x + shadow.x, card_y + shadow.y),
        Size::new(card_width, card_height),
        radius,
    );
    frame.fill(&shadow_path, iced::Color::from(shadow.color));
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
        font: Font::MONOSPACE,
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
