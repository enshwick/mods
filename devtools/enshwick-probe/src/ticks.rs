//! Ticks: an every-step handler, `Ticked` on its default period and a
//! handler on a period of its own, counted over a span, then the period
//! changed while the game runs and counted again.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use enshwick::events::{TickPeriod, Ticked};
use enshwick::schedule::Scheduler;
use enshwick::{Load, Off};

/// How long each count runs.
const SPAN: Duration = Duration::from_secs(5);
/// The period the probe's own handler starts on.
const EVERY: Duration = Duration::from_secs(1);
/// The period the probe sets on its handler while the game runs.
const FASTER: Duration = Duration::from_millis(250);

/// The calls each handler saw.
#[derive(Debug, Default)]
struct Calls {
    steps: AtomicU64,
    /// Every-step calls whose data says more than one step passed.
    skipped: AtomicU64,
    ticked: AtomicU64,
    periodic: AtomicU64,
    /// The steps the periodic handler's calls say passed, summed.
    periodic_steps: AtomicU64,
}

/// What one count saw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Count {
    steps: u64,
    skipped: u64,
    ticked: u64,
    periodic: u64,
    periodic_steps: u64,
}

impl Calls {
    fn count(&self) -> Count {
        Count {
            steps: self.steps.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            ticked: self.ticked.load(Ordering::Relaxed),
            periodic: self.periodic.load(Ordering::Relaxed),
            periodic_steps: self.periodic_steps.load(Ordering::Relaxed),
        }
    }
}

impl Count {
    fn since(self, before: Self) -> Self {
        Self {
            steps: self.steps - before.steps,
            skipped: self.skipped - before.skipped,
            ticked: self.ticked - before.ticked,
            periodic: self.periodic - before.periodic,
            periodic_steps: self.periodic_steps - before.periodic_steps,
        }
    }
}

/// Whether `calls` over `span` fit a handler run every `every`, within half
/// a period's worth either way of a step's lateness.
fn fits(calls: u64, span: Duration, every: Duration) -> bool {
    let expected = span.as_secs_f64() / every.as_secs_f64();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a few hundred calls at most over a span of seconds"
    )]
    let calls = calls as f64;
    (expected * 0.5..=expected * 1.5).contains(&calls)
}

/// The tick checks.
#[derive(Debug, Clone)]
pub(crate) struct TickChecks {
    calls: Arc<Calls>,
    period: TickPeriod,
    scheduler: Scheduler,
}

impl TickChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let calls = Arc::new(Calls::default());
        let seen = Arc::clone(&calls);
        load.on_every_step(move |tick| {
            seen.steps.fetch_add(1, Ordering::Relaxed);
            if tick.elapsed_steps != 1 {
                seen.skipped.fetch_add(1, Ordering::Relaxed);
            }
        })?;
        let seen = Arc::clone(&calls);
        load.on::<Ticked>(move |_| {
            seen.ticked.fetch_add(1, Ordering::Relaxed);
        })?;
        let seen = Arc::clone(&calls);
        let period = load.on_tick(EVERY, move |tick| {
            seen.periodic.fetch_add(1, Ordering::Relaxed);
            seen.periodic_steps
                .fetch_add(tick.elapsed_steps, Ordering::Relaxed);
        })?;
        Ok(Self {
            calls,
            period,
            scheduler,
        })
    }

    pub(crate) fn run(&self) {
        let checks = self.clone();
        let before = self.calls.count();
        self.scheduler.after(SPAN, move || {
            let seen = checks.calls.count().since(before);
            seen.report(EVERY);
            if let Err(off) = checks.period.set_every(FASTER) {
                enshwick::warn!("FAIL tick period change: {off}");
                return;
            }
            let before = checks.calls.count();
            let again = checks.clone();
            checks.scheduler.after(SPAN, move || {
                again.calls.count().since(before).report(FASTER);
                let _back = again.period.set_every(EVERY);
            });
        });
    }
}

impl Count {
    /// Logs whether each handler kept its period over the span.
    fn report(self, every: Duration) {
        let Count {
            steps,
            skipped,
            ticked,
            periodic,
            periodic_steps,
        } = self;
        if steps > 0 && skipped == 0 {
            enshwick::info!("PASS every step: {steps} calls in {SPAN:?}, one step each");
        } else {
            enshwick::warn!(
                "FAIL every step: {steps} calls in {SPAN:?}, {skipped} said more than one step"
            );
        }
        if ticked <= steps && fits(ticked, SPAN, Ticked::DEFAULT_EVERY) {
            enshwick::info!("PASS Ticked: {ticked} calls in {SPAN:?}");
        } else {
            enshwick::warn!(
                "FAIL Ticked: {ticked} calls in {SPAN:?} against {steps} steps, every {:?}",
                Ticked::DEFAULT_EVERY
            );
        }
        if fits(periodic, SPAN, every) {
            enshwick::info!(
                "PASS on_tick every {every:?}: {periodic} calls in {SPAN:?}, {periodic_steps} steps between"
            );
        } else {
            enshwick::warn!(
                "FAIL on_tick every {every:?}: {periodic} calls in {SPAN:?}, {periodic_steps} steps between"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_fits_its_period_within_half_either_way() {
        assert!(fits(50, SPAN, Ticked::DEFAULT_EVERY));
        assert!(fits(5, SPAN, EVERY));
        assert!(fits(3, SPAN, EVERY));
        assert!(!fits(2, SPAN, EVERY));
        assert!(!fits(8, SPAN, EVERY));
        assert!(!fits(100, SPAN, Ticked::DEFAULT_EVERY));
    }

    #[test]
    fn a_count_since_takes_the_earlier_one_away() {
        let before = Count {
            steps: 10,
            skipped: 0,
            ticked: 5,
            periodic: 1,
            periodic_steps: 9,
        };
        let now = Count {
            steps: 110,
            skipped: 1,
            ticked: 55,
            periodic: 6,
            periodic_steps: 109,
        };
        assert_eq!(
            now.since(before),
            Count {
                steps: 100,
                skipped: 1,
                ticked: 50,
                periodic: 5,
                periodic_steps: 100,
            }
        );
    }
}
