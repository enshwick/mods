//! The extension points the mod defines; other mods write their definitions
//! of them with `cargo enshwick point`.

use enshwick::ecs::Entity;

/// Whether a loot container around an emptied one comes along,
/// `area-loot:eligible`: the answer is whether it does.
///
/// Asked only for a full enemy loot container within the mod's radius
/// that no other player is at, with the answer true, so a link can leave
/// it out but never take in one the mod passed over. Asked on the server's
/// game thread when a player empties a container, once per container
/// around it.
#[enshwick::point(name = "area-loot:eligible", mode = Override, output = bool)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Eligible {
    /// The player who emptied a container.
    pub player: Entity,
    /// The container the player emptied.
    pub emptied: Entity,
    /// The container around it.
    pub container: Entity,
    /// Metres between the two, across the ground.
    pub distance: f32,
}
