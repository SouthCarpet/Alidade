use alidade_engine::{RoundKind, Scheduler, Settings};
use std::time::{Duration, SystemTime};

const MINUTE: Duration = Duration::from_secs(60);

fn hours(n: u64) -> Duration {
    Duration::from_secs(n * 3600)
}

fn minutes(n: u64) -> Duration {
    MINUTE * n as u32
}

/// Run the scheduler forward over `span`, taking every slot it offers exactly
/// as the CLI loop does: plan at the slot, record the start, jump to the next
/// slot. Returns `(minutes since start, kind)` per round.
fn rounds_over(sched: &Scheduler, start: SystemTime, span: Duration) -> Vec<(u64, RoundKind)> {
    let end = start + span;
    let mut now = start;
    let mut out = Vec::new();
    while now <= end {
        let plan = sched.plan_next_round(now);
        sched.record_round_start(now, plan.kind);
        let at = now
            .duration_since(start)
            .expect("the simulation never walks backwards")
            .as_secs()
            / 60;
        out.push((at, plan.kind));
        now = sched.next_due(now);
        // A slot that is still "now" after recording it would spin forever;
        // the cadences are minutes apart, so this only trips on a bug.
        assert!(out.len() < 1000, "the schedule stopped advancing: {out:?}");
    }
    out
}

#[test]
fn next_due_is_interval_after_the_last_round_start_not_after_it_finished() {
    let s = Settings::test_default(hours(1), minutes(5));
    let sched = Scheduler::for_test(s);
    let start = SystemTime::now();
    sched.record_round_start(start, RoundKind::Full);
    // 35 s into a round that has not finished, the next slot is still the
    // ping cadence measured from this round's START.
    assert_eq!(
        sched.next_due(start + Duration::from_secs(35)),
        start + minutes(5)
    );
    assert_eq!(
        sched.next_full_due(start + Duration::from_secs(35)),
        start + hours(1)
    );
}

#[test]
fn budget_exhaustion_skips_throughput_but_not_pings() {
    let mut s = Settings::test_default(minutes(1), minutes(1));
    s.daily_budget_bytes = Some(1_000);
    let sched = Scheduler::for_test(s);
    let now = SystemTime::now();
    sched.record_bytes(now, 2_000);

    let plan = sched.plan_next_round(now);

    assert_eq!(plan.kind, RoundKind::Full, "the throughput cadence was due");
    assert!(!plan.metrics.download && !plan.metrics.upload);
    assert!(plan.metrics.ping);
    assert_eq!(plan.skip_reason.as_deref(), Some("daily budget exhausted"));
}

/// D6: with no budget there is no byte ceiling at all, so only the clock ends
/// a phase. With a budget, what is left of it is the ceiling — the one thing
/// allowed to end a phase by byte count.
#[test]
fn the_byte_ceiling_comes_only_from_the_daily_budget() {
    let now = SystemTime::now();

    let no_budget = Scheduler::for_test(Settings::test_default(hours(1), minutes(5)));
    assert_eq!(
        no_budget.plan_next_round(now).byte_ceiling,
        None,
        "the default is no ceiling; the clock alone ends a phase"
    );

    let mut with_budget = Settings::test_default(hours(1), minutes(5));
    with_budget.daily_budget_bytes = Some(5_000_000_000);
    let sched = Scheduler::for_test(with_budget);
    sched.record_bytes(now, 1_000_000_000);
    assert_eq!(
        sched.plan_next_round(now).byte_ceiling,
        Some(4_000_000_000),
        "the ceiling is what is left of today's budget"
    );
}

#[test]
fn a_missed_interval_after_sleep_runs_once_not_a_burst() {
    let s = Settings::test_default(hours(1), minutes(5));
    let sched = Scheduler::for_test(s);
    let t0 = SystemTime::now();
    sched.record_round_start(t0, RoundKind::Full);
    let now = t0 + Duration::from_secs(3 * 3600);

    // Both cadences are overdue after a three-hour sleep — three throughput
    // slots and thirty-five ping slots were missed. That is one round.
    assert!(sched.next_due(now) <= now, "due immediately");
    assert!(sched.next_full_due(now) <= now, "the throughput cadence is overdue too");
    assert_eq!(sched.pending_rounds(now), 1, "no catch-up burst");
    assert_eq!(
        sched.kind_due(now),
        RoundKind::Full,
        "when both are overdue the full round wins, as at any other coincidence"
    );
}

/// A ping-only round must not reset the throughput cadence: dense pings would
/// otherwise push the hourly round out forever and the app would never
/// measure a speed again.
#[test]
fn a_ping_only_round_does_not_postpone_the_throughput_round() {
    let sched = Scheduler::for_test(Settings::test_default(hours(1), minutes(5)));
    let t0 = SystemTime::now();
    sched.record_round_start(t0, RoundKind::Full);
    sched.record_round_start(t0 + minutes(5), RoundKind::PingOnly);
    sched.record_round_start(t0 + minutes(10), RoundKind::PingOnly);

    assert_eq!(
        sched.next_full_due(t0 + minutes(10)),
        t0 + hours(1),
        "the throughput cadence is still measured from the last FULL round"
    );
}

/// **F1, mutation-checked.** Over a simulated span the two cadences produce
/// one round per slot, with a full round replacing the ping-only round it
/// coincides with — never two rounds at the same instant, and never a
/// ping-only round measuring a link a full round is about to load.
///
/// Two hours at 1 h throughput / 15 min ping is one full round at 0, 60 and
/// 120 minutes and one ping round at every other quarter hour: nine rounds,
/// not the eleven you get if both cadences fire at the coincidences.
///
/// Mutation: let `record_round_start` advance the ping anchor only for
/// `PingOnly` rounds (that is, drop the replacement rule and let both fire).
/// The sequence then carries a second, ping-only round at minute 60 and the
/// assertion fails.
#[test]
fn a_full_round_replaces_the_ping_round_it_coincides_with() {
    let sched = Scheduler::for_test(Settings::test_default(hours(1), minutes(15)));
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);

    let rounds = rounds_over(&sched, start, hours(2));

    use RoundKind::{Full, PingOnly};
    assert_eq!(
        rounds,
        vec![
            (0, Full),
            (15, PingOnly),
            (30, PingOnly),
            (45, PingOnly),
            (60, Full),
            (75, PingOnly),
            (90, PingOnly),
            (105, PingOnly),
            (120, Full),
        ],
        "the two cadences must interleave, with the full round replacing the coincident ping round"
    );

    let mut minutes_seen: Vec<u64> = rounds.iter().map(|(at, _)| *at).collect();
    let before = minutes_seen.len();
    minutes_seen.dedup();
    assert_eq!(
        minutes_seen.len(),
        before,
        "two rounds started at the same instant: {rounds:?}"
    );
}

/// The no-catch-up property, now over both cadences and over a real span: a
/// three-hour stall must not produce three hours of backlogged rounds.
#[test]
fn neither_cadence_bursts_after_a_stall() {
    let sched = Scheduler::for_test(Settings::test_default(hours(1), minutes(15)));
    let start = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    sched.record_round_start(start, RoundKind::Full);

    // Nothing ran for three hours (sleep, stall, laptop lid). Resume.
    let wake = start + hours(3);
    let rounds = rounds_over(&sched, wake, minutes(20));

    assert_eq!(
        rounds.first(),
        Some(&(0, RoundKind::Full)),
        "one round is due at once, and it is the full one"
    );
    assert_eq!(
        rounds,
        vec![(0, RoundKind::Full), (15, RoundKind::PingOnly)],
        "twelve missed ping slots and three missed throughput slots become one round each, not a burst: {rounds:?}"
    );
}
