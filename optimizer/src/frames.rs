//! Frame times in the game, for measure mode: the time between two frames of
//! the interface, so what a change does to frame pacing shows beside its CPU.
//!
//! The thread that draws records each frame into a histogram of fixed
//! buckets with a few relaxed atomics; the measuring thread takes it once a
//! period.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Instant;

use enshwick::state::ModLocal;

/// The width of a bucket.
const BUCKET_NS: u64 = 250_000;
/// How many buckets: frames up to 64 ms each get one, longer ones the last.
const BUCKETS: usize = 256;

/// The frames since the measuring thread last took them.
#[derive(Debug)]
struct Frames {
    /// The clock the frames are timed on.
    epoch: Instant,
    /// When the last frame came, ns after `epoch`; 0 before the first.
    last_ns: AtomicU64,
    /// The time of all frames taken together.
    sum_ns: AtomicU64,
    /// The longest frame.
    worst_ns: AtomicU64,
    /// Frames by length, [`BUCKET_NS`] a bucket.
    counts: [AtomicU32; BUCKETS],
}

/// The frames of this mod life.
static FRAMES: ModLocal<Frames> = ModLocal::new();

/// The frames of this mod life, none at first.
fn frames() -> &'static Frames {
    FRAMES.get_or_init(|| Frames {
        epoch: Instant::now(),
        last_ns: AtomicU64::new(0),
        sum_ns: AtomicU64::new(0),
        worst_ns: AtomicU64::new(0),
        counts: core::array::from_fn(|_| AtomicU32::new(0)),
    })
}

/// Records a frame of the interface; on the thread that draws, once a frame.
pub(crate) fn frame() {
    let frames = frames();
    let now = u64::try_from(frames.epoch.elapsed().as_nanos())
        .unwrap_or(u64::MAX)
        .max(1);
    let last = frames.last_ns.swap(now, Ordering::Relaxed);
    if last != 0 {
        record(frames, now.saturating_sub(last));
    }
}

/// Counts a frame of `length` ns.
fn record(frames: &Frames, length: u64) {
    let bucket = usize::try_from(length / BUCKET_NS).map_or(BUCKETS - 1, |b| b.min(BUCKETS - 1));
    let _count = frames.counts[bucket].fetch_add(1, Ordering::Relaxed);
    let _sum = frames.sum_ns.fetch_add(length, Ordering::Relaxed);
    let _worst = frames.worst_ns.fetch_max(length, Ordering::Relaxed);
}

/// The frames of one period.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Times {
    /// How many frames.
    pub(crate) count: u64,
    /// Their mean length, in ns.
    pub(crate) mean_ns: u64,
    /// The length 99 of 100 frames stay within, to a bucket, in ns.
    pub(crate) p99_ns: u64,
    /// The longest frame, in ns.
    pub(crate) worst_ns: u64,
}

/// The frames since the last call, and a fresh start; a frame recorded while
/// this runs counts in this period or the next.
pub(crate) fn take() -> Times {
    let frames = frames();
    let counts: [u32; BUCKETS] =
        core::array::from_fn(|i| frames.counts[i].swap(0, Ordering::Relaxed));
    let sum_ns = frames.sum_ns.swap(0, Ordering::Relaxed);
    let worst_ns = frames.worst_ns.swap(0, Ordering::Relaxed);
    times(&counts, sum_ns, worst_ns)
}

/// The times of a histogram `counts` whose frames took `sum_ns` together
/// and `worst_ns` at most.
fn times(counts: &[u32; BUCKETS], sum_ns: u64, worst_ns: u64) -> Times {
    let count: u64 = counts.iter().map(|&c| u64::from(c)).sum();
    if count == 0 {
        return Times::default();
    }
    let within = count - count / 100;
    let (mut seen, mut end_ns, mut p99_ns) = (0, 0, worst_ns);
    for (bucket, &c) in counts.iter().enumerate() {
        seen += u64::from(c);
        end_ns += BUCKET_NS;
        if seen >= within {
            // The last bucket holds every longer frame: the worst bounds it.
            if bucket < BUCKETS - 1 {
                p99_ns = end_ns.min(worst_ns);
            }
            break;
        }
    }
    Times {
        count,
        mean_ns: sum_ns / count,
        p99_ns,
        worst_ns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn histogram(frames: &[(u64, u32)]) -> ([u32; BUCKETS], u64, u64) {
        let mut counts = [0; BUCKETS];
        let (mut sum, mut worst) = (0, 0);
        for &(ns, n) in frames {
            let bucket = usize::try_from(ns / BUCKET_NS).unwrap().min(BUCKETS - 1);
            counts[bucket] += n;
            sum += ns * u64::from(n);
            worst = worst.max(ns);
        }
        (counts, sum, worst)
    }

    #[test]
    fn steady_frames_have_their_length_as_mean_and_as_99th_percentile() {
        let (counts, sum, worst) = histogram(&[(16_600_000, 3_600)]);
        let found = times(&counts, sum, worst);
        assert_eq!(found.count, 3_600);
        assert_eq!(found.mean_ns, 16_600_000);
        assert_eq!(found.p99_ns, 16_600_000, "bounded by the worst frame");
        assert_eq!(found.worst_ns, 16_600_000);
    }

    #[test]
    fn a_stutter_in_more_than_one_frame_of_a_hundred_moves_the_99th_percentile() {
        let few = histogram(&[(7_000_000, 990), (40_000_000, 10)]);
        assert_eq!(
            times(&few.0, few.1, few.2).p99_ns,
            7_250_000,
            "the end of the frame's bucket"
        );
        let many = histogram(&[(7_000_000, 980), (40_000_000, 20)]);
        let found = times(&many.0, many.1, many.2);
        assert_eq!(found.p99_ns, 40_000_000);
        assert_eq!(found.worst_ns, 40_000_000);
    }

    #[test]
    fn frames_longer_than_the_buckets_count_as_the_worst() {
        let (counts, sum, worst) = histogram(&[(10_000_000, 50), (500_000_000, 50)]);
        assert_eq!(times(&counts, sum, worst).p99_ns, 500_000_000);
    }

    #[test]
    fn no_frames_give_no_times() {
        assert_eq!(times(&[0; BUCKETS], 0, 0), Times::default());
    }

    #[test]
    fn recorded_frames_are_taken_once() {
        frame();
        let _old = take();
        std::thread::sleep(std::time::Duration::from_millis(2));
        frame();
        frame();
        let found = take();
        assert_eq!(found.count, 2);
        assert!(found.worst_ns >= 2_000_000);
        assert_eq!(take().count, 0);
    }

    #[test]
    fn each_frame_lands_in_the_bucket_of_its_length_and_long_ones_in_the_last() {
        let frames = frames();
        let _old = take();
        for _ in 0..98 {
            record(frames, 1_000_000);
        }
        record(frames, 10_000_000);
        record(frames, 100_000_000);
        let found = take();
        assert_eq!(found.count, 100);
        assert_eq!(
            found.p99_ns, 10_250_000,
            "the end of the 10 ms frame's bucket"
        );
        assert_eq!(found.worst_ns, 100_000_000);
        assert_eq!(found.mean_ns, 2_080_000);
    }
}
