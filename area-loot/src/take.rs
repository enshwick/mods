//! After each loot container check the mod notices a container a player
//! emptied and queues the full ones around it; each is then emptied into that
//! player, nearest first, through the game's own inventory transactions,
//! once it is checked again that nobody else opened it meanwhile. Other mods
//! leave containers out at the extension point `enshwick.area-loot:eligible`.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use enshwick::Entity;
use enshwick::inventory::{Batch, Inventories, Op, Outcome};
use enshwick::loot::{Container, LootPass};
use enshwick::points::OwnedPoint;
use enshwick::state::ModLocal;
use enshwick::time::StepClock;
use enshwick::world::{World, near};
use enshwick::{Load, Off};

use crate::Settings;
use crate::points::Eligible;
use crate::tracker::Tracker;

/// Another player's interaction keeps a container out for this long.
const BUSY: Duration = Duration::from_secs(10);
/// Another player who last opened a container and stands this close still
/// has it open.
const OPEN_RANGE: f64 = 4.0;
/// How often the pass prunes what it remembers.
const PRUNE_EVERY: Duration = Duration::from_secs(60);

/// The settings in force and the pass's switches.
#[derive(Debug)]
struct Switches {
    enabled: AtomicBool,
    /// Radius and height in metres, as `f32` bits.
    radius: AtomicU32,
    height: AtomicU32,
    /// Set once the pass panicked: vanilla from then on.
    off: AtomicBool,
    /// Counts the games run, so a take chain never reaches into the next one.
    game: AtomicU64,
}

impl Default for Switches {
    fn default() -> Self {
        let settings = Settings::default();
        Self {
            enabled: AtomicBool::new(settings.enabled),
            radius: AtomicU32::new(settings.radius.to_bits()),
            height: AtomicU32::new(settings.height.to_bits()),
            off: AtomicBool::new(false),
            game: AtomicU64::new(0),
        }
    }
}

static SWITCHES: ModLocal<Switches> = ModLocal::new();
static INVENTORIES: ModLocal<Inventories> = ModLocal::new();
/// Where players stand, for the check right before a container's turn.
static WORLD: ModLocal<World> = ModLocal::new();
static TRACKER: ModLocal<Mutex<Option<Tracker>>> = ModLocal::new();
static LAST_PRUNE: ModLocal<Mutex<Option<Duration>>> = ModLocal::new();
/// The time the holds and the busy window count in: the server's steps.
static CLOCK: ModLocal<&'static StepClock> = ModLocal::new();
/// The extension point `enshwick.area-loot:eligible`, once defined.
static ELIGIBLE: ModLocal<OwnedPoint<Eligible>> = ModLocal::new();

fn switches() -> &'static Switches {
    SWITCHES.get_or_init(Switches::default)
}

/// Whether `container`, `distance` metres across the ground from the
/// container `emptied` that `player` emptied, comes along: yes, unless
/// another mod's link says no.
fn eligible(player: Entity, emptied: Entity, container: Entity, distance: f32) -> bool {
    ELIGIBLE.get().is_none_or(|point| {
        let input = Eligible {
            player,
            emptied,
            container,
            distance,
        };
        point.decide(&input, true).value_or(false)
    })
}

/// Applies `settings` from now on.
pub(crate) fn apply(settings: &Settings) {
    let was = switches().enabled.swap(settings.enabled, Ordering::Relaxed);
    // Off, the tracker stood still: what was emptied meanwhile must not count
    // as emptied now, with its loot sent to whoever opened it last.
    if settings.enabled
        && !was
        && let Some(t) = tracker().as_mut()
    {
        t.clear();
    }
    switches()
        .radius
        .store(settings.radius.to_bits(), Ordering::Relaxed);
    switches()
        .height
        .store(settings.height.to_bits(), Ordering::Relaxed);
}

/// The step clock's time; zero before the mod linked it.
fn now() -> Duration {
    CLOCK.get().map_or(Duration::ZERO, |clock| clock.now())
}

fn tracker() -> MutexGuard<'static, Option<Tracker>> {
    TRACKER
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

/// Links the pass.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let _first = ELIGIBLE.set(load.define_point()?);
    let _first = INVENTORIES.set(load.inventories()?);
    match load.world() {
        Ok(world) => {
            let _first = WORLD.set(world);
        }
        Err(off) => {
            enshwick::warn!("a player who walks up to a queued container may lose it: {off}");
        }
    }
    *tracker() = Some(Tracker::default());
    let _first = CLOCK.set(load.step_clock()?);
    load.on_loot_pass(check)?;
    // Entity ids belong to one game.
    load.on::<enshwick::events::ShutdownStarted>(|()| {
        switches().game.fetch_add(1, Ordering::AcqRel);
        if let Some(t) = tracker().as_mut() {
            t.clear();
        }
    })
}

fn check(loot: &LootPass<'_>) {
    if switches().off.load(Ordering::Acquire) || !switches().enabled.load(Ordering::Relaxed) {
        return;
    }
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| pass(loot))).is_err()
        && !switches().off.swap(true, Ordering::AcqRel)
    {
        enshwick::warn!("area loot off, vanilla from now on: the pass panicked");
    }
}

/// Watches every container and starts a take for each one a player emptied.
fn pass(loot: &LootPass<'_>) {
    let now = now();
    let mut emptied: Vec<(Container, Entity)> = Vec::new();
    {
        let mut guard = tracker();
        let Some(t) = guard.as_mut() else {
            return;
        };
        loot.each(|c| {
            if let Some(by) = t.observe(c, now) {
                emptied.push((*c, by));
            }
        });
        let mut last = LAST_PRUNE
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if last.is_none_or(|at| now.saturating_sub(at) >= PRUNE_EVERY) {
            t.prune(now);
            *last = Some(now);
        }
    }
    for (container, player) in emptied {
        if container.is_enemy_loot() && loot.is_player(player) {
            let queue = gather(loot, &container, player, now);
            if !queue.is_empty() {
                enshwick::debug!(
                    "{} emptied {}: taking {} more",
                    player.0,
                    container.entity.0,
                    queue.len()
                );
                take_next(player, queue, switches().game.load(Ordering::Acquire));
            }
        }
    }
}

/// The full enemy loot containers around `emptied` that no other player is
/// at, nearest first, each held for the take.
fn gather(
    loot: &LootPass<'_>,
    emptied: &Container,
    player: Entity,
    now: Duration,
) -> VecDeque<Entity> {
    let radius = f64::from(f32::from_bits(switches().radius.load(Ordering::Relaxed)));
    let height = f64::from(f32::from_bits(switches().height.load(Ordering::Relaxed)));
    let mut guard = tracker();
    let Some(t) = guard.as_mut() else {
        return VecDeque::new();
    };
    let mut queue = VecDeque::new();
    for entity in t.around(emptied.position, radius, height, emptied.entity, now) {
        let Some(c) = loot.container(entity) else {
            t.forget(entity);
            continue;
        };
        if !c.is_open() || !near(emptied.position, c.position, radius, height) {
            continue;
        }
        if t.busy(&c, player, now, BUSY) {
            continue;
        }
        if let Some(guest) = c.offer.and_then(|o| o.guest_id)
            && guest != player
        {
            let at = loot.position(guest);
            if at.is_some_and(|at| near(c.position, at, OPEN_RANGE, OPEN_RANGE)) {
                continue;
            }
        }
        if !eligible(
            player,
            emptied.entity,
            entity,
            ground(emptied.position, c.position),
        ) {
            continue;
        }
        t.hold(entity, now);
        queue.push_back(entity);
    }
    queue
}

/// Metres between `a` and `b` across the ground.
#[expect(
    clippy::cast_possible_truncation,
    reason = "distances around a player fit in f32"
)]
fn ground(a: [f64; 3], b: [f64; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[2] - b[2]) as f32
}

/// Empties the first container of `queue` into `player`, and the next once
/// that one emptied completely.
fn take_next(player: Entity, mut queue: VecDeque<Entity>, game: u64) {
    let Some(&inventories) = INVENTORIES.get() else {
        return;
    };
    // Switched off, stopped by a panic or another game since: the rest stays.
    if switches().off.load(Ordering::Acquire)
        || !switches().enabled.load(Ordering::Relaxed)
        || switches().game.load(Ordering::Acquire) != game
    {
        if let Some(t) = tracker().as_mut() {
            for rest in queue {
                t.release(rest);
            }
        }
        return;
    }
    // Each container is checked again right before its turn: another
    // player may have opened it since the take gathered it, or walked up to
    // the one they opened last.
    let container = loop {
        let Some(next) = queue.pop_front() else {
            return;
        };
        let mut guard = tracker();
        let Some(t) = guard.as_mut() else {
            return;
        };
        let guest_near = WORLD.get().is_some_and(|world| {
            t.guest_near(next, player, OPEN_RANGE, |guest| world.position(guest))
        });
        if t.still_free(next, player, now()) && !guest_near {
            break next;
        }
        t.release(next);
        enshwick::debug!(
            "area loot for {} skips {}: opened since or another player at it",
            player.0,
            next.0
        );
    };
    let batch = Batch::new()
        .with(Op::MoveAll {
            from: container,
            to: player,
            include_special: false,
        })
        .actor(player)
        .allow_partial()
        .notify();
    let _ticket = inventories.submit(&batch, move |outcome: &Outcome| {
        let left = inventories
            .read()
            .slots(container)
            .iter()
            .any(|s| !s.is_empty());
        if emptied(outcome) && !left {
            take_next(player, queue, game);
            return;
        }
        // Inventory full or the container changed: the rest stays for later.
        enshwick::debug!(
            "area loot for {} stopped at {}: {:?}",
            player.0,
            container.0,
            outcome.done
        );
        if let Some(t) = tracker().as_mut() {
            t.release(container);
            for rest in queue {
                t.release(rest);
            }
        }
    });
}

/// Whether the take committed with every operation run; what is left in
/// the container is checked apart.
fn emptied(outcome: &Outcome) -> bool {
    outcome.is_committed() && outcome.ops.iter().all(|r| r.ok())
}

#[cfg(test)]
mod tests {
    use enshwick::inventory::{Done, OpError, OpResult};

    use super::*;

    #[test]
    fn only_a_full_commit_empties() {
        let full = |error, remaining| Outcome {
            done: Done::Committed,
            ops: vec![OpResult { error, remaining }],
        };
        assert!(emptied(&full(None, 0)));
        assert!(!emptied(&full(Some(OpError::InvalidState), 0)));
        assert!(!emptied(&Outcome {
            done: Done::RolledBack,
            ops: vec![OpResult {
                error: None,
                remaining: 0
            }],
        }));
    }

    #[test]
    fn other_mods_leave_containers_out() {
        use enshwick::abi::Status;
        use enshwick::points::Verdict;

        assert!(
            eligible(Entity(1), Entity(2), Entity(3), 4.0),
            "before init"
        );
        let loader = enshwick::test::FakeHost::new().install();
        // The fake lacks the inventories, which the mod links after its
        // point: it turns off, and the point stays defined.
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe { loader.start(crate::enshwick_mod_init, crate::enshwick_mod_ready) };
        assert_ne!(status, Status::OK);
        assert_eq!(loader.defined_points(), ["enshwick.area-loot:eligible"]);
        assert!(eligible(Entity(1), Entity(2), Entity(3), 4.0));
        assert!(
            loader.extend_point(|loot: &Eligible, _: &bool| { Verdict::Set(loot.distance <= 5.0) })
        );
        assert!(eligible(Entity(1), Entity(2), Entity(3), 4.0));
        assert!(!eligible(Entity(1), Entity(2), Entity(3), 6.0));
        assert!((ground([0.0, 0.0, 0.0], [3.0, 9.0, 4.0]) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn settings_reach_the_pass() {
        apply(&Settings {
            radius: 20.0,
            height: 2.5,
            enabled: false,
        });
        assert!(!switches().enabled.load(Ordering::Relaxed));
        assert!(
            (f32::from_bits(switches().radius.load(Ordering::Relaxed)) - 20.0).abs() < f32::EPSILON
        );
        assert!(
            (f32::from_bits(switches().height.load(Ordering::Relaxed)) - 2.5).abs() < f32::EPSILON
        );
        apply(&Settings::default());
        assert!(switches().enabled.load(Ordering::Relaxed));
    }
}
