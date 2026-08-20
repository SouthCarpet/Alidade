//! Interval scheduling and per-calendar-day transfer-budget decisions.

use std::cell::RefCell;
use std::time::{Duration, SystemTime};

use tokio_util::sync::CancellationToken;

use crate::{MetricSelection, Settings};

/// The metrics a scheduled round should run and, if applicable, why throughput was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoundPlan {
    pub metrics: MetricSelection,
    pub skip_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct ScheduleState {
    last_round_start: Option<SystemTime>,
    budget_day: Option<u64>,
    bytes_today: u64,
}

/// Keeps cadence anchored to a round's start, avoiding catch-up bursts after sleep.
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
                last_round_start: None,
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
                _ = tokio::time::sleep(wait) => self.record_round_start(SystemTime::now()),
            }
        }
    }

    /// Next slot, measured from the last round *start*. An overdue slot stays due now rather than
    /// producing one slot for every missed interval.
    pub fn next_due(&self, now: SystemTime) -> SystemTime {
        match self.state.borrow().last_round_start {
            Some(last) => {
                let due = last + self.settings.interval;
                if due <= now {
                    now
                } else {
                    due
                }
            }
            None => now,
        }
    }

    /// Remaining data budget for the current UTC calendar day; `None` means budgeting is off.
    pub fn budget_left_today(&self) -> Option<u64> {
        self.refresh_budget_day(SystemTime::now());
        let state = self.state.borrow();
        self.settings
            .daily_budget_bytes
            .map(|limit| limit.saturating_sub(state.bytes_today))
    }

    /// Record an actual round start, anchoring subsequent schedule slots to this instant.
    pub fn record_round_start(&self, start: SystemTime) {
        self.state.borrow_mut().last_round_start = Some(start);
    }

    /// Charge transferred bytes to the current day. Saturation prevents accounting overflow.
    pub fn record_bytes(&self, bytes: u64) {
        self.refresh_budget_day(SystemTime::now());
        let mut state = self.state.borrow_mut();
        state.bytes_today = state.bytes_today.saturating_add(bytes);
    }

    /// Produce one round policy. Budget exhaustion preserves ping and marks its throughput skip.
    pub fn plan_next_round(&self) -> RoundPlan {
        let mut metrics = self.settings.metrics;
        let over_budget = self.budget_left_today().is_some_and(|left| left == 0);
        if over_budget && (metrics.download || metrics.upload) {
            metrics.download = false;
            metrics.upload = false;
            RoundPlan {
                metrics,
                skip_reason: Some("daily budget exhausted".to_string()),
            }
        } else {
            RoundPlan {
                metrics,
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
