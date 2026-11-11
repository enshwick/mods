//! Where a server runs: what each joining player gets, and the enemy that
//! comes to the first player.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use core::time::Duration;

use enshwick::content::props::PropId;
use enshwick::content::templates::TemplateId;
use enshwick::effects::{Effects, Spawn};
use enshwick::events::PlayerJoined;
use enshwick::inventory::Inventories;
use enshwick::knowledge::{Grant, Knowledge};
use enshwick::schedule::Scheduler;
use enshwick::typed::ComponentType;
use enshwick::typed::ecs::CurrentTransform;
use enshwick::world::World;
use enshwick::{Entity, ItemId, Load, Off, PlayerKey, RecipeId};

/// How long after the first player joins the enemy comes.
const ENEMY_AFTER: Duration = Duration::from_secs(30);
/// How far in front of the player it stands, in metres.
const ENEMY_AHEAD: f64 = 10.0;
/// How far above the player's feet it starts, so ground that rises between
/// them does not hold it.
const ENEMY_LIFT: f64 = 2.0;

/// What the content added and the server hands out; `None` for a part
/// that turned off.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Gifts {
    enemy: Option<TemplateId>,
    altar: Option<ItemId>,
    chest: Option<ItemId>,
    chest_recipe: Option<RecipeId>,
}

impl Gifts {
    /// The gifts of the parts that are on.
    pub(crate) fn new(
        enemy: Option<TemplateId>,
        altar: Option<PropId>,
        chest: Option<PropId>,
    ) -> Self {
        Self {
            enemy,
            altar: altar.map(|kind| kind.item),
            chest: chest.map(|chest| chest.item),
            chest_recipe: chest.map(|_| RecipeId::from_guid(crate::CHEST.part("recipe").guid())),
        }
    }
}

/// The services a server lends.
struct Server {
    world: World,
    effects: Effects,
    scheduler: Scheduler,
    inventories: Inventories,
    knowledge: Knowledge,
    transform: ComponentType<CurrentTransform>,
    /// Whether the enemy is sent already: it comes once, to the first player.
    sent: AtomicBool,
}

impl Server {
    fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self {
            world: load.world()?,
            effects: load.effects(),
            scheduler: load.scheduler()?,
            inventories: load.inventories()?,
            knowledge: load.knowledge(),
            transform: ComponentType::resolve(load)?,
            sent: AtomicBool::new(false),
        })
    }

    /// Gives `player` the altar and the chest, and teaches them the chest's
    /// recipe.
    fn welcome(&self, player: Entity, gifts: Gifts) {
        for (what, item) in [("altar", gifts.altar), ("chest", gifts.chest)] {
            if let Some(item) = item {
                let _ticket = self.inventories.give(player, item, 1, move |outcome| {
                    enshwick::info!("{what} given to {player}: {:?}", outcome.done);
                });
            }
        }
        if let Some(recipe) = gifts.chest_recipe {
            let learned = self.knowledge.grant(player, recipe.into(), Grant::Flag(0));
            enshwick::info!("chest recipe taught to {player}: {learned:?}");
        }
    }

    /// Spawns the enemy [`ENEMY_AHEAD`] in front of the player `key`,
    /// facing them.
    fn send_enemy(&self, key: PlayerKey, enemy: TemplateId) {
        let Some(player) = self.world.player_by_key(key) else {
            enshwick::warn!("the first player left before the enemy came");
            return;
        };
        let transform = match self.transform.read(self.world, player.entity) {
            Ok(Some(current)) => current.transform,
            other => {
                enshwick::warn!("the first player's transform does not read: {other:?}");
                return;
            }
        };
        let Some(player_at) = self.world.position(player.entity) else {
            enshwick::warn!("the first player has no position");
            return;
        };
        let rotation = transform.orientation;
        let [dx, dz] = ahead([rotation.x, rotation.y, rotation.z, rotation.w]);
        let at = in_front(player_at, [dx, dz]);
        let how = Spawn::new()
            .level(crate::ENEMY_LEVEL)
            .orientation_yaw(facing_back([dx, dz]))
            .do_not_save();
        let spawned = self.effects.spawn(enemy.guid().0, at, how);
        enshwick::info!(
            "enemy sent to {} at {at:?}, {ENEMY_AHEAD} m along {:?}: {spawned:?}",
            player.name,
            [dx, dz]
        );
    }
}

/// The horizontal direction, `[x, z]` and of length 1, that the rotation
/// `[x, y, z, w]` turns the forward axis +z to; +z when it points straight
/// up or down.
// shortcut: +z as a character's forward is the axis `Spawn::orientation_yaw`
// turns from, not yet seen on a character; the in-game check shows it.
fn ahead([x, y, z, w]: [f32; 4]) -> [f64; 2] {
    let [x, y, z, w] = [x, y, z, w].map(f64::from);
    let dx = 2.0 * (x * z + w * y);
    let dz = 1.0 - 2.0 * (x * x + y * y);
    let length = dx.hypot(dz);
    if length < 1e-6 {
        [0.0, 1.0]
    } else {
        [dx / length, dz / length]
    }
}

/// Where the enemy starts: [`ENEMY_AHEAD`] along the heading `[dx, dz]`
/// from `[x, y, z]`, [`ENEMY_LIFT`] up.
fn in_front([x, y, z]: [f64; 3], [dx, dz]: [f64; 2]) -> [f64; 3] {
    [x + dx * ENEMY_AHEAD, y + ENEMY_LIFT, z + dz * ENEMY_AHEAD]
}

/// The yaw that turns +z to the opposite of `[dx, dz]`, to face back along
/// it.
#[expect(
    clippy::cast_possible_truncation,
    reason = "a yaw in radians fits an f32"
)]
fn facing_back([dx, dz]: [f64; 2]) -> f32 {
    (-dx).atan2(-dz) as f32
}

/// Hands out the gifts at each join and sends the enemy to the first
/// player, where a server runs; a client that does not host has nothing to
/// link.
pub(crate) fn link(load: &mut Load<'_>, gifts: Gifts) {
    let joins = load.feature("join gifts and the enemy", |load| {
        let server = Arc::new(Server::new(load)?);
        load.on::<PlayerJoined>(move |joined| {
            server.welcome(joined.entity, gifts);
            let Some(enemy) = gifts.enemy else {
                return;
            };
            if server.sent.swap(true, Ordering::Relaxed) {
                return;
            }
            let (sender, key) = (Arc::clone(&server), joined.key);
            server
                .scheduler
                .after(ENEMY_AFTER, move || sender.send_enemy(key, enemy));
        })
    });
    if joins.is_none() {
        enshwick::info!("no server runs here: no gifts at join and no enemy");
    }
}

#[cfg(test)]
mod tests {
    use core::f32::consts::{FRAC_PI_2, PI};

    use super::*;

    /// A turn of `yaw` radians about the vertical axis, as `Spawn` makes it.
    fn yaw(yaw: f32) -> [f32; 4] {
        let half = yaw / 2.0;
        [0.0, half.sin(), 0.0, half.cos()]
    }

    fn close(got: [f64; 2], want: [f64; 2]) -> bool {
        (got[0] - want[0]).abs() < 1e-6 && (got[1] - want[1]).abs() < 1e-6
    }

    #[test]
    fn no_turn_looks_along_plus_z() {
        assert!(close(ahead(yaw(0.0)), [0.0, 1.0]));
    }

    #[test]
    fn a_quarter_turn_looks_along_plus_x() {
        assert!(
            close(ahead(yaw(FRAC_PI_2)), [1.0, 0.0]),
            "{:?}",
            ahead(yaw(FRAC_PI_2))
        );
    }

    #[test]
    fn a_tilt_keeps_the_heading_on_the_ground() {
        // Half a right angle down about x, then a quarter turn about y.
        let (s, c) = (PI / 8.0).sin_cos();
        let (ys, yc) = (PI / 4.0).sin_cos();
        let tilted = [yc * s, ys * c, -ys * s, yc * c];
        let [dx, dz] = ahead(tilted);
        assert!((dx.hypot(dz) - 1.0).abs() < 1e-6, "{dx} {dz}");
        assert!(close([dx, dz], [1.0, 0.0]), "{dx} {dz}");
    }

    #[test]
    fn a_tilt_alone_still_looks_along_plus_z_at_full_length() {
        let (s, c) = (PI / 8.0).sin_cos();
        assert!(
            close(ahead([s, 0.0, 0.0, c]), [0.0, 1.0]),
            "{:?}",
            ahead([s, 0.0, 0.0, c])
        );
    }

    #[test]
    fn the_enemy_starts_ten_metres_ahead_and_two_up() {
        assert_eq!(
            in_front([5.0, 100.0, -3.0], [0.6, -0.8]),
            [11.0, 102.0, -11.0]
        );
    }

    #[test]
    fn looking_straight_up_falls_back_to_plus_z() {
        let (s, c) = (PI / 4.0).sin_cos();
        assert_eq!(ahead([-s, 0.0, 0.0, c]), [0.0, 1.0]);
    }

    #[test]
    fn the_enemy_faces_back_at_the_player() {
        for heading in [[0.0, 1.0], [1.0, 0.0], [0.6, -0.8]] {
            let back = ahead(yaw(facing_back(heading)));
            assert!(
                close(back, [-heading[0], -heading[1]]),
                "{heading:?} {back:?}"
            );
        }
    }

    #[test]
    fn parts_that_turned_off_hand_out_nothing() {
        let gifts = Gifts::new(None, None, None);
        assert_eq!(gifts.chest_recipe, None);
        assert!(gifts.altar.is_none() && gifts.chest.is_none() && gifts.enemy.is_none());
    }
}
