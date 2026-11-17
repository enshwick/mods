//! The checks of what a container and its moves show: the vanilla chest's
//! placement metadata, the game's pickup message a mod shows, and what a
//! player's move into or out of a container carries.

use enshwick::content::vanilla;
use enshwick::interactions::Kind;
use enshwick::inventory::Inventories;
use enshwick::world::World;
use enshwick::{Entity, ItemId, Load, Off, WorldError};

use crate::checks::{fail, pass};

/// The vanilla wooden chest, as the item registry of server 26430253 lists
/// it.
const CHEST: &str = "Prop_Storage_32_T2_Chest_Wood";
/// An entity id no world holds, which a pickup message refuses.
const NO_PLAYER: Entity = Entity(u32::MAX - 1);

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
        load.on_interaction(|interaction| {
            if interaction.kind == Kind::Move {
                enshwick::info!(
                    "interaction move by {}: {:?}",
                    interaction.player.0,
                    interaction.moved
                );
            }
            true
        })?;
        pass("interaction filter registered, logging each move's moved");
        Ok(Self {
            world: load.world()?,
            inventories: load.inventories()?,
            chest,
        })
    }

    /// Runs every check on the game thread.
    pub(crate) fn run(&self) {
        match self.world.prop_placement_info(self.chest) {
            Ok(info) => pass(&format!("prop placement info of {CHEST}: {info:?}")),
            Err(why) => fail(&format!("prop placement info of {CHEST}: {why}")),
        }
        match self.inventories.show_pickup(NO_PLAYER, self.chest, 1) {
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
            Ok(()) => pass(&format!("show pickup to player {}", player.entity.0)),
            Err(why) => fail(&format!("show pickup to player {}: {why}", player.entity.0)),
        }
    }
}
