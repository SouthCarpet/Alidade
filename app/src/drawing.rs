//! The latest round, with one domain per unit and no dimension-line primitive.

use design_tokens::scale::{SPACE, TYPE};
use iced::alignment;
use iced::widget::{canvas, column, text, Canvas};
use iced::{mouse, Color, Element, Fill, Point, Rectangle, Renderer, Size, Theme};

use crate::state::{App, LatestRound, Message};
use crate::theme::Palette;
use crate::ui::{font_numeric, font_regular};

#[derive(Debug, Clone)]
pub struct Scale {
    pub unit: &'static str,
    pub values: [Option<f64>; 2],
}

impl Scale {
    pub fn end_label(&self) -> String {
        if self.unit == "ms" {
            format!("{:.1} ms", self.domain().1)
        } else {
            format!("{:.2} {}", self.domain().1, self.unit)
        }
    }

    pub fn domain(&self) -> (f64, f64) {
        (0.0, self.values.into_iter().flatten().fold(0.0, f64::max))
    }

    fn fraction(&self, value: f64) -> f64 {
        let high = self.domain().1;
        if high == 0.0 {
            0.0
        } else {
            value / high
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagram {
    pub speed: Scale,
    pub latency: Scale,
}

/// Only measured bars and marks can reach the canvas. Axes are plain scale baselines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Primitive {
    Bar { series: usize, fraction: f64 },
    Mark { series: usize, fraction: f64 },
}

pub fn diagram(round: Option<&LatestRound>) -> Result<Diagram, &'static str> {
    round
        .map(Diagram::from_round)
        .ok_or("No diagram: no round has been measured.")
}

impl Diagram {
    pub fn from_round(round: &LatestRound) -> Self {
        let (speed, latency) = match round {
            LatestRound::Live(outcome) => {
                let result = &outcome.result;
                (
                    [result.down, result.up].map(|speed| speed.map(|s| s.bits_per_sec)),
                    [
                        result.ping_idle.and_then(|p| p.avg_ms),
                        mean_present(
                            result.ping_down.and_then(|p| p.avg_ms),
                            result.ping_up.and_then(|p| p.avg_ms),
                        ),
                    ],
                )
            }
            LatestRound::Stored(row) => (
                [row.down_bps, row.up_bps],
                [row.ping_idle_ms, super::ping_load_ms(row)],
            ),
        };
        Self {
            speed: Scale {
                unit: "Mbit/s",
                values: speed.map(|value| valid(value).map(|bps| bps / 1_000_000.0)),
            },
            latency: Scale {
                unit: "ms",
                values: latency.map(valid),
            },
        }
    }

    pub fn difference_text(&self) -> String {
        match self.latency.values {
            [Some(idle), Some(load)] => format!("Ping change under load: {:+.1} ms", load - idle),
            _ => "Ping change unavailable: both ping values are needed.".into(),
        }
    }

    pub fn primitives(&self) -> Vec<Primitive> {
        let bars = self
            .speed
            .values
            .into_iter()
            .enumerate()
            .filter_map(|(series, value)| {
                value.map(|value| Primitive::Bar {
                    series,
                    fraction: self.speed.fraction(value),
                })
            });
        let marks = self
            .latency
            .values
            .into_iter()
            .enumerate()
            .filter_map(|(series, value)| {
                value.map(|value| Primitive::Mark {
                    series,
                    fraction: self.latency.fraction(value),
                })
            });
        bars.chain(marks).collect()
    }
}

fn valid(value: Option<f64>) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0)
}

fn mean_present(left: Option<f64>, right: Option<f64>) -> Option<f64> {
    match (left, right) {
        (Some(left), Some(right)) => Some((left + right) / 2.0),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

pub fn view(app: &App, palette: Palette) -> Element<'static, Message> {
    let data = match diagram(app.last_round.as_ref()) {
        Ok(data) => data,
        Err(reason) => {
            return text(reason)
                .size(TYPE.sm)
                .font(font_regular())
                .color(palette.muted())
                .into()
        }
    };
    let difference = data.difference_text();
    let frames = app
        .measurement_draw_in
        .as_ref()
        .map_or([(0.0, 1.0); 3], |motion| motion.frames(app.activity_at));
    palette.measurement_sheet(
        column![
            Canvas::new(DiagramCanvas {
                data,
                palette,
                frames
            })
            .width(Fill)
            .height(SPACE.s6 * 4.0),
            text(difference)
                .size(TYPE.sm)
                .font(font_numeric())
                .color(palette.muted()),
        ]
        .spacing(SPACE.s1),
    )
}

struct DiagramCanvas {
    data: Diagram,
    palette: Palette,
    frames: [(f32, f32); 3],
}

impl canvas::Program<Message> for DiagramCanvas {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let width = (bounds.width - SPACE.s6) / 2.0;
        self.scale(
            &mut frame,
            &self.data.speed,
            0.0,
            width,
            ["Download", "Upload"],
        );
        self.scale(
            &mut frame,
            &self.data.latency,
            width + SPACE.s6,
            width,
            ["Idle ping", "Under load"],
        );
        for primitive in self.data.primitives() {
            self.mark(&mut frame, primitive, width);
        }
        vec![frame.into_geometry()]
    }
}

impl DiagramCanvas {
    fn scale(
        &self,
        frame: &mut canvas::Frame,
        scale: &Scale,
        x: f32,
        width: f32,
        labels: [&str; 2],
    ) {
        let start = x + SPACE.s6 * 4.0;
        let end = x + width - SPACE.s2;
        let title = if scale.unit == "ms" {
            "Latency"
        } else {
            "Throughput"
        };
        canvas_text(
            frame,
            format!("{title} ({})", scale.unit),
            Point::new(x, SPACE.s2),
            self.palette.text(),
            false,
        );
        for (index, label) in labels.into_iter().enumerate() {
            canvas_text(
                frame,
                label,
                Point::new(x, SPACE.s8 + index as f32 * SPACE.s6),
                self.palette.muted(),
                false,
            );
        }
        let y = SPACE.s6 * 3.0;
        if scale.values.iter().all(Option::is_none) {
            canvas_text(
                frame,
                "No values in this round",
                Point::new(start, y + SPACE.s3),
                self.palette.muted(),
                false,
            );
            return;
        }
        frame.stroke(
            &canvas::Path::line(Point::new(start, y), Point::new(end, y)),
            canvas::Stroke::default()
                .with_width(1.0)
                .with_color(self.palette.border()),
        );
        canvas_text(
            frame,
            "0",
            Point::new(start, y + SPACE.s3),
            self.palette.muted(),
            false,
        );
        canvas_text(
            frame,
            scale.end_label(),
            Point::new(end, y + SPACE.s3),
            self.palette.muted(),
            true,
        );
    }

    fn mark(&self, frame: &mut canvas::Frame, primitive: Primitive, width: f32) {
        let (series, fraction, panel, movement) = match primitive {
            Primitive::Bar { series, fraction } => (series, fraction, 0.0, self.frames[series]),
            Primitive::Mark { series, fraction } => {
                (series, fraction, width + SPACE.s6, self.frames[2])
            }
        };
        let start = panel + SPACE.s6 * 4.0;
        let length = (width - SPACE.s6 * 4.0 - SPACE.s2).max(0.0);
        let y = SPACE.s8 + series as f32 * SPACE.s6;
        let color = Color {
            a: movement.1,
            ..self.palette.accent()
        };
        match primitive {
            Primitive::Bar { .. } => {
                frame.fill_rectangle(
                    Point::new(start + movement.0, y - SPACE.s1),
                    Size::new(length * fraction as f32, SPACE.s2),
                    color,
                );
            }
            Primitive::Mark { .. } => {
                let x = start + length * fraction as f32 + movement.0;
                frame.fill(&canvas::Path::circle(Point::new(x, y), SPACE.s1), color);
            }
        }
    }
}

fn canvas_text(
    frame: &mut canvas::Frame,
    content: impl Into<String>,
    position: Point,
    color: Color,
    right: bool,
) {
    frame.fill_text(canvas::Text {
        content: content.into(),
        position,
        color,
        size: TYPE.xs.into(),
        font: font_numeric(),
        align_x: if right {
            alignment::Horizontal::Right
        } else {
            alignment::Horizontal::Left
        }
        .into(),
        align_y: alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}
