//! More comfort from furniture inside a base.
//!
//! Each tick every comfort provider in a base registers its comfort for its
//! category, and the base keeps the best per category; a player's comfort is
//! the sum. The mod scales what each provider registers, on the server, which
//! decides comfort.

use core::sync::atomic::{AtomicU32, Ordering};

/// The largest comfort factor: the game sums the comfort of every category
/// into one number, which a larger factor could overflow.
pub(crate) const MAX_FACTOR: f32 = 100.0;

/// The comfort factor in force, read on every registration.
#[derive(Debug)]
pub(crate) struct ComfortRate {
    /// The factor as `f32` bits.
    factor: AtomicU32,
}

/// The one comfort rate.
pub(crate) static RATE: ComfortRate = ComfortRate {
    factor: AtomicU32::new(0x3f80_0000),
};

impl ComfortRate {
    /// Puts `factor` in force.
    pub(crate) fn set(&self, factor: f32) {
        self.factor.store(factor.to_bits(), Ordering::Relaxed);
    }

    /// The comfort a provider registers instead of `comfort`.
    pub(crate) fn scale(&self, comfort: i32) -> i32 {
        scaled(comfort, f32::from_bits(self.factor.load(Ordering::Relaxed)))
    }
}

/// `comfort` scaled by `factor`, rounded and kept in range.
#[expect(
    clippy::cast_possible_truncation,
    reason = "clamped to the i32 range first"
)]
fn scaled(comfort: i32, factor: f32) -> i32 {
    let value = (f64::from(comfort) * f64::from(factor)).round();
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comfort_scales_rounds_and_stays_in_range() {
        assert_eq!(scaled(7, 2.0), 14);
        assert_eq!(scaled(3, 1.5), 5);
        assert_eq!(scaled(i32::MAX, 2.0), i32::MAX);
        assert_eq!(scaled(5, 0.0), 0);
    }

    #[test]
    fn the_rate_in_force_scales() {
        let rate = ComfortRate {
            factor: AtomicU32::new(0),
        };
        rate.set(MAX_FACTOR);
        assert_eq!(rate.scale(3), 300);
    }
}
