//! How many Flame Altars each Flame level allows, up to 255.
//!
//! The platform lets the game group up to 255 altar zones into its own 16
//! bases; where it cannot, the table stays capped at the
//! game's own count. Other mods change the limit by joining the loader's
//! altar-limit chain with `on_altar_limit`.

use std::sync::Arc;

use arc_swap::ArcSwap;
use enshwick::abi::Side;
use enshwick::altars::base_limit;
use enshwick::{Load, Off};

use crate::hold;

/// The most altars the settings take: the most altar zones the platform
/// groups into bases. The loader keeps every answer within what the game
/// groups.
pub(crate) const CAP: u8 = u8::MAX;

/// The bases to ask for: the game's own. A world's own count waits for the
/// platform to take it as the world loads.
const BASES: u32 = 16;

/// The most altars the game's own grouping takes, from the loader; the
/// settings' cap when the loader does not say.
fn vanilla_cap() -> u8 {
    base_limit()
        .and_then(|limit| u8::try_from(limit.vanilla_zones).ok())
        .unwrap_or(CAP)
}

/// The altar zones to ask the platform for: [`CAP`], or less where the
/// loader takes less.
fn requested_cap() -> u8 {
    base_limit().map_or(CAP, |limit| u8::try_from(limit.max_zones).unwrap_or(CAP))
}

/// How many altars each Flame level allows.
#[enshwick::config(group)]
#[derive(Debug)]
pub(crate) struct Count {
    /// Altars this Flame level allows, from 1 to 255; the list starts at level
    /// 0, before the level 1 a new world starts at, and levels past it use
    /// its last entry.
    /// Vanilla: [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16]
    #[config(range = 1..=CAP, first = 0)]
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
    /// `cap`; empty, the game's own counts apply.
    pub(crate) fn new(levels: &[u16], cap: u8) -> Self {
        let table = levels
            .iter()
            .map(|&n| u8::try_from(n.min(u16::from(cap))).unwrap_or(cap))
            .collect();
        Self { table }
    }

    /// The table from `count`, warning about values above `cap`.
    fn from_settings(count: &Count, cap: u8) -> Self {
        if count
            .altars_per_flame_level
            .iter()
            .any(|&n| n > u16::from(cap))
        {
            enshwick::warn!(
                "altars above {cap} lowered to {cap}, the most altar zones the game groups into bases here"
            );
        }
        let table = Self::new(&count.altars_per_flame_level, cap);
        enshwick::info!("altars per Flame level: {:?}", table.table);
        table
    }

    /// Altars allowed at `flame_level`, where the game allows `vanilla`.
    pub(crate) fn altars(&self, flame_level: u32, vanilla: u8) -> u8 {
        let index = usize::try_from(flame_level).unwrap_or(usize::MAX);
        self.table
            .get(index)
            .or(self.table.last())
            .copied()
            .unwrap_or(vanilla)
    }
}

/// The running table, swapped whole when the settings or the server's join
/// values change, under the cap in force.
#[derive(Debug)]
pub(crate) struct AltarCount {
    table: ArcSwap<AltarTable>,
    /// The altar zones the platform groups once the request is in force,
    /// else the game's own grouping.
    cap: u8,
}

impl AltarCount {
    /// Asks for the altar zones the table takes in the game's bases, keeps
    /// the hold on a server and starts on the default table; the game's
    /// limit is linked by the mod.
    pub(crate) fn start(load: &mut Load<'_>) -> Result<Self, Off> {
        let server = load.host().build().side == Side::SERVER;
        let requested = requested_cap();
        let cap = if load.request_base_limits(u32::from(requested), BASES)? {
            if server {
                hold::keep(load)?;
            }
            requested
        } else {
            // A world held for its altars must not open with the vanilla
            // grouping: off, the loader stops the server for the hold.
            if server && let Some(reason) = hold::reason(load)? {
                enshwick::warn!("held: {reason}");
                return Err(Off::text(crate::texts::OFF_GROUPING, &[]));
            }
            let cap = vanilla_cap();
            enshwick::warn!("the game keeps {cap} altar zones in bases");
            cap
        };
        let table = AltarTable::from_settings(&Count::default(), cap);
        Ok(Self {
            table: ArcSwap::from_pointee(table),
            cap,
        })
    }

    /// The altars `flame_level` allows, where the game allows `vanilla`:
    /// the table's answer, between 1 and the cap. Other mods' filters in the
    /// loader's chain change it after.
    pub(crate) fn limit(&self, flame_level: u32, vanilla: u8) -> u8 {
        self.table
            .load()
            .altars(flame_level, vanilla)
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
        // The game's own counts on 1.0.
        let vanilla: [u8; 12] = [1, 2, 4, 6, 8, 10, 11, 12, 13, 14, 15, 16];
        let table = AltarTable::new(&Count::default().altars_per_flame_level, CAP);
        assert_eq!(table.altars(0, 1), 2);
        assert_eq!(table.altars(1, 2), 4);
        assert_eq!(table.altars(4, 8), 16);
        let doubled: Vec<u8> = vanilla.iter().map(|&n| n * 2).collect();
        assert_eq!(table.table, doubled);
        assert_eq!(table.altars(99, 16), 32);
    }

    #[test]
    fn values_are_capped_and_an_empty_table_answers_the_game_s_count() {
        let table = AltarTable::new(&[1, 3, 32, 255, 300], CAP);
        assert_eq!(
            [
                table.altars(0, 1),
                table.altars(1, 2),
                table.altars(2, 4),
                table.altars(3, 6),
                table.altars(4, 8),
            ],
            [1, 3, 32, 255, 255]
        );
        assert_eq!(AltarTable::new(&[], CAP).altars(0, 1), 1);
        let unlifted = AltarTable::new(&[1, 3, 32], 16);
        assert_eq!(unlifted.altars(2, 4), 16);
    }
}
