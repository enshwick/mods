//! Frame times in the game, for measure mode: the time between two frames of
//! the interface, so what a change does to frame pacing shows beside its CPU.
//!
//! The thread that draws records each frame into a histogram of fixed
//! buckets with a few relaxed atomics; the measuring thread takes it once a
//! period.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use enshwick::state::ModLocal;

use crate::{measure, waits};

/// The width of a bucket.
const BUCKET_NS: u64 = 250_000;
/// How many buckets: a frame shorter than 256 ms gets its own, longer ones
/// share the last.
const BUCKETS: usize = 1_024;

/// The frames since the measuring thread last took them.
#[derive(Debug)]
struct Frames {
    /// When the last frame came, on the clock of [`waits::now_ns`], never 0;
    /// 0 before the first.
    last_ns: AtomicU64,
    /// The longest frame.
    worst_ns: AtomicU64,
    /// Frames by length, [`BUCKET_NS`] a bucket.
    counts: [AtomicU32; BUCKETS],
}

impl Frames {
    fn new() -> Self {
        Self {
            last_ns: AtomicU64::new(0),
            worst_ns: AtomicU64::new(0),
            counts: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }

    /// Records the frame that came at `now_ns`, never 0.
    fn frame_at(&self, now_ns: u64) {
        let last = self.last_ns.swap(now_ns, Ordering::Relaxed);
        if last != 0 {
            self.record(now_ns.saturating_sub(last));
        }
    }

    /// Counts a frame of `length` ns.
    fn record(&self, length: u64) {
        let bucket =
            usize::try_from(length / BUCKET_NS).map_or(BUCKETS - 1, |b| b.min(BUCKETS - 1));
        let _count = self.counts[bucket].fetch_add(1, Ordering::Relaxed);
        let _worst = self.worst_ns.fetch_max(length, Ordering::Relaxed);
    }

    /// The frames since the last call, and a fresh start.
    fn take(&self) -> Times {
        let counts: [u32; BUCKETS] =
            core::array::from_fn(|i| self.counts[i].swap(0, Ordering::Relaxed));
        times(&counts, self.worst_ns.swap(0, Ordering::Relaxed))
    }
}

/// The frames of this mod life.
static FRAMES: ModLocal<Frames> = ModLocal::new();

/// The frames of this mod life, none at first.
fn frames() -> &'static Frames {
    FRAMES.get_or_init(Frames::new)
}

/// Records a frame of the interface while measure mode is on; on the thread
/// that draws, once a frame.
pub(crate) fn frame() {
    let frames = frames();
    if !measure::on() {
        // The first frame after measure mode turns on starts a fresh interval.
        frames.last_ns.store(0, Ordering::Relaxed);
        return;
    }
    if let Some(now) = waits::now_ns() {
        frames.frame_at(now);
    }
}

/// The frames of one period.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Times {
    /// How many frames.
    pub(crate) count: u64,
    /// The length 99 of 100 frames stay within, to a bucket, in ns.
    pub(crate) p99_ns: u64,
    /// The longest frame, in ns.
    pub(crate) worst_ns: u64,
}

/// The frames since the last call, and a fresh start; a frame recorded while
/// this runs counts in this period or the next.
pub(crate) fn take() -> Times {
    frames().take()
}

/// The times of a histogram `counts` whose longest frame took `worst_ns`.
///
/// A frame recorded during a take may land its count in one period and its
/// length in the other, so the worst is never below the longest bucket.
fn times(counts: &[u32; BUCKETS], worst_ns: u64) -> Times {
    let count: u64 = counts.iter().map(|&c| u64::from(c)).sum();
    if count == 0 {
        return Times::default();
    }
    let within = count - count / 100;
    let (mut seen, mut floor_ns, mut longest_ns, mut p99_end) = (0, 0, 0, None);
    for (bucket, &c) in counts.iter().enumerate() {
        if c > 0 {
            longest_ns = floor_ns;
        }
        seen += u64::from(c);
        if p99_end.is_none() && seen >= within {
            // The last bucket holds every longer frame: the worst bounds it.
            p99_end = Some(if bucket < BUCKETS - 1 {
                floor_ns + BUCKET_NS
            } else {
                u64::MAX
            });
        }
        floor_ns += BUCKET_NS;
    }
    let worst_ns = worst_ns.max(longest_ns);
    Times {
        count,
        p99_ns: p99_end.map_or(worst_ns, |end| end.min(worst_ns)),
        worst_ns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram(frames: &[(u64, u32)]) -> ([u32; BUCKETS], u64) {
        let mut counts = [0; BUCKETS];
        let mut worst = 0;
        for &(ns, n) in frames {
            let bucket = usize::try_from(ns / BUCKET_NS).unwrap().min(BUCKETS - 1);
            counts[bucket] += n;
            worst = worst.max(ns);
        }
        (counts, worst)
    }

    #[test]
    fn steady_frames_have_their_length_as_99th_percentile() {
        let (counts, worst) = histogram(&[(16_600_000, 3_600)]);
        let found = times(&counts, worst);
        assert_eq!(found.count, 3_600);
        assert_eq!(found.p99_ns, 16_600_000, "bounded by the worst frame");
        assert_eq!(found.worst_ns, 16_600_000);
    }

    #[test]
    fn a_stutter_in_more_than_one_frame_of_a_hundred_moves_the_99th_percentile() {
        let few = histogram(&[(7_000_000, 990), (40_000_000, 10)]);
        assert_eq!(
            times(&few.0, few.1).p99_ns,
            7_250_000,
            "the end of the frame's bucket"
        );
        let many = histogram(&[(7_000_000, 980), (40_000_000, 20)]);
        let found = times(&many.0, many.1);
        assert_eq!(found.p99_ns, 40_000_000);
        assert_eq!(found.worst_ns, 40_000_000);
    }

    #[test]
    fn slow_frames_and_one_long_load_keep_the_99th_percentile_at_the_slow_frames() {
        let (counts, worst) =
            histogram(&[(30_000_000, 980), (80_000_000, 20), (15_000_000_000, 1)]);
        let found = times(&counts, worst);
        assert_eq!(found.p99_ns, 80_250_000, "not the 15 s frame");
        assert_eq!(found.worst_ns, 15_000_000_000);
    }

    #[test]
    fn frames_longer_than_the_buckets_count_as_the_worst() {
        let (counts, worst) = histogram(&[(10_000_000, 50), (500_000_000, 50)]);
        assert_eq!(times(&counts, worst).p99_ns, 500_000_000);
    }

    #[test]
    fn a_frame_counted_without_its_length_still_bounds_the_worst() {
        let (counts, _) = histogram(&[(40_000_000, 1)]);
        let found = times(&counts, 0);
        assert_eq!(found.worst_ns, 40_000_000, "the floor of its bucket");
        assert_eq!(found.p99_ns, 40_000_000);
    }

    #[test]
    fn no_frames_give_no_times() {
        assert_eq!(times(&[0; BUCKETS], 0), Times::default());
    }

    #[test]
    fn recorded_frames_are_taken_once() {
        let frames = Frames::new();
        frames.frame_at(1_000_000);
        let first = frames.take();
        assert_eq!(first.count, 0, "the first frame only starts the clock");
        frames.frame_at(3_000_000);
        frames.frame_at(3_500_000);
        let found = frames.take();
        assert_eq!(found.count, 2);
        assert_eq!(found.worst_ns, 2_000_000);
        assert_eq!(frames.take().count, 0);
    }

    #[test]
    fn each_frame_lands_in_the_bucket_of_its_length_and_long_ones_in_the_last() {
        let frames = Frames::new();
        for _ in 0..98 {
            frames.record(1_000_000);
        }
        frames.record(10_000_000);
        frames.record(1_000_000_000);
        let found = frames.take();
        assert_eq!(found.count, 100);
        assert_eq!(
            found.p99_ns, 10_250_000,
            "the end of the 10 ms frame's bucket"
        );
        assert_eq!(found.worst_ns, 1_000_000_000);
    }
}
