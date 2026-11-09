//! The server's part: the actions the client's panels and window run, and
//! the build refusal near Flame Altars.

use std::time::Duration;

use enshwick::actions::{Action, Actions, Outcome};
use enshwick::effects::{Effects, Spawn};
use enshwick::schedule::Scheduler;
use enshwick::vetoes::BuildVerdict;
use enshwick::world::{ContainerKinds, World};
use enshwick::{Load, Off};

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
/// Where the two chests stand from the player, in metres: 4 m apart.
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

/// Names the survivor nearest `player` [`SURVIVOR`], so its prompt says so.
fn mark(server: Server, player: enshwick::Entity) -> Outcome {
    let Some(me) = server.world.position(player) else {
        return Outcome::refused("your character has no position");
    };
    let nearest = server
        .world
        .containers(ContainerKinds::NPCS)
        .into_iter()
        .min_by(|a, b| distance(a.position, me).total_cmp(&distance(b.position, me)));
    let Some(survivor) = nearest else {
        return Outcome::refused("no survivor in the world");
    };
    let labelled = server.world.set_label(survivor.entity, SURVIVOR);
    enshwick::info!(
        "survivor {} at {:?} marked \"{SURVIVOR}\": {labelled}",
        survivor.entity,
        survivor.position
    );
    Outcome::done(format!("survivor {} marked: {labelled}", survivor.entity))
}

/// Spawns two chests of the template of the placed container nearest
/// `player`, Kept and Unsaved, and takes Unsaved out of the save a few steps
/// later.
fn spawn(server: Server, player: enshwick::Entity) -> Outcome {
    let Some(me) = server.world.position(player) else {
        return Outcome::refused("your character has no position");
    };
    let template = server
        .world
        .containers(ContainerKinds::PLACED)
        .into_iter()
        .filter_map(|placed| {
            Some((
                server.world.template(placed.entity)?,
                distance(placed.position, me),
            ))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(template, _)| template);
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
    let Some(unsaved) = spawned("Unsaved", UNSAVED_AT) else {
        return Outcome::refused("the chests did not spawn; see the log");
    };
    let world = server.world;
    server.scheduler.after(SETTLE, move || {
        let result = world.set_saved(unsaved, false);
        enshwick::info!("set_saved(Unsaved {unsaved}, false): {result:?}");
    });
    Outcome::done(format!("Kept {kept:?} and Unsaved {unsaved} spawned"))
}

/// Declares the actions on every side, so a client runs them; they act
/// where a server runs.
///
/// # Errors
///
/// [`Off`] when the loader refuses an action.
pub(crate) fn register(load: &mut Load<'_>, actions: &Actions) -> Result<(), Off> {
    let server = load.world().ok().and_then(|world| {
        Some(Server {
            world,
            effects: load.effects(),
            scheduler: load.scheduler().ok()?,
        })
    });
    let on_server = move |act: fn(Server, enshwick::Entity) -> Outcome| {
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
    load.on_build(move |placement| {
        // shortcut: lists the altars on each cursor check; cache them on
        // AltarsChanged if this tool ever runs where frames matter.
        let near = altars
            .list()
            .iter()
            .any(|altar| distance(altar.position, placement.position) < ALTAR_DISTANCE);
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
        let apart = distance(offset(me, KEPT_AT), offset(me, UNSAVED_AT));
        assert!((apart - 4.0).abs() < 1e-9, "{apart}");
    }
}
