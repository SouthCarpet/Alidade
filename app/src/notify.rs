//! Threshold notifications with a per-threshold cooldown (plan 052, UI
//! Task 6).
//!
//! Split in two layers on purpose: [`evaluate`] is a pure decision (master
//! switch, per-threshold enable, cooldown) that a synthetic clock can drive
//! in a test with no toast ever shown; [`show`] is the one line that
//! actually raises a Windows toast, `cfg(windows)`-gated per the Global
//! Constraints, and is never called from a test.

use std::time::{Duration, Instant};

use alidade_engine::RoundResult;

use crate::screens::settings::UiSettings;

/// One of the three thresholds a round can cross.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThresholdKind {
    Ping,
    Download,
    Loss,
}

/// A threshold that fired, with a short human-readable body for the toast.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub kind: ThresholdKind,
    pub title: String,
    pub body: String,
}

/// When each threshold last fired, so the cooldown can silence repeats.
/// One instance lives on `App` for the process lifetime; a fresh instance
/// (every test starts with one) has never fired anything.
#[derive(Debug, Default)]
pub struct NotifyState {
    last_ping: Option<Instant>,
    last_download: Option<Instant>,
    last_loss: Option<Instant>,
}

impl NotifyState {
    fn last_fired(&self, kind: ThresholdKind) -> Option<Instant> {
        match kind {
            ThresholdKind::Ping => self.last_ping,
            ThresholdKind::Download => self.last_download,
            ThresholdKind::Loss => self.last_loss,
        }
    }

    fn mark_fired(&mut self, kind: ThresholdKind, at: Instant) {
        match kind {
            ThresholdKind::Ping => self.last_ping = Some(at),
            ThresholdKind::Download => self.last_download = Some(at),
            ThresholdKind::Loss => self.last_loss = Some(at),
        }
    }
}

/// Not a UI animation value: `design_tokens::scale::MOTION`'s fast/base/
/// draw/live are all sub-second transition timings, unrelated to how long a
/// bad-link warning should stay silent. Kept as one named constant instead
/// of the same magic number repeated at each call site.
pub const DEFAULT_COOLDOWN: Duration = Duration::from_secs(15 * 60);

/// Decide which thresholds (if any) a finished round crosses, honouring the
/// master switch, each threshold's own enable flag, and the cooldown.
/// `now`/`cooldown` are parameters rather than reading the wall clock or a
/// fixed constant internally, so a test can drive both with a synthetic
/// clock. A threshold that fires has its cooldown re-armed from `now`
/// immediately, before the caller has shown anything — a second round
/// finishing inside the same cooldown window must not fire again even if
/// the caller never gets around to raising the toast for the first one.
pub fn evaluate(
    settings: &UiSettings,
    result: &RoundResult,
    state: &mut NotifyState,
    now: Instant,
    cooldown: Duration,
) -> Vec<Notification> {
    if !settings.notify_master {
        return Vec::new();
    }
    candidates(settings, result)
        .into_iter()
        .filter(|candidate| {
            let due = !is_on_cooldown(state, candidate.kind, now, cooldown);
            if due {
                state.mark_fired(candidate.kind, now);
            }
            due
        })
        .collect()
}

fn is_on_cooldown(
    state: &NotifyState,
    kind: ThresholdKind,
    now: Instant,
    cooldown: Duration,
) -> bool {
    matches!(state.last_fired(kind), Some(at) if now.duration_since(at) < cooldown)
}

/// Every threshold this round crosses, ignoring the cooldown — `evaluate`
/// filters that in afterwards. A metric the round could not measure (a
/// skipped phase, no ping target) simply cannot cross its threshold; that
/// is silence, not a false alarm.
fn candidates(settings: &UiSettings, result: &RoundResult) -> Vec<Notification> {
    let mut out = Vec::new();

    if settings.notify_ping_ms.enabled {
        if let Some(avg_ms) = result.ping_idle.and_then(|stats| stats.avg_ms) {
            let limit = settings.notify_ping_ms.value;
            if avg_ms > limit {
                out.push(Notification {
                    kind: ThresholdKind::Ping,
                    title: "Alidade: ping is high".to_string(),
                    body: format!("Ping is {avg_ms:.0} ms, above your {limit:.0} ms alert."),
                });
            }
        }
    }

    if settings.notify_download_mbit.enabled {
        if let Some(down) = &result.down {
            // Same Mbit/s conversion the Home KPI tile uses
            // (`screens::home::throughput_value`).
            let mbit = down.bits_per_sec / 1_000_000.0;
            let limit = settings.notify_download_mbit.value;
            if mbit < limit {
                out.push(Notification {
                    kind: ThresholdKind::Download,
                    title: "Alidade: download is slow".to_string(),
                    body: format!(
                        "Download is {mbit:.1} Mbit/s, below your {limit:.1} Mbit/s alert."
                    ),
                });
            }
        }
    }

    if settings.notify_loss_pct.enabled {
        if let Some(stats) = result.ping_idle {
            let limit = settings.notify_loss_pct.value;
            if stats.loss_pct > limit {
                out.push(Notification {
                    kind: ThresholdKind::Loss,
                    title: "Alidade: packet loss is high".to_string(),
                    body: format!(
                        "Ping loss is {:.0}%, above your {limit:.0}% alert.",
                        stats.loss_pct
                    ),
                });
            }
        }
    }

    out
}

/// Raise the Windows toast for one fired threshold. `cfg(windows)`-gated
/// per the Global Constraints (the tray and toast paths are Windows-only);
/// never called from a test — `evaluate` above is what tests drive.
#[cfg(windows)]
pub fn show(notification: &Notification) {
    let _ = notify_rust::Notification::new()
        .summary(&notification.title)
        .body(&notification.body)
        .show();
}

#[cfg(not(windows))]
pub fn show(_notification: &Notification) {}

#[cfg(test)]
mod tests {
    use super::*;
    use alidade_engine::{PingStats, RoundKind, Throughput};
    use std::time::Duration;

    fn base_settings() -> UiSettings {
        UiSettings {
            notify_master: true,
            ..UiSettings::default()
        }
    }

    fn ping_result(avg_ms: f64, loss_pct: f64) -> RoundResult {
        RoundResult {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            kind: RoundKind::Full,
            down: None,
            up: None,
            ping_idle: Some(PingStats {
                avg_ms: Some(avg_ms),
                min_ms: Some(avg_ms),
                max_ms: Some(avg_ms),
                jitter_ms: Some(0.0),
                loss_pct,
                sent: 10,
            }),
            ping_down: None,
            ping_up: None,
            down_load: None,
            up_load: None,
            capped: false,
            skipped_reason: None,
        }
    }

    fn download_result(mbit: f64) -> RoundResult {
        RoundResult {
            started_at: std::time::SystemTime::UNIX_EPOCH,
            kind: RoundKind::Full,
            down: Some(Throughput {
                bits_per_sec: mbit * 1_000_000.0,
                bytes: 0,
                duration: Duration::from_secs(10),
                capped: false,
            }),
            up: None,
            ping_idle: None,
            ping_down: None,
            ping_up: None,
            down_load: None,
            up_load: None,
            capped: false,
            skipped_reason: None,
        }
    }

    #[test]
    fn the_master_switch_off_means_no_toasts_at_all_even_past_every_threshold() {
        let mut settings = base_settings();
        settings.notify_master = false;
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 10.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &ping_result(500.0, 100.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert!(fired.is_empty());
    }

    #[test]
    fn a_disabled_threshold_never_fires_even_when_crossed() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = false;
        settings.notify_ping_ms.value = 10.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &ping_result(500.0, 0.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert!(fired.is_empty());
    }

    #[test]
    fn ping_above_the_limit_fires_once() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 150.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &ping_result(200.0, 0.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, ThresholdKind::Ping);
    }

    #[test]
    fn ping_at_or_below_the_limit_does_not_fire() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 150.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &ping_result(150.0, 0.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert!(fired.is_empty());
    }

    #[test]
    fn download_below_the_limit_fires() {
        let mut settings = base_settings();
        settings.notify_download_mbit.enabled = true;
        settings.notify_download_mbit.value = 20.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &download_result(5.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, ThresholdKind::Download);
    }

    #[test]
    fn loss_above_the_limit_fires() {
        let mut settings = base_settings();
        settings.notify_loss_pct.enabled = true;
        settings.notify_loss_pct.value = 5.0;
        let mut state = NotifyState::default();
        let fired = evaluate(
            &settings,
            &ping_result(20.0, 50.0),
            &mut state,
            Instant::now(),
            DEFAULT_COOLDOWN,
        );
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].kind, ThresholdKind::Loss);
    }

    #[test]
    fn a_second_breach_inside_the_cooldown_window_stays_silent() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 150.0;
        let mut state = NotifyState::default();
        let cooldown = Duration::from_secs(900);
        let start = Instant::now();

        let first = evaluate(
            &settings,
            &ping_result(300.0, 0.0),
            &mut state,
            start,
            cooldown,
        );
        assert_eq!(first.len(), 1, "the first breach must fire");

        let still_cooling = start + Duration::from_secs(600);
        let second = evaluate(
            &settings,
            &ping_result(300.0, 0.0),
            &mut state,
            still_cooling,
            cooldown,
        );
        assert!(
            second.is_empty(),
            "a repeat inside the cooldown must stay silent"
        );
    }

    #[test]
    fn a_breach_after_the_cooldown_expires_fires_again() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 150.0;
        let mut state = NotifyState::default();
        let cooldown = Duration::from_secs(900);
        let start = Instant::now();

        let first = evaluate(
            &settings,
            &ping_result(300.0, 0.0),
            &mut state,
            start,
            cooldown,
        );
        assert_eq!(first.len(), 1);

        let after_cooldown = start + Duration::from_secs(901);
        let second = evaluate(
            &settings,
            &ping_result(300.0, 0.0),
            &mut state,
            after_cooldown,
            cooldown,
        );
        assert_eq!(
            second.len(),
            1,
            "a breach past the cooldown must fire again"
        );
    }

    #[test]
    fn each_threshold_has_its_own_independent_cooldown() {
        let mut settings = base_settings();
        settings.notify_ping_ms.enabled = true;
        settings.notify_ping_ms.value = 150.0;
        settings.notify_loss_pct.enabled = true;
        settings.notify_loss_pct.value = 5.0;
        let mut state = NotifyState::default();
        let cooldown = Duration::from_secs(900);
        let start = Instant::now();

        // Round 1: only ping breaches (loss is at 0%).
        let first = evaluate(
            &settings,
            &ping_result(300.0, 0.0),
            &mut state,
            start,
            cooldown,
        );
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, ThresholdKind::Ping);

        // Round 2, a second later: ping is still on cooldown, but loss
        // breaches for the first time and must fire anyway — its cooldown
        // is a separate clock from ping's.
        let second = evaluate(
            &settings,
            &ping_result(300.0, 50.0),
            &mut state,
            start + Duration::from_secs(1),
            cooldown,
        );
        assert_eq!(
            second,
            vec![Notification {
                kind: ThresholdKind::Loss,
                title: "Alidade: packet loss is high".to_string(),
                body: "Ping loss is 50%, above your 5% alert.".to_string(),
            }],
            "loss must fire on its own clock even while ping is still cooling down"
        );
    }
}
