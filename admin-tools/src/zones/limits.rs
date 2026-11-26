//! What a protected zone takes from the players near it: the game's own
//! permission bits, taken through the platform while a player who is not a
//! zone builder stands in a zone or within its margin, so the game refuses
//! digging, breaking and chests there itself.

use std::sync::atomic::Ordering;

use enshwick::world::{Player, PlayerPermissions};
use enshwick::{PlayerKey, Relock as _, WorldError};

use super::Shape;
use crate::settings::Who;
use crate::{Tools, texts};

impl Shape {
    /// Whether `at` lies inside or within `margin` metres of it, measured on
    /// each axis for a box.
    fn near(&self, at: [f64; 3], margin: f64) -> bool {
        match *self {
            Self::Circle { x, z, radius } => Self::Circle {
                x,
                z,
                radius: radius + margin,
            },
            Self::Box { min, max } => Self::Box {
                min: min.map(|low| low - margin),
                max: max.map(|high| high + margin),
            },
        }
        .holds(at)
    }
}

/// The permission bits a player at `at` loses near `zones`, each a shape
/// and the bits it takes, within `margin` metres.
fn denied_at(zones: &[(Shape, u32)], at: [f64; 3], margin: f64) -> u32 {
    zones
        .iter()
        .filter(|(shape, _)| shape.near(at, margin))
        .fold(0, |bits, (_, denied)| bits | denied)
}

/// The permissions of `bits`, as the platform takes them.
fn listed(bits: u32) -> Vec<PlayerPermissions> {
    PlayerPermissions::ALL
        .iter()
        .filter(|permission| bits & permission.bit() != 0)
        .copied()
        .collect()
}

/// What the zones take from one player online.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Limit {
    key: PlayerKey,
    /// The permission bits the zones take from them.
    wanted: u32,
    /// Those of `wanted` their user group gave them, which the zone builders
    /// rule still counts.
    taken: u32,
}

impl Limit {
    const fn none(key: PlayerKey) -> Self {
        Self {
            key,
            wanted: 0,
            taken: 0,
        }
    }
}

/// The bits the zones took from the player with `key` that their group
/// gave them.
pub(super) fn taken_from(limited: &[Limit], key: PlayerKey) -> u32 {
    limited
        .iter()
        .find(|limit| limit.key == key)
        .map_or(0, |limit| limit.taken)
}

/// Whether `rule` lets `player` build in zones, judged by their user
/// group's permissions: with `taken`, what the zones took, given back.
pub(super) fn admitted(rule: Who, player: &Player, taken: u32) -> bool {
    if taken == 0 {
        return rule.admits(player);
    }
    let mut grouped = player.clone();
    grouped.permissions |= taken;
    rule.admits(&grouped)
}

impl Tools {
    /// Takes permissions from each player in `players` the zone builders
    /// rule does not name while they stand in or within the margin of an
    /// active zone that stops breaking or containers, gives them back as
    /// they leave it, and tells them on entering. Changes only what moved
    /// since the last pass; nothing while the zones do not read, after the
    /// world stopped, or once the platform refused to take permissions.
    pub(super) fn limit_in_zones(&self, players: &[Player]) {
        let Some(zones) = self.limiting.load_full() else {
            return;
        };
        if self.limits_off.load(Ordering::Relaxed) {
            return;
        }
        let settings = self.settings.load();
        let (rule, margin) = (settings.zone_rule(), f64::from(settings.zone_margin));
        let mut limited = self.limited.relock();
        for player in players {
            let at = limited.iter().position(|limit| limit.key == player.key);
            let Limit {
                wanted: had, taken, ..
            } = at.map_or(Limit::none(player.key), |at| limited[at]);
            let wanted = if admitted(rule, player, taken) {
                0
            } else {
                // A position that does not read keeps what the zones took.
                self.world
                    .position(player.entity)
                    .map_or(had, |at| denied_at(&zones, at, margin))
            };
            if wanted == had {
                continue;
            }
            match self
                .world
                .change_permissions(player.key, &listed(wanted), &[])
            {
                Ok(()) => {}
                // The player left between the list and the call.
                Err(WorldError::Refused) => continue,
                Err(why) => {
                    self.limits_off.store(true, Ordering::Relaxed);
                    enshwick::warn!("protected zones stop only building in this world: {why}");
                    return;
                }
            }
            let limit = Limit {
                key: player.key,
                wanted,
                taken: (taken & wanted) | (player.permissions & wanted & !had),
            };
            match at {
                Some(at) if wanted == 0 => drop(limited.swap_remove(at)),
                Some(at) => limited[at] = limit,
                None => limited.push(limit),
            }
            self.tell_limits(player, wanted & !had);
        }
    }

    /// Tells `player` what the zone they came near now stops for them, by
    /// the permission bits `taken` that they did not lose before.
    fn tell_limits(&self, player: &Player, taken: u32) {
        let breaking = PlayerPermissions::CanEditWorld.bit();
        let containers = PlayerPermissions::CanAccessInventories.bit();
        let language = self.world.language_of(player.key);
        for (bit, key) in [
            (breaking, texts::NOTICE_ZONE_BREAKING),
            (containers, texts::NOTICE_ZONE_CONTAINERS),
        ] {
            if taken & bit != 0 {
                let _shown = self
                    .world
                    .notify(player, &self.texts.fill(key, language, &[]));
            }
        }
    }

    /// Forgets what the zones took from the player with `key`, who left or
    /// joined: the platform ends a player's permission changes when they
    /// leave.
    pub(crate) fn forget_limits(&self, key: PlayerKey) {
        self.limited.relock().retain(|limit| limit.key != key);
    }
}

#[cfg(test)]
mod tests {
    use enshwick::Entity;

    use super::*;

    const MARKET: Shape = Shape::Circle {
        x: 10.0,
        z: -20.0,
        radius: 5.0,
    };

    #[test]
    fn a_player_is_near_a_zone_within_the_margin_of_its_edge_on_every_axis() {
        let hall = Shape::Box {
            min: [-5.0, 0.0, -5.0],
            max: [5.0, 4.0, 5.0],
        };
        let zones = [(MARKET, 1), (hall, 2)];
        assert_eq!(
            denied_at(&zones, [10.0, 0.0, -27.0], 2.0),
            1,
            "2 m past the circle"
        );
        assert_eq!(denied_at(&zones, [10.0, 0.0, -28.0], 2.0), 0);
        assert_eq!(denied_at(&zones, [0.0, 5.5, 0.0], 2.0), 2, "above the box");
        assert_eq!(denied_at(&zones, [0.0, 6.5, 0.0], 2.0), 0);
        assert_eq!(denied_at(&zones, [7.0, 0.0, -20.0], 0.0), 1, "inside");
    }

    #[test]
    fn the_zone_builders_rule_counts_what_a_zone_took_from_the_group() {
        let base = PlayerPermissions::CanEditBase;
        let player = Player::new(
            Entity::from_raw(11).unwrap(),
            1,
            PlayerKey::new(2),
            0,
            "Bob".into(),
        );
        let rule = Who::Permission(base);
        assert!(!admitted(rule, &player, 0));
        assert!(admitted(rule, &player, base.bit()), "the zone took it");
        assert!(!admitted(
            rule,
            &player,
            PlayerPermissions::CanEditWorld.bit()
        ));
    }

    #[test]
    fn the_platform_takes_the_permissions_a_bit_mask_names() {
        let bits = PlayerPermissions::CanEditWorld.bit() | PlayerPermissions::CanExtendBase.bit();
        assert_eq!(
            listed(bits),
            [
                PlayerPermissions::CanEditWorld,
                PlayerPermissions::CanExtendBase
            ]
        );
        assert!(listed(0).is_empty());
    }
}
