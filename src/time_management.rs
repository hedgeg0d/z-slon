//! Wall-clock budgets. Adaptive stopping deliberately keeps the existing
//! linear allocation formula, but can finish an iteration within a hard cap.
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tokio::sync::Notify;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TimeMode {
    Linear,
    #[default]
    Adaptive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub optimum: Option<Duration>,
    pub maximum: Duration,
}

impl Budget {
    pub fn clock(mode: TimeMode, remaining: u64, increment: u64, overhead: u64,
                 moves_to_go: Option<u64>) -> Self {
        let usable = remaining.saturating_sub(overhead);
        let horizon = moves_to_go.filter(|&n| n > 0).unwrap_or(25);
        let increment_share = (increment / 4) * 3 + (increment % 4) * 3 / 4;
        let target = (usable / horizon).saturating_add(increment_share);
        // Preserve the old allocation at normal clock values, but never let
        // its 50 ms floor exceed the usable clock in time trouble.
        let cap = if moves_to_go == Some(1) { usable } else { (usable / 2).max(50) };
        let optimum_ms = target.max(50).min(cap).min(usable);
        let maximum_ms = match mode {
            TimeMode::Linear => optimum_ms,
            TimeMode::Adaptive => optimum_ms.saturating_mul(3)
                .min((usable / 2).max(optimum_ms)).min(usable),
        };
        Self {
            optimum: (mode == TimeMode::Adaptive).then(|| Duration::from_millis(optimum_ms)),
            maximum: Duration::from_millis(maximum_ms),
        }
    }

    pub fn movetime(ms: u64) -> Self {
        Self { optimum: None, maximum: Duration::from_millis(ms) }
    }
}

struct Clock {
    start: Option<Instant>,
    finished: bool,
}

/// One controller per search: no deadline can cancel a later search. A ponder
/// search has no running clock until ponderhit; all budgets start at that hit.
pub struct SearchTime {
    budget: Option<Budget>,
    clock: Mutex<Clock>,
    changed: Notify,
}

impl SearchTime {
    pub fn new(budget: Option<Budget>, start: Option<Instant>) -> Self {
        Self { budget, clock: Mutex::new(Clock { start, finished: false }), changed: Notify::new() }
    }

    pub fn ponderhit(&self, now: Instant) {
        let mut clock = self.clock.lock().unwrap();
        if !clock.finished && clock.start.is_none() {
            clock.start = Some(now);
        }
        drop(clock);
        self.changed.notify_one();
    }

    pub fn soft_expired(&self, now: Instant) -> bool {
        let clock = self.clock.lock().unwrap();
        match (clock.start, self.budget.and_then(|b| b.optimum)) {
            (Some(start), Some(limit)) => now.saturating_duration_since(start) >= limit,
            _ => false,
        }
    }

    pub fn finish(&self) {
        self.clock.lock().unwrap().finished = true;
        self.changed.notify_one();
    }

    pub async fn enforce_hard_limit(&self, cancel: &AtomicBool) {
        loop {
            let deadline = {
                let clock = self.clock.lock().unwrap();
                if clock.finished { return; }
                clock.start.zip(self.budget).and_then(|(start, b)| start.checked_add(b.maximum))
            };
            match deadline {
                Some(deadline) => {
                    tokio::select! {
                        _ = self.changed.notified() => {},
                        _ = tokio::time::sleep_until(deadline.into()) => {
                            if !self.clock.lock().unwrap().finished {
                                cancel.store(true, Ordering::Relaxed);
                            }
                            return;
                        }
                    }
                }
                None => self.changed.notified().await,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_is_default() {
        assert_eq!(TimeMode::default(), TimeMode::Adaptive);
    }

    #[test]
    fn normal_linear_budget_is_unchanged() {
        for time in [100, 1000, 8000, 60_000, 300_000, 600_000] {
            for inc in [0, 80, 600, 2000] {
                let usable = time - 30;
                let old = (usable / 25 + inc * 3 / 4).clamp(50, (usable / 2).max(50));
                let b = Budget::clock(TimeMode::Linear, time, inc, 30, None);
                assert_eq!(b.maximum, Duration::from_millis(old));
                assert_eq!(b.optimum, None);
            }
        }
    }

    #[test]
    fn budgets_never_exceed_clock_even_with_large_increment() {
        for mode in [TimeMode::Linear, TimeMode::Adaptive] {
            for time in [0, 1, 29, 30, 31, 49, 50, 100, 1000, u64::MAX] {
                for inc in [0, 2000, u64::MAX] {
                    let b = Budget::clock(mode, time, inc, 30, None);
                    assert!(b.maximum <= Duration::from_millis(time.saturating_sub(30)));
                    assert!(b.optimum.unwrap_or_default() <= b.maximum);
                }
            }
        }
    }

    #[test]
    fn moves_to_go_and_explicit_movetime() {
        assert_eq!(Budget::clock(TimeMode::Linear, 10_030, 0, 30, Some(10)).maximum,
                   Duration::from_millis(1000));
        assert_eq!(Budget::clock(TimeMode::Linear, 10_030, 0, 30, Some(1)).maximum,
                   Duration::from_millis(10_000));
        assert_eq!(Budget::movetime(123).maximum, Duration::from_millis(123));
        assert_eq!(Budget::movetime(123).optimum, None);
    }

    #[test]
    fn adaptive_keeps_linear_allocation_as_its_soft_target() {
        let linear = Budget::clock(TimeMode::Linear, 300_000, 2000, 30, None);
        let adaptive = Budget::clock(TimeMode::Adaptive, 300_000, 2000, 30, None);
        assert_eq!(adaptive.optimum, Some(linear.maximum));
        assert_eq!(adaptive.maximum, linear.maximum * 3);
    }

    #[test]
    fn soft_limit_starts_at_ponderhit_not_ponder_start() {
        let budget = Budget::clock(TimeMode::Adaptive, 25_030, 0, 30, None);
        let time = SearchTime::new(Some(budget), None);
        let hit = Instant::now();
        assert!(!time.soft_expired(hit + Duration::from_secs(60)));
        time.ponderhit(hit);
        assert!(!time.soft_expired(hit + Duration::from_millis(999)));
        assert!(time.soft_expired(hit + Duration::from_millis(1000)));
        time.ponderhit(hit + Duration::from_secs(1)); // Duplicate hit cannot reset the clock.
        assert!(time.soft_expired(hit + Duration::from_millis(1000)));
    }

    #[tokio::test]
    async fn completed_timer_cannot_cancel_another_search() {
        let old_cancel = AtomicBool::new(false);
        let new_cancel = AtomicBool::new(false);
        let old = SearchTime::new(Some(Budget::movetime(0)), Some(Instant::now()));
        old.finish();
        old.enforce_hard_limit(&old_cancel).await;
        assert!(!old_cancel.load(Ordering::Relaxed));
        assert!(!new_cancel.load(Ordering::Relaxed));
        let new = SearchTime::new(Some(Budget::movetime(0)), Some(Instant::now()));
        new.enforce_hard_limit(&new_cancel).await;
        assert!(new_cancel.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn hard_timer_waits_for_ponderhit() {
        let cancel = AtomicBool::new(false);
        let time = SearchTime::new(Some(Budget::movetime(1)), None);
        assert!(tokio::time::timeout(Duration::from_millis(5),
            time.enforce_hard_limit(&cancel)).await.is_err());
        assert!(!cancel.load(Ordering::Relaxed));
        time.ponderhit(Instant::now() - Duration::from_millis(10));
        time.enforce_hard_limit(&cancel).await;
        assert!(cancel.load(Ordering::Relaxed));
    }
}
