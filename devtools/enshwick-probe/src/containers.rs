//! The checks of what a container and its moves show: the vanilla chest's
//! placement metadata, the game's pickup message a mod shows, and what a
//! player's move into or out of a container carries.

use enshwick::content::vanilla;
use enshwick::interactions::Kind;
use enshwick::inventory::Inventories;
use enshwick::world::World;
use enshwick::{ItemId, Load, Off, WorldError};

use crate::checks::{NOT_IN_WORLD, fail, pass};
use crate::platform::CHEST;

/// The container checks and what they need.
#[derive(Debug)]
pub(crate) struct ContainerChecks {
    world: World,
    inventories: Inventories,
    chest: ItemId,
}

impl ContainerChecks {
    /// Reads the parts and logs every move a player makes, which only init
    /// may register.
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        let chest = vanilla::item(CHEST)
            .map(ItemId::from_guid)
            .map_err(|why| Off::new(format!("{CHEST}: {why}")))?;
        let (world, inventories) = (load.world()?, load.inventories()?);
        load.on_interaction(|interaction| {
            if interaction.kind == Kind::Move {
                enshwick::info!(
                    "interaction move by {}: {:?}",
                    interaction.player.get(),
                    interaction.moved
                );
            }
            true
        })?;
        pass("interaction filter registered, logging each move's moved");
        Ok(Self {
            world,
            inventories,
            chest,
        })
    }

    /// Runs every check on the game thread.
    pub(crate) fn run(&self) {
        match self.world.prop_placement_info(self.chest) {
            Ok(info) => pass(&format!("prop placement info of {CHEST}: {info:?}")),
            Err(why) => fail(&format!("prop placement info of {CHEST}: {why}")),
        }
        match self.inventories.show_pickup(NOT_IN_WORLD, self.chest, 1) {
            Err(WorldError::Refused) => pass("show pickup refuses an entity that is no player"),
            shown => fail(&format!(
                "show pickup to an entity that is no player: {shown:?}, not refused"
            )),
        }
        let Some(player) = self.world.players().into_iter().next() else {
            enshwick::info!("SKIP show pickup to a player: nobody online");
            return;
        };
        match self.inventories.show_pickup(player.entity, self.chest, 1) {
            Ok(()) => pass(&format!("show pickup to player {}", player.entity.get())),
            Err(why) => fail(&format!(
                "show pickup to player {}: {why}",
                player.entity.get()
            )),
        }
    }
}
