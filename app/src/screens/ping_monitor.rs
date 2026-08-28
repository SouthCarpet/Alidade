//! Ping monitor screen (plan Task 4): one row per enabled target, sparkline
//! from the last N samples, Start/Stop for the dense 1 s loop.

use design_tokens::scale::{SPACE, TYPE};
use iced::mouse;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{button, column, container, row, text, Canvas};
use iced::{Color, Element, Fill, Length, Point, Rectangle, Renderer};

use crate::chart::y_scale;
use crate::state::{current_rtt_ms, target_is_failing, target_stats, App, Message, TargetLive};
use crate::theme::Palette;
use crate::ui::{font_medium, font_numeric, font_regular};

use super::{format_ms, format_pct, with_toast};

pub(crate) const SPARKLINE_SAMPLES: usize = 60;

pub fn view(app: &App) -> Element<'_, Message> {
    let palette = Palette::light();
    let start_stop = if app.ping_running {
        button(text("Stop").size(TYPE.sm).font(font_medium()))
            .padding([SPACE.s2, SPACE.s4])
            .style(palette.btn_danger())
            .on_press(Message::StopPingMonitor)
    } else {
        button(text("Start").size(TYPE.sm).font(font_medium()))
            .padding([SPACE.s2, SPACE.s4])
            .style(palette.btn_primary())
            .on_press(Message::StartPingMonitor)
    };

    let mut rows: Vec<Element<'_, Message>> = vec![start_stop.into(), header(palette)];
    if app.ping_targets.is_empty() {
        rows.push(
            text("No enabled targets. Add some in Settings, then press Start.")
                .size(TYPE.sm)
                .font(font_regular())
                .color(palette.muted())
                .into(),
        );
    } else {
        for target in &app.ping_targets {
            rows.push(target_row(palette, target));
        }
    }

    with_toast(
        palette,
        app,
        container(column(rows).spacing(SPACE.s3).width(Fill))
            .width(Fill)
            .height(Fill)
            .into(),
    )
}

fn header(palette: Palette) -> Element<'static, Message> {
    row![
        label(palette, "Target", 3),
        label(palette, "Current", 2),
        label(palette, "Avg", 2),
        label(palette, "Min", 2),
        label(palette, "Max", 2),
        label(palette, "Jitter", 2),
        label(palette, "Loss", 2),
        label(palette, "Sparkline", 3),
    ]
    .spacing(SPACE.s2)
    .into()
}

fn label(palette: Palette, text_value: &'static str, portion: u16) -> Element<'static, Message> {
    text(text_value)
        .size(TYPE.xs)
        .font(font_medium())
        .color(palette.muted())
        .width(Length::FillPortion(portion))
        .into()
}

fn target_row<'a>(palette: Palette, target: &TargetLive) -> Element<'a, Message> {
    let failing = target_is_failing(target);
    let color = if failing {
        palette.danger()
    } else {
        palette.text()
    };
    let stats = target_stats(target);
    let current = match current_rtt_ms(target) {
        Some(ms) => format_ms(Some(ms)),
        None if target.samples.is_empty() => format_ms(None),
        None => "no answer".to_string(),
    };
    let spark = sparkline_points(&target.samples, SPARKLINE_SAMPLES);
    let spark_canvas: Canvas<Sparkline, Message> = Canvas::new(Sparkline {
        points: spark,
        palette,
    })
    .width(Fill)
    .height(Length::Fixed(SPACE.s6));

    palette.card(
        true,
        row![
            text(target.name.clone())
                .size(TYPE.sm)
                .font(font_medium())
                .color(color)
                .width(Length::FillPortion(3)),
            numeric(current, color, 2),
            numeric(format_ms(stats.avg_ms), color, 2),
            numeric(format_ms(stats.min_ms), color, 2),
            numeric(format_ms(stats.max_ms), color, 2),
            numeric(format_ms(stats.jitter_ms), color, 2),
            numeric(format_pct(Some(stats.loss_pct)), color, 2),
            container(spark_canvas).width(Length::FillPortion(3)),
        ]
        .spacing(SPACE.s2)
        .align_y(iced::Alignment::Center),
    )
}

fn numeric<'a>(value: String, color: Color, portion: u16) -> Element<'a, Message> {
    text(value)
        .size(TYPE.sm)
        .font(font_numeric())
        .color(color)
        .align_x(iced::alignment::Horizontal::Right)
        .width(Length::FillPortion(portion))
        .into()
}

/// Last N RTT samples as milliseconds. A loss is `None`, never `0.0`.
pub(crate) fn sparkline_points(
    samples: &[alidade_engine::PingSample],
    n: usize,
) -> Vec<Option<f64>> {
    let start = samples.len().saturating_sub(n);
    samples[start..]
        .iter()
        .map(|sample| sample.rtt.map(|rtt| rtt.as_secs_f64() * 1000.0))
        .collect()
}

struct Sparkline {
    points: Vec<Option<f64>>,
    palette: Palette,
}

impl<Message> canvas::Program<Message> for Sparkline {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let values: Vec<f64> = self.points.iter().copied().flatten().collect();
        let scale = y_scale(&values, true);
        let width = bounds.width.max(1.0);
        let height = bounds.height.max(1.0);
        let path = Path::new(|builder| {
            let mut connected = false;
            for (index, point) in self.points.iter().enumerate() {
                let Some(value) = point else {
                    connected = false;
                    continue;
                };
                let x = if self.points.len() < 2 {
                    width / 2.0
                } else {
                    width * index as f32 / (self.points.len() - 1) as f32
                };
                let span = (scale.1 - scale.0).max(f64::EPSILON);
                let fraction = ((value - scale.0) / span).clamp(0.0, 1.0) as f32;
                let y = height * (1.0 - fraction);
                let position = Point::new(x, y);
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
            Stroke::default()
                .with_width(1.5)
                .with_color(self.palette.accent()),
        );
        vec![frame.into_geometry()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alidade_engine::PingSample;
    use std::time::{Duration, UNIX_EPOCH};

    fn sample(rtt_ms: Option<f64>) -> PingSample {
        PingSample {
            at: UNIX_EPOCH,
            rtt: rtt_ms.map(|ms| Duration::from_secs_f64(ms / 1000.0)),
        }
    }

    #[test]
    fn sparkline_keeps_losses_as_gaps_not_zeros() {
        let points = sparkline_points(&[sample(Some(20.0)), sample(None), sample(Some(22.0))], 60);
        assert_eq!(points, vec![Some(20.0), None, Some(22.0)]);
        assert!(!points.contains(&Some(0.0)));
    }

    #[test]
    fn sparkline_keeps_only_the_last_n_samples() {
        let samples: Vec<_> = (0..80).map(|i| sample(Some(i as f64))).collect();
        let points = sparkline_points(&samples, 60);
        assert_eq!(points.len(), 60);
        assert_eq!(points.first().copied().flatten(), Some(20.0));
        assert_eq!(points.last().copied().flatten(), Some(79.0));
    }

    #[test]
    fn a_failing_sample_is_not_formatted_as_zero() {
        assert_eq!(format_ms(None).trim(), "—");
        assert!(!format_ms(None).contains('0'));
    }
}
