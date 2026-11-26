//! Stats: a kind of the probe's own and built-in ones opened, modifiers set
//! on an entity with health, its values and maximum health read back after
//! a step, and the modifiers removed again; then a buff's modifier read on a
//! character, and on a watched entity that is no player.

use std::time::Duration;

use enshwick::Entity;
use enshwick::buffs::{Apply, BuffTypeId, Buffs};
use enshwick::schedule::Scheduler;
use enshwick::stats::{Lasting, Modifier, Op, Stat, StatDef, Stats, builtin};
use enshwick::world::{ContainerKinds, World};
use enshwick::{Load, Off};

/// How long the probe waits for the values to follow.
const SETTLE: Duration = Duration::from_secs(1);
/// The extra maximum health the probe gives.
const EXTRA: i16 = 100;
/// The stat the probe defines.
const LUCK: enshwick::content::Id = enshwick::content::id!("luck");
/// The probe's key for its modifiers.
const KEY: &str = "probe";
/// The buff the probe gives luck to: the Food slot's hunger debuff, whose
/// id the game's code returns at 0x140059190 (server 26430253).
const HUNGER: BuffTypeId = BuffTypeId(0x89cb_0b34);
/// The luck the probe's modifier adds.
const LUCK_ADDED: f32 = 0.25;
/// The luck the hunger debuff gives while it holds.
const BUFF_LUCK: f32 = 0.5;
/// When the buff checks start: after the modifier checks put luck back.
const BUFFS_AFTER: Duration = Duration::from_secs(3);
/// How long the probe's hunger debuff lasts, should its removal fail: past
/// the reads it takes.
const BUFF_LASTS: Duration = Duration::from_secs(3);

/// Whether luck read `under` the buff is the buff's [`BUFF_LUCK`] above the
/// luck read `before` it.
fn buff_shows(before: f32, under: f32) -> bool {
    (under - before - BUFF_LUCK).abs() < 1e-5
}

/// The stats checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct StatChecks {
    stats: Stats,
    buffs: Buffs,
    world: World,
    scheduler: Scheduler,
    luck: Stat,
    health_max: Stat,
    fire: Stat,
    /// Whether the hunger debuff carries the probe's luck.
    buffed: bool,
}

impl StatChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let world = load.world()?;
        let luck = load.define_stat(LUCK, &StatDef::additive().range(0.0, 1.0))?;
        load.on_stats_changed(|change| {
            enshwick::info!(
                "stats: {:?} stat {} {} -> {}",
                change.entity,
                change.stat.index(),
                change.old,
                change.new
            );
        })?;
        let buffed = load
            .add_buff_modifiers(HUNGER, &[Modifier::new(luck, Op::Add, BUFF_LUCK)])
            .inspect_err(|why| enshwick::warn!("FAIL stat buff: modifiers for {HUNGER:?}: {why}"))
            .is_ok();
        Ok(Self {
            stats: load.stats()?,
            buffs: load.buffs(),
            world,
            scheduler,
            luck,
            health_max: load.open_stat(builtin::HEALTH_MAX)?,
            fire: load.open_stat(builtin::RESISTANCE_FIRE)?,
            buffed,
        })
    }

    pub(crate) fn run(self) {
        if self.buffed {
            self.scheduler
                .after(BUFFS_AFTER, move || self.buff_checks());
        }
        match self.stats.info(self.fire) {
            Some(info) if info.id == builtin::RESISTANCE_FIRE.to_string() => {
                enshwick::info!("PASS stats: built-in kind {info:?}");
            }
            other => enshwick::warn!("FAIL stats: built-in kind reads {other:?}"),
        }
        let Some(target) = crate::living(self.world)
            .into_iter()
            .find(|&e| self.world.vitals(e).is_some_and(|v| v.health.current > 1))
        else {
            enshwick::info!("SKIP stats: no living entity with health");
            return;
        };
        let before = self.world.vitals(target).map(|v| v.health.max);
        let luck = self.stats.value(target, self.luck);
        let set = [
            Modifier::new(self.luck, Op::Add, LUCK_ADDED),
            Modifier::new(self.health_max, Op::Add, f32::from(EXTRA)),
        ]
        .into_iter()
        .try_for_each(|m| self.stats.set(target, KEY, m, Lasting::UntilRemoved));
        if let Err(why) = set {
            enshwick::warn!("FAIL stats: modifiers on {target:?}: {why}");
            return;
        }
        self.scheduler
            .after(SETTLE, move || self.raised(target, before, luck));
    }

    /// Reads `target`'s values under the modifiers, against its maximum
    /// health and luck `before` them, which a buff such as hunger may hold.
    fn raised(self, target: Entity, before: Option<i32>, luck_before: f32) {
        let luck = self.stats.value(target, self.luck);
        if (luck - luck_before - LUCK_ADDED).abs() < 1e-5 {
            enshwick::info!("PASS stats: luck of {target:?} {luck_before} -> {luck}");
        } else {
            enshwick::warn!("FAIL stats: luck of {target:?} {luck_before} -> {luck}");
        }
        let sources = self.stats.sources(target, None);
        enshwick::info!("stats: sources of {target:?}: {sources:?}");
        let after = self.world.vitals(target).map(|v| v.health.max);
        match (before, after) {
            (Some(b), Some(a)) if a == b + i32::from(EXTRA) => {
                enshwick::info!("PASS stats: maximum health of {target:?} {b} -> {a}");
            }
            other => {
                enshwick::warn!("FAIL stats: maximum health of {target:?}: {other:?}");
            }
        }
        let removed = [self.luck, self.health_max]
            .into_iter()
            .try_for_each(|stat| self.stats.remove(target, KEY, stat));
        if let Err(why) = removed {
            enshwick::warn!("FAIL stats: removal on {target:?}: {why}");
            return;
        }
        self.scheduler.after(SETTLE, move || {
            let back = self.world.vitals(target).map(|v| v.health.max);
            if back == before {
                enshwick::info!("PASS stats: maximum health of {target:?} back to {back:?}");
            } else {
                enshwick::warn!(
                    "FAIL stats: maximum health of {target:?} {back:?}, was {before:?}"
                );
            }
        });
    }

    /// Reads the buff's luck on a player's character, which is always
    /// watched, and on a watched entity that is no player.
    fn buff_checks(self) {
        match crate::checks::listed(
            "containers",
            self.world.containers(ContainerKinds::CHARACTERS),
        )
        .first()
        {
            Some(character) => self.under_buff("stat buff", character.entity, false),
            None => enshwick::info!("SKIP stat buff: no character in the world"),
        }
        let other = crate::checks::listed(
            "containers",
            self.world
                .containers(ContainerKinds::NPCS | ContainerKinds::ANIMALS),
        )
        .into_iter()
        .map(|placed| placed.entity)
        .find(|&entity| self.buffs.of(entity).is_ok());
        match other {
            Some(entity) => self.under_buff("stat watch", entity, true),
            None => enshwick::info!("SKIP stat watch: no survivor or animal holds buffs"),
        }
    }

    /// Gives `target` the hunger debuff for a moment and reads the luck its
    /// modifier adds; with `watch`, it first reads that the buff does not
    /// count while `target` is unwatched, then watches it.
    fn under_buff(self, what: &'static str, target: Entity, watch: bool) {
        match self.buffs.has(target, HUNGER) {
            Ok(false) => {}
            Ok(true) => {
                enshwick::info!("SKIP {what}: {target:?} has {HUNGER:?} already");
                return;
            }
            Err(why) => {
                crate::checks::fail(&format!("{what}: buffs of {target:?} not read: {why}"));
                return;
            }
        }
        let before = self.stats.value(target, self.luck);
        if let Err(why) = self
            .buffs
            .apply(target, HUNGER, Apply::new().lasting(BUFF_LASTS))
        {
            enshwick::warn!("FAIL {what}: {HUNGER:?} on {target:?}: {why}");
            return;
        }
        if !watch {
            self.scheduler
                .after(SETTLE, move || self.read_buff(what, target, before, false));
            return;
        }
        self.scheduler.after(SETTLE, move || {
            let unwatched = self.stats.value(target, self.luck);
            if buff_shows(before, unwatched) {
                enshwick::info!(
                    "SKIP {what}: {target:?} counts {HUNGER:?} unwatched, so something else watches it"
                );
                self.lift_buff(what, target);
                return;
            }
            self.stats.watch(target, true);
            self.scheduler
                .after(SETTLE, move || self.read_buff(what, target, before, true));
        });
    }

    /// Reads `target`'s luck under the buff against the luck `before` it,
    /// then lifts the buff and the watch the probe set.
    fn read_buff(self, what: &'static str, target: Entity, before: f32, watched: bool) {
        let under = self.stats.value(target, self.luck);
        self.lift_buff(what, target);
        if watched {
            self.stats.watch(target, false);
        }
        let line = format!("{what}: luck of {target:?} {before} -> {under} under {HUNGER:?}");
        if buff_shows(before, under) {
            enshwick::info!("PASS {line}");
        } else {
            enshwick::warn!("FAIL {line}");
        }
    }

    /// Takes the probe's hunger debuff off `target`.
    fn lift_buff(self, what: &'static str, target: Entity) {
        if let Err(why) = self.buffs.remove(target, HUNGER) {
            enshwick::warn!("FAIL {what}: removal of {HUNGER:?} from {target:?}: {why}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buff_shows_as_its_luck_over_what_was_there() {
        assert!(buff_shows(0.0, BUFF_LUCK));
        assert!(buff_shows(0.25, 0.25 + BUFF_LUCK));
    }

    #[test]
    fn luck_without_the_buff_or_doubled_does_not_show_it() {
        assert!(!buff_shows(0.0, 0.0));
        assert!(!buff_shows(0.0, 2.0 * BUFF_LUCK));
    }
}
