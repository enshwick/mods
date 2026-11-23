//! What the server sends a player's client: the marks nearest the player,
//! and how they are picked.

use core::cmp::Ordering;

use enshwick::ui::MAX_MARKERS;

/// What a mark shows.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub(crate) enum Kind {
    /// A loot chest the world comes with.
    Chest,
    /// A plant or another node that holds what it yields.
    Resource,
}

/// One mark: its kind and where it is, in metres, y up.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Mark {
    pub(crate) kind: Kind,
    pub(crate) at: [f32; 3],
}

/// The marks of `found` within `radius` metres of `around` across the
/// ground, the nearest [`MAX_MARKERS`] of them, in an order that depends
/// only on the set, so an unchanged set compares equal.
pub(crate) fn nearest(
    found: impl IntoIterator<Item = (Kind, [f64; 3])>,
    around: [f64; 3],
    radius: f64,
) -> Vec<Mark> {
    let mut near: Vec<(f64, Kind, [f64; 3])> = found
        .into_iter()
        .map(|(kind, at)| (ground_distance(at, around), kind, at))
        .filter(|&(distance, _, _)| distance <= radius)
        .collect();
    if near.len() > MAX_MARKERS {
        // Ties at the cut-off fall by kind and place, so the kept set does
        // not hang on the order the world lists them in.
        near.select_nth_unstable_by(MAX_MARKERS - 1, |a, b| {
            a.0.total_cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| lexical(&a.2, &b.2, f64::total_cmp))
        });
        near.truncate(MAX_MARKERS);
    }
    let mut marks: Vec<Mark> = near
        .into_iter()
        .map(|(_, kind, at)| Mark {
            kind,
            at: at.map(narrow),
        })
        .collect();
    marks.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| lexical(&a.at, &b.at, f32::total_cmp))
    });
    marks
}

/// `a` against `b` axis by axis, the first axis that differs deciding.
fn lexical<T>(a: &[T; 3], b: &[T; 3], cmp: fn(&T, &T) -> Ordering) -> Ordering {
    a.iter()
        .zip(b)
        .map(|(x, y)| cmp(x, y))
        .find(|order| order.is_ne())
        .unwrap_or(Ordering::Equal)
}

/// The distance between `a` and `b` across the ground, leaving out height,
/// as the map shows it.
fn ground_distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "a world position in metres keeps about a millimetre in f32 across the 10 km map"
)]
const fn narrow(metres: f64) -> f32 {
    metres as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_beyond_the_radius_across_the_ground_are_left_out() {
        let found = [
            (Kind::Chest, [3.0, 0.0, 4.0]),
            (Kind::Resource, [6.0, 0.0, 8.0]),
        ];
        let marks = nearest(found, [0.0; 3], 5.0);
        assert_eq!(
            marks,
            [Mark {
                kind: Kind::Chest,
                at: [3.0, 0.0, 4.0]
            }]
        );
    }

    #[test]
    fn height_does_not_count_toward_the_radius() {
        let marks = nearest([(Kind::Chest, [1.0, 500.0, 0.0])], [0.0; 3], 2.0);
        assert_eq!(marks.len(), 1);
    }

    #[test]
    fn only_the_nearest_marks_up_to_the_limit_are_kept() {
        let far = (0..MAX_MARKERS).map(|i| {
            (
                Kind::Resource,
                [100.0 + f64::from(u32::try_from(i).unwrap()), 0.0, 0.0],
            )
        });
        let close = (0..10).map(|i| (Kind::Chest, [f64::from(i), 0.0, 0.0]));
        let marks = nearest(far.chain(close), [0.0; 3], 1000.0);
        assert_eq!(marks.len(), MAX_MARKERS);
        assert_eq!(marks.iter().filter(|m| m.kind == Kind::Chest).count(), 10);
        let farthest_kept = 100.0 + f64::from(u32::try_from(MAX_MARKERS - 10).unwrap());
        assert!(marks.iter().all(|m| f64::from(m.at[0]) < farthest_kept));
    }

    #[test]
    fn marks_tied_at_the_cut_off_are_kept_the_same_whatever_order_they_come_in() {
        let inside = (1..MAX_MARKERS).map(|i| {
            (
                Kind::Chest,
                [f64::from(u32::try_from(i).unwrap()), 0.0, 0.0],
            )
        });
        let tied = [
            (Kind::Resource, [0.0, 0.0, 900.0]),
            (Kind::Chest, [900.0, 0.0, 0.0]),
            (Kind::Chest, [0.0, 0.0, -900.0]),
            (Kind::Resource, [-900.0, 0.0, 0.0]),
        ];
        let forward: Vec<_> = inside.clone().chain(tied).collect();
        let mut backward = forward.clone();
        backward.reverse();
        let mut tied_first: Vec<_> = tied.into_iter().chain(inside).collect();
        tied_first.rotate_left(1);
        let kept = nearest(forward, [0.0; 3], 1000.0);
        assert_eq!(kept.len(), MAX_MARKERS);
        assert_eq!(kept, nearest(backward, [0.0; 3], 1000.0));
        assert_eq!(kept, nearest(tied_first, [0.0; 3], 1000.0));
    }

    #[test]
    fn the_same_set_in_another_order_gives_equal_marks() {
        let a = [
            (Kind::Resource, [2.0, 0.0, 0.0]),
            (Kind::Chest, [1.0, 0.0, 0.0]),
            (Kind::Chest, [0.0, 0.0, 1.0]),
        ];
        let mut b = a;
        b.reverse();
        assert_eq!(
            nearest(a, [0.0; 3], 10.0),
            nearest(b, [5.0, 0.0, 0.0], 10.0)
        );
    }
}
