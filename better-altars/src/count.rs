//! How many Flame Altars each Flame level allows, up to 32 in 16 bases.
//!
//! The table is a join value: a client joined to a server runs with the
//! server's. The platform raises the game's grouping to 32 altar zones in its
//! 16 bases; where it cannot, the table stays capped at 16. Other mods change
//! the limit at the extension point `flame-altar-count:limit`, the name it had
//! before the mods merged.

use std::sync::Arc;

use arc_swap::ArcSwap;
use enshwick::abi::Side;
use enshwick::altars::{RAISED_ALTAR_ZONES, VANILLA_ALTAR_ZONES, VANILLA_ALTARS_PER_FLAME_LEVEL};
use enshwick::points::OwnedPoint;
use enshwick::{Load, Off};

use crate::hold;

/// The most altars with the base limit raised.
pub(crate) const CAP: u8 = RAISED_ALTAR_ZONES;
/// The most altars the vanilla grouping takes.
pub(crate) const VANILLA_CAP: u8 = VANILLA_ALTAR_ZONES;

/// How many Flame Altars a Flame level allows, `flame-altar-count:limit`:
/// the answer is the altars.
///
/// The mod answers from its table and keeps the result between 1 and
/// `cap`, links' answers too, so a link cannot raise a level past `cap`.
/// Asked whenever the game checks the limit, on the thread that checks it,
/// on the server and on every client: a linking mod belongs on both sides
/// with `join: true`, or the two disagree on the limit.
#[enshwick::point(name = "flame-altar-count:limit", mode = Override, output = u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AltarLimit {
    /// The Flame level, from 0.
    pub flame_level: u32,
    /// The most altars the game's grouping takes on this build.
    pub cap: u8,
}

/// How many altars each Flame level allows.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Count {
    /// Altars this Flame level allows, from 1 to 32; the list starts at level
    /// 0, before the level 1 a new world starts at, and levels past it use
    /// its last entry. All altars fit in 16 bases: altars whose zones touch
    /// share a base, and an altar that would start a 17th base is refused.
    /// Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
    /// A joined player runs with the server's table.
    #[config(sync, range = 1..=CAP, first = 0)]
    pub(crate) altars_per_flame_level: Vec<u16>,
}

impl Default for Count {
    fn default() -> Self {
        Self {
            altars_per_flame_level: vec![2, 4, 8, 12, 16, 20, 22, 24, 26, 28, 30, 32],
        }
    }
}

/// The table of altars per Flame level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AltarTable {
    table: Vec<u8>,
}

impl AltarTable {
    /// A table from the settings, already within 1 to [`CAP`]: lowered to
    /// `cap`, the vanilla table when empty.
    pub(crate) fn new(levels: &[u16], cap: u8) -> Self {
        let table = if levels.is_empty() {
            VANILLA_ALTARS_PER_FLAME_LEVEL.to_vec()
        } else {
            levels
                .iter()
                .map(|&n| u8::try_from(n.min(u16::from(cap))).unwrap_or(cap))
                .collect()
        };
        Self { table }
    }

    /// The table from `count`, warning about values above `cap`.
    fn from_settings(count: &Count, cap: u8) -> Self {
        if count
            .altars_per_flame_level
            .iter()
            .any(|&n| n > u16::from(cap))
        {
            enshwick::warn!("altars above {cap} lowered to {cap}: {}", cap_reason(cap));
        }
        let table = Self::new(&count.altars_per_flame_level, cap);
        enshwick::info!("altars per Flame level: {:?}", table.table);
        table
    }

    /// Altars allowed at `flame_level`.
    pub(crate) fn altars(&self, flame_level: u32) -> u8 {
        let index = usize::try_from(flame_level).unwrap_or(usize::MAX);
        self.table
            .get(index)
            .or(self.table.last())
            .copied()
            .unwrap_or(VANILLA_CAP)
    }
}

/// Why the table stops at `cap`.
fn cap_reason(cap: u8) -> &'static str {
    if cap == CAP {
        "a base names at most 32 altar zones"
    } else {
        "the game groups only 16 altar zones into bases here"
    }
}

/// The running table, swapped whole when the settings or the server's join
/// values change, under the cap in force.
#[derive(Debug)]
pub(crate) struct AltarCount {
    table: ArcSwap<AltarTable>,
    /// [`CAP`] once the base limit is lifted, else [`VANILLA_CAP`].
    cap: u8,
    /// The extension point `flame-altar-count:limit`.
    point: OwnedPoint<AltarLimit>,
}

impl AltarCount {
    /// Raises the base limit, keeps the hold on a server and defines the
    /// extension point; the game's limit is linked by the mod.
    pub(crate) fn start(load: &mut Load<'_>, count: &Count) -> Result<Self, Off> {
        let server = load.host().build().side == Side::SERVER;
        let cap = if load.raise_base_limit()? {
            if server {
                hold::keep(load)?;
            }
            CAP
        } else {
            // A world held for its altars must not open with the vanilla
            // grouping: off, the loader stops the server for the hold.
            if server && let Some(reason) = hold::reason(load)? {
                return Err(Off::new(format!(
                    "{reason}, and this build's grouping is not the one the platform knows"
                )));
            }
            enshwick::warn!("the game keeps 16 altar zones in bases");
            VANILLA_CAP
        };
        let table = AltarTable::from_settings(count, cap);
        let point = load.define_point()?;
        Ok(Self {
            table: ArcSwap::from_pointee(table),
            cap,
            point,
        })
    }

    /// The altars `flame_level` allows: the table's answer, or what other
    /// mods' links make of it, between 1 and the cap.
    pub(crate) fn limit(&self, flame_level: u32) -> u8 {
        let table = self.table.load().altars(flame_level);
        self.point
            .decide(
                &AltarLimit {
                    flame_level,
                    cap: self.cap,
                },
                table,
            )
            .value_or(table)
            .clamp(1, self.cap)
    }

    /// Takes the table of `count`, the settings now in force.
    pub(crate) fn apply(&self, count: &Count) {
        self.table
            .store(Arc::new(AltarTable::from_settings(count, self.cap)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_double_vanilla() {
        let table = AltarTable::new(&Count::default().altars_per_flame_level, CAP);
        assert_eq!(table.altars(0), 2);
        assert_eq!(table.altars(1), 4);
        assert_eq!(table.altars(4), 16);
        let doubled: Vec<u8> = VANILLA_ALTARS_PER_FLAME_LEVEL
            .iter()
            .map(|&n| n * 2)
            .collect();
        assert_eq!(table.table, doubled);
        assert_eq!(table.altars(99), 32);
    }

    #[test]
    fn values_are_capped_and_an_empty_table_is_vanilla() {
        let table = AltarTable::new(&[1, 3, 32], CAP);
        assert_eq!(
            [table.altars(0), table.altars(1), table.altars(2)],
            [1, 3, 32]
        );
        assert_eq!(AltarTable::new(&[], CAP).altars(0), 1);
        let unlifted = AltarTable::new(&[1, 3, 32], VANILLA_CAP);
        assert_eq!(unlifted.altars(2), 16);
    }
}
