use alidade_engine::{Scheduler, Settings};
use std::time::{Duration, SystemTime};

#[test]
fn next_due_is_interval_after_the_last_round_start_not_after_it_finished() {
    let s = Settings::test_default(Duration::from_secs(600));
    let sched = Scheduler::for_test(s);
    let start = SystemTime::now();
    sched.record_round_start(start);
    let due = sched.next_due(start + Duration::from_secs(35));
    assert_eq!(due, start + Duration::from_secs(600));
}

#[test]
fn budget_exhaustion_skips_throughput_but_not_pings() {
    let mut s = Settings::test_default(Duration::from_secs(60));
    s.daily_budget_bytes = Some(1_000);
    let sched = Scheduler::for_test(s);
    sched.record_bytes(2_000);
    let plan = sched.plan_next_round();
    assert!(!plan.metrics.download && !plan.metrics.upload);
    assert!(plan.metrics.ping);
    assert_eq!(plan.skip_reason.as_deref(), Some("daily budget exhausted"));
}

#[test]
fn a_missed_interval_after_sleep_runs_once_not_a_burst() {
    let s = Settings::test_default(Duration::from_secs(600));
    let sched = Scheduler::for_test(s);
    let t0 = SystemTime::now();
    sched.record_round_start(t0);
    let now = t0 + Duration::from_secs(3 * 3600);
    let due = sched.next_due(now);
    assert!(due <= now, "due immediately");
    assert_eq!(sched.pending_rounds(now), 1, "no catch-up burst");
}
