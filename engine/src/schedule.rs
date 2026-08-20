//! Two-cadence scheduling and per-calendar-day transfer-budget decisions.
//!
//! Continuous mode runs two cadences (spec D5a), not one: a **throughput**
//! cadence (default 1 h) that runs the full D4 round, and a **ping** cadence
//! (default 5 min) that runs the idle-ping phase alone. One cadence forced a
//! choice between an honest 10 s measurement and ~37 GB/day of test traffic;
//! two dissolve it — throughput is the expensive part and hourly is ample for
//! an ISP dispute, ping is nearly free and wants to be dense.
//!
//! The two never overlap. A throughput round due at the same moment as a
//! ping-only round **replaces** it: `record_round_start` advances the ping
//! anchor for every round, because a full round already contains the
//! idle-ping phase. Both anchors are the round's *start*, so a stall or a
//! sleep leaves one overdue slot, never a catch-up burst.

use std::cell::RefCell;
use std::time::{Duration, SystemTime};

use tokio_util::sync::CancellationToken;

use crate::{MetricSelection, RoundKind, Settings};

/// What a scheduled round should run: which cadence produced it, the metrics
/// it may measure, the byte ceiling it must respect, and why throughput was
/// skipped if it was.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundPlan {
    pub kind: RoundKind,
    pub metrics: MetricSelection,
    /// What is left of today's data budget, or `None` when budgeting is off
    /// — which is the default, and then only the clock ends a phase (D6).
    pub byte_ceiling: Option<u64>,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ScheduleState {
    last_full_start: Option<SystemTime>,
    last_ping_start: Option<SystemTime>,
    budget_day: Option<u64>,
    bytes_today: u64,
}

/// Keeps both cadences anchored to a round's start, avoiding catch-up bursts
/// after sleep.
pub struct Scheduler {
    settings: Settings,
    state: RefCell<ScheduleState>,
}

impl Scheduler {
    /// Construct a scheduler. Work execution is intentionally kept by the caller so the engine
    /// remains independent of the store crate (which itself depends on the engine).
    pub fn new(settings: Settings) -> Self {
        Self {
            settings,
            state: RefCell::new(ScheduleState {
                last_full_start: None,
                last_ping_start: None,
                budget_day: None,
                bytes_today: 0,
            }),
        }
    }

    /// Deterministic constructor for schedule policy tests.
    pub fn for_test(settings: Settings) -> Self {
        Self::new(settings)
    }

    /// Wait for scheduled slots until cancelled. Each elapsed slot is recorded once; callers that
    /// execute rounds themselves should use `next_due` and `record_round_start` directly.
    pub async fn run_forever(&mut self, stop: CancellationToken) {
        loop {
            let now = SystemTime::now();
            let due = self.next_due(now);
            let wait = due.duration_since(now).unwrap_or(Duration::ZERO);
            tokio::select! {
                _ = stop.cancelled() => break,
                _ = tokio::time::sleep(wait) => {
                    let start = SystemTime::now();
                    let kind = self.kind_due(start);
                    self.record_round_start(start, kind);
                }
            }
        }
    }

    /// Next slot of either cadence, measured from the matching last round
    /// *start*. An overdue slot stays due now rather than producing one slot
    /// for every missed interval.
    pub fn next_due(&self, now: SystemTime) -> SystemTime {
        self.next_full_due(now).min(self.next_ping_due(now))
    }

    /// Next slot of the throughput cadence.
    pub fn next_full_due(&self, now: SystemTime) -> SystemTime {
        due_from(
            self.state.borrow().last_full_start,
            self.settings.throughput_every,
            now,
        )
    }

    /// Next slot of the ping-only cadence.
    pub fn next_ping_due(&self, now: SystemTime) -> SystemTime {
        due_from(
            self.state.borrow().last_ping_start,
            self.settings.ping_every,
            now,
        )
    }

    /// Which cadence owns the round starting at `now`. A throughput round due
    /// at the same moment as a ping-only one wins — running both would put
    /// two rounds on the same instant, and the ping-only one would measure a
    /// link the full round is about to load.
    pub fn kind_due(&self, now: SystemTime) -> RoundKind {
        if self.next_full_due(now) <= self.next_ping_due(now) {
            RoundKind::Full
        } else {
            RoundKind::PingOnly
        }
    }

    /// Remaining data budget for the UTC calendar day containing `now`;
    /// `None` means budgeting is off.
    pub fn budget_left(&self, now: SystemTime) -> Option<u64> {
        self.refresh_budget_day(now);
        let state = self.state.borrow();
        self.settings
            .daily_budget_bytes
            .map(|limit| limit.saturating_sub(state.bytes_today))
    }

    /// Record an actual round start, anchoring subsequent schedule slots to
    /// this instant. A `Full` round advances BOTH cadences: it contains the
    /// idle-ping phase, so it satisfies the ping cadence too, and this is
    /// what stops the two from drifting into coincident rounds.
    pub fn record_round_start(&self, start: SystemTime, kind: RoundKind) {
        let mut state = self.state.borrow_mut();
        state.last_ping_start = Some(start);
        if kind == RoundKind::Full {
            state.last_full_start = Some(start);
        }
    }

    /// Charge transferred bytes to the day containing `now`. Saturation
    /// prevents accounting overflow.
    pub fn record_bytes(&self, now: SystemTime, bytes: u64) {
        self.refresh_budget_day(now);
        let mut state = self.state.borrow_mut();
        state.bytes_today = state.bytes_today.saturating_add(bytes);
    }

    /// Produce the policy for the round starting at `now`. Budget exhaustion
    /// preserves ping and marks its throughput skip.
    pub fn plan_next_round(&self, now: SystemTime) -> RoundPlan {
        let kind = self.kind_due(now);
        let mut metrics = self.settings.metrics;

        if kind == RoundKind::PingOnly {
            metrics.download = false;
            metrics.upload = false;
            return RoundPlan {
                kind,
                metrics,
                byte_ceiling: None,
                skip_reason: None,
            };
        }

        let left = self.budget_left(now);
        if left == Some(0) && (metrics.download || metrics.upload) {
            metrics.download = false;
            metrics.upload = false;
            RoundPlan {
                kind,
                metrics,
                byte_ceiling: Some(0),
                skip_reason: Some("daily budget exhausted".to_string()),
            }
        } else {
            RoundPlan {
                kind,
                metrics,
                byte_ceiling: left,
                skip_reason: None,
            }
        }
    }

    /// An overdue schedule always creates exactly one immediate round.
    pub fn pending_rounds(&self, now: SystemTime) -> u32 {
        u32::from(self.next_due(now) <= now)
    }

    fn refresh_budget_day(&self, now: SystemTime) {
        let day = now
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() / 86_400);
        let mut state = self.state.borrow_mut();
        if state.budget_day != Some(day) {
            state.budget_day = Some(day);
            state.bytes_today = 0;
        }
    }
}

/// Next slot for one cadence: `every` after the last start of that cadence,
/// clamped to `now` when it is already overdue (one round, not a burst).
/// Never having run means due immediately.
fn due_from(last: Option<SystemTime>, every: Duration, now: SystemTime) -> SystemTime {
    match last {
        Some(last) => {
            let due = last + every;
            if due <= now {
                now
            } else {
                due
            }
        }
        None => now,
    }
}
