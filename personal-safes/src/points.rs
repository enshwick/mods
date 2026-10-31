//! The extension points the mod defines; other mods write their definitions
//! of them with `cargo enshwick point`.

use enshwick::{Entity, PlayerKey};

/// What a player tries to do with a Safe.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#[shape(values(USE, REMOVE, MANAGE))]
pub struct SafeAction(pub u32);

impl SafeAction {
    /// Open it, take, put or sort, craft and build from it.
    pub const USE: Self = Self(0);
    /// Pick it up or dismantle it.
    pub const REMOVE: Self = Self(1);
    /// Rename it or change who may use it.
    pub const MANAGE: Self = Self(2);
}

/// Who may open, use, remove or manage a Safe,
/// `enshwick.personal-safes:access`: the answer is whether the player may,
/// and a refusal is final.
///
/// The mod answers first: the owner may do all, the players they allowed
/// only use it, administrators all when its settings allow. A player the
/// server has not registered yet is refused before the point is asked.
/// Asked on the server's worker threads, inside the game's systems: a link
/// reads lock-free state only.
#[enshwick::point(
    name = "enshwick.personal-safes:access",
    mode = Override,
    output = bool,
    refusable
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Access {
    /// The player's account key.
    pub player: PlayerKey,
    /// The owner's account key.
    pub owner: PlayerKey,
    /// The player's character.
    pub character: Entity,
    /// The Safe; [`Entity`] of the chest itself.
    pub safe: Entity,
    /// What the player tries.
    pub action: SafeAction,
    /// Whether the server makes the player an administrator.
    pub admin: bool,
    /// Whether the owner allowed the player at this Safe.
    pub allowed: bool,
    /// Keeps the layout free of padding; always zero.
    pub reserved: u16,
}
