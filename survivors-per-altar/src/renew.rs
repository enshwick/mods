//! Copy staffs from before charges get their charges at once: in the
//! dedicated server and in a game a client hosts, every copy staff without
//! an entity of its own, in a player's inventory, a built chest or a world
//! chest, is renewed full, when the world loads, when a player joins and
//! every minute for chests loaded since.

use std::ops::ControlFlow;
use std::time::Duration;

use enshwick::events::{PlayerJoined, WorldLoaded};
use enshwick::inventory::{Fill, Inventories};
use enshwick::world::{ContainerKinds, World};
use enshwick::{Entity, ItemId, Load, Off};

/// How often containers are looked through for old staffs.
const SWEEP: Duration = Duration::from_secs(60);

/// Renews the old copy staffs in `root`; logs what it asked for.
fn renew(inventories: &Inventories, root: Entity, staffs: &[ItemId]) {
    let asked = inventories.renew(root, staffs, Fill::FULL, move |outcome| {
        enshwick::info!(
            "old copy staffs in {root} renewed with full charges: {:?}",
            outcome.done
        );
    });
    if asked.is_some() {
        enshwick::debug!("old copy staffs in {root}: renewal asked");
    }
}

/// Every player of the world, and every container players build or the
/// world fills with loot.
fn holders(world: World) -> Vec<Entity> {
    let mut roots: Vec<Entity> = world.players().iter().map(|p| p.entity).collect();
    let containers = world.containers(ContainerKinds::PLACED | ContainerKinds::LOOT);
    roots.extend(containers.iter().map(|placed| placed.entity));
    roots
}

/// Links the renewals: they run in the dedicated server and in a game a
/// client hosts, solo play included.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let inventories = load.inventories()?;
    let world = load.world()?;
    let scheduler = load.scheduler()?;
    let staffs = crate::staffs::copy_ids();
    let sweep = {
        let staffs = staffs.clone();
        move || {
            for root in holders(world) {
                renew(&inventories, root, &staffs);
            }
        }
    };
    let loaded = sweep.clone();
    load.on::<WorldLoaded>(move |()| loaded())?;
    scheduler.every(SWEEP, move || {
        sweep();
        ControlFlow::Continue(())
    });
    load.on::<PlayerJoined>(move |joined| {
        renew(&inventories, joined.entity, &staffs);
    })?;
    Ok(())
}
