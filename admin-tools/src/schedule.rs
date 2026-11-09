//! Announcements on a schedule and the countdown of a restart warning, as
//! plain state the mod's tick advances.

use core::time::Duration;

/// The minutes left at which a running restart warning speaks again.
const MARKS: [u32; 7] = [60, 30, 15, 10, 5, 2, 1];

/// What players read now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Say {
    /// The next announcement of the schedule.
    Announcement(String),
    /// The restart comes in this many minutes.
    RestartIn(u32),
    /// The restart is due.
    RestartNow,
}

/// The schedule's place and the restart warning under way, if any.
#[derive(Debug, Default)]
pub(crate) struct Ticker {
    since: Duration,
    next: usize,
    restart: Option<Countdown>,
}

/// A restart warning under way.
#[derive(Debug, Clone, Copy)]
struct Countdown {
    left: Duration,
    /// The minutes last said, so each mark speaks once.
    said: u32,
}

impl Ticker {
    /// Starts a warning of a restart in `minutes`, replacing one under way;
    /// its first notice is the caller's to send.
    pub(crate) fn warn(&mut self, minutes: u32) {
        self.restart = Some(Countdown {
            left: Duration::from_secs(u64::from(minutes) * 60),
            said: minutes,
        });
    }

    /// Stops the warning under way; whether there was one.
    pub(crate) fn cancel(&mut self) -> bool {
        self.restart.take().is_some()
    }

    /// Moves time on by `elapsed`: the next announcement once each `every`
    /// while `announcements` has any, and the restart warning at each mark
    /// it passes.
    pub(crate) fn step(
        &mut self,
        elapsed: Duration,
        announcements: &[String],
        every: Duration,
    ) -> Vec<Say> {
        let mut said = Vec::new();
        if announcements.is_empty() || every.is_zero() {
            self.since = Duration::ZERO;
        } else {
            self.since += elapsed;
            if self.since >= every {
                self.since = Duration::ZERO;
                said.push(Say::Announcement(
                    announcements[self.next % announcements.len()].clone(),
                ));
                self.next = self.next.wrapping_add(1);
            }
        }
        if let Some(countdown) = &mut self.restart {
            countdown.left = countdown.left.saturating_sub(elapsed);
            if countdown.left.is_zero() {
                said.push(Say::RestartNow);
                self.restart = None;
            } else if let Some(mark) = MARKS
                .into_iter()
                .filter(|&mark| {
                    mark < countdown.said
                        && countdown.left <= Duration::from_secs(u64::from(mark) * 60)
                })
                .min()
            {
                countdown.said = mark;
                said.push(Say::RestartIn(mark));
            }
        }
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    /// Steps `ticker` a second at a time for `seconds`, collecting what it says.
    fn run(
        ticker: &mut Ticker,
        seconds: u64,
        announcements: &[String],
        every: Duration,
    ) -> Vec<Say> {
        (0..seconds)
            .flat_map(|_| ticker.step(SECOND, announcements, every))
            .collect()
    }

    #[test]
    fn announcements_come_in_turn_once_each_interval() {
        let mut ticker = Ticker::default();
        let texts = ["a".to_owned(), "b".to_owned()];
        let said = run(&mut ticker, 180, &texts, Duration::from_secs(60));
        assert_eq!(
            said,
            [
                Say::Announcement("a".into()),
                Say::Announcement("b".into()),
                Say::Announcement("a".into())
            ]
        );
        assert!(run(&mut ticker, 600, &[], Duration::from_secs(60)).is_empty());
    }

    #[test]
    fn a_restart_warning_speaks_at_each_mark_below_its_start_then_once_at_the_end() {
        let mut ticker = Ticker::default();
        ticker.warn(10);
        let said = run(&mut ticker, 600, &[], Duration::ZERO);
        assert_eq!(
            said,
            [
                Say::RestartIn(5),
                Say::RestartIn(2),
                Say::RestartIn(1),
                Say::RestartNow
            ]
        );
        assert!(!ticker.cancel(), "the warning ended");
    }

    #[test]
    fn a_long_step_says_only_the_lowest_mark_it_passed() {
        let mut ticker = Ticker::default();
        ticker.warn(60);
        let said = ticker.step(Duration::from_mins(55), &[], Duration::ZERO);
        assert_eq!(said, [Say::RestartIn(5)]);
    }

    #[test]
    fn a_cancelled_warning_says_nothing_more() {
        let mut ticker = Ticker::default();
        ticker.warn(3);
        assert!(ticker.cancel());
        assert!(!ticker.cancel());
        assert!(run(&mut ticker, 300, &[], Duration::ZERO).is_empty());
    }
}
