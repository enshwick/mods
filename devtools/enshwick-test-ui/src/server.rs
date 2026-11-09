//! The server's part: the actions the client's panels and window run, and
//! the build refusal near Flame Altars.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use enshwick::actions::{Action, Actions, Outcome};
use enshwick::effects::{Effects, Spawn};
use enshwick::events::AltarsChanged;
use enshwick::schedule::Scheduler;
use enshwick::vetoes::BuildVerdict;
use enshwick::world::{ContainerKinds, Placed, World};
use enshwick::{Entity, Load, Off};

use crate::SURVIVOR;

/// The action of the survivor dialog's "Hello" button.
pub(crate) const HELLO: &str = "hello";
/// The action that marks the survivor nearest the player.
pub(crate) const MARK: &str = "mark";
/// The action that spawns the kept and the unsaved chest.
pub(crate) const SPAWN: &str = "spawn-kept-and-unsaved";

/// How close to an altar a placement is refused, in metres.
const ALTAR_DISTANCE: f64 = 5.0;
/// Why a placement near an altar is refused.
const TOO_CLOSE: &str = "Too close to the altar";
/// How far from the player a survivor is still marked, in metres.
const MARK_REACH: f64 = 10.0;
/// Where the two chests stand from the player, in metres along the world's
/// axes, whichever way the player faces: 4 m apart.
const KEPT_AT: [f64; 3] = [2.0, 0.0, 3.0];
const UNSAVED_AT: [f64; 3] = [-2.0, 0.0, 3.0];
/// How long after the spawn the unsaved chest leaves the save: a few
/// steps, so it is in the world.
const SETTLE: Duration = Duration::from_millis(200);

/// The services the actions use where a server runs.
#[derive(Clone, Copy)]
struct Server {
    world: World,
    effects: Effects,
    scheduler: Scheduler,
}

fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.iter()
        .zip(b)
        .map(|(a, b)| (a - b).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn offset(at: [f64; 3], by: [f64; 3]) -> [f64; 3] {
    [at[0] + by[0], at[1] + by[1], at[2] + by[2]]
}

/// The one of `places` nearest `me`, no further than `reach` metres.
fn nearest_within(places: Vec<Placed>, me: [f64; 3], reach: f64) -> Option<Placed> {
    places
        .into_iter()
        .filter(|placed| placed.distance(me) <= reach)
        .min_by(|a, b| a.distance(me).total_cmp(&b.distance(me)))
}

/// What the player hears of marking `survivor`.
fn marked(survivor: Entity, labelled: bool) -> Outcome {
    if labelled {
        Outcome::done(format!("survivor {survivor} marked"))
    } else {
        Outcome::refused(format!(
            "the game did not rename survivor {survivor}; see the log"
        ))
    }
}

/// Names the survivor nearest `player`, within [`MARK_REACH`], [`SURVIVOR`],
/// so its prompt says so.
fn mark(server: Server, player: Entity) -> Outcome {
    let Some(me) = server.world.position(player) else {
        return Outcome::refused("your character has no position");
    };
    let npcs = server.world.containers(ContainerKinds::NPCS);
    let Some(survivor) = nearest_within(npcs, me, MARK_REACH) else {
        return Outcome::refused(format!("no survivor within {MARK_REACH} m"));
    };
    let labelled = server.world.set_label(survivor.entity, SURVIVOR);
    enshwick::info!(
        "survivor {} at {:?} marked \"{SURVIVOR}\": {labelled}",
        survivor.entity,
        survivor.position
    );
    marked(survivor.entity, labelled)
}

/// What the player hears of the two spawns.
fn spawn_outcome(kept: Option<Entity>, unsaved: Option<Entity>) -> Outcome {
    match (kept, unsaved) {
        (Some(kept), Some(unsaved)) => {
            Outcome::done(format!("Kept {kept} and Unsaved {unsaved} spawned"))
        }
        (Some(kept), None) => Outcome::refused(format!(
            "Kept {kept} spawned and stays; Unsaved did not spawn, see the log"
        )),
        (None, Some(unsaved)) => Outcome::refused(format!(
            "Unsaved {unsaved} spawned; Kept did not spawn, see the log"
        )),
        (None, None) => Outcome::refused("the chests did not spawn; see the log"),
    }
}

/// Spawns two chests of the template of the placed container nearest
/// `player`, Kept and Unsaved, and takes Unsaved out of the save a few steps
/// later.
fn spawn(server: Server, player: Entity) -> Outcome {
    let Some(me) = server.world.position(player) else {
        return Outcome::refused("your character has no position");
    };
    let mut placed = server.world.containers(ContainerKinds::PLACED);
    placed.sort_by(|a, b| a.distance(me).total_cmp(&b.distance(me)));
    let template = placed
        .iter()
        .find_map(|placed| server.world.template(placed.entity));
    let Some(template) = template else {
        return Outcome::refused("build a storage chest first: its template is copied");
    };
    let spawned = |name: &str, by| {
        let at = offset(me, by);
        let entity = server.effects.spawn(template, at, Spawn::new());
        enshwick::info!("{name}: spawn of {template:02x?} at {at:?}: {entity:?}");
        entity.ok()
    };
    let kept = spawned("Kept", KEPT_AT);
    let unsaved = spawned("Unsaved", UNSAVED_AT);
    if let Some(unsaved) = unsaved {
        let world = server.world;
        server.scheduler.after(SETTLE, move || {
            let result = world.set_saved(unsaved, false);
            enshwick::info!("set_saved(Unsaved {unsaved}, false): {result:?}");
        });
    }
    spawn_outcome(kept, unsaved)
}

/// The services of a server, or why this side has none.
fn services(load: &mut Load<'_>) -> Result<Server, Off> {
    Ok(Server {
        world: load.world()?,
        effects: load.effects(),
        scheduler: load.scheduler()?,
    })
}

/// Declares the actions on every side, so a client runs them; they act
/// where a server runs.
///
/// # Errors
///
/// [`Off`] when the loader refuses an action.
pub(crate) fn register(load: &mut Load<'_>, actions: &Actions) -> Result<(), Off> {
    let server = load.feature("server actions", services);
    let on_server = move |act: fn(Server, Entity) -> Outcome| {
        move |request: &enshwick::actions::Request<'_>| {
            server.map_or_else(
                || Outcome::refused("no server runs here"),
                |server| act(server, request.player.entity),
            )
        }
    };
    actions.add(Action::new(HELLO, "Hello"), |request| {
        enshwick::info!("{} said hello to the test survivor", request.player.name);
        Outcome::done("Hello from the test survivor")
    })?;
    actions.add(
        Action::new(MARK, "Mark the nearest survivor").admins(),
        on_server(mark),
    )?;
    actions.add(
        Action::new(SPAWN, "Spawn kept and unsaved").admins(),
        on_server(spawn),
    )?;
    Ok(())
}

/// Refuses every placement within [`ALTAR_DISTANCE`] of a Flame Altar of
/// the server's world, with [`TOO_CLOSE`].
///
/// # Errors
///
/// [`Off`] where the loader follows no altars or refuses the filter.
pub(crate) fn refuse_near_altars(load: &mut Load<'_>) -> Result<(), Off> {
    let altars = load.altars()?;
    // The cursor check runs every frame on worker threads and must not
    // allocate or lock, so it reads the positions kept at the last change.
    let kept = Arc::new(ArcSwap::from_pointee(Vec::<[f64; 3]>::new()));
    let changed = Arc::clone(&kept);
    load.on::<AltarsChanged>(move |()| {
        let positions = altars.list().iter().map(|altar| altar.position).collect();
        changed.store(Arc::new(positions));
    })?;
    load.on_build(move |placement| {
        let near = kept
            .load()
            .iter()
            .any(|&altar| distance(altar, placement.position) < ALTAR_DISTANCE);
        if near {
            BuildVerdict::refuse_because(TOO_CLOSE)
        } else {
            BuildVerdict::Allow
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_chests_stand_four_metres_apart() {
        let me = [10.0, 5.0, -3.0];
        assert_eq!(offset(me, KEPT_AT), [12.0, 5.0, 0.0]);
        let apart = distance(offset(me, KEPT_AT), offset(me, UNSAVED_AT));
        assert!((apart - 4.0).abs() < 1e-9, "{apart}");
    }

    fn placed(entity: u32, x: f64) -> Placed {
        Placed {
            entity: Entity(entity),
            position: [x, 0.0, 0.0],
        }
    }

    #[test]
    fn only_a_survivor_within_reach_is_marked() {
        let near = nearest_within(vec![placed(1, 30.0), placed(2, 9.0)], [0.0; 3], MARK_REACH);
        assert_eq!(near.map(|placed| placed.entity.0), Some(2));
        assert_eq!(
            nearest_within(vec![placed(1, 30.0)], [0.0; 3], MARK_REACH),
            None
        );
    }

    #[test]
    fn a_label_the_game_refused_is_not_reported_as_marked() {
        assert!(marked(Entity(7), true).is_done());
        assert!(!marked(Entity(7), false).is_done());
    }

    #[test]
    fn a_spawn_that_half_failed_says_which_chest_stands() {
        let (kept, unsaved) = (Entity(1), Entity(2));
        assert_eq!(
            spawn_outcome(Some(kept), Some(unsaved)),
            Outcome::done("Kept 1 and Unsaved 2 spawned")
        );
        assert_eq!(
            spawn_outcome(Some(kept), None),
            Outcome::refused("Kept 1 spawned and stays; Unsaved did not spawn, see the log")
        );
        assert_eq!(
            spawn_outcome(None, Some(unsaved)),
            Outcome::refused("Unsaved 2 spawned; Kept did not spawn, see the log")
        );
        assert_eq!(
            spawn_outcome(None, None),
            Outcome::refused("the chests did not spawn; see the log")
        );
    }
}
