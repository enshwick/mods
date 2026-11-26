//! The `respawn` and `set-respawn` actions: show a player where they
//! respawn, remember it, and move them back there later.

use std::sync::Mutex;

use enshwick::actions::Outcome;
use enshwick::world::{Player, World};
use enshwick::{Entity, PlayerKey, Relock};
use rustc_hash::FxHashMap;

/// The respawn point `respawn` last showed each player in this world.
#[derive(Debug)]
pub(crate) struct RespawnPoints {
    world: World,
    shown: Mutex<FxHashMap<PlayerKey, Entity>>,
}

impl RespawnPoints {
    pub(crate) fn new(world: World) -> Self {
        Self {
            world,
            shown: Mutex::new(FxHashMap::default()),
        }
    }

    /// The `respawn` action: where `player` respawns, which `set` then
    /// moves back to; a point gone or unread is forgotten, and refused when
    /// it does not read.
    pub(crate) fn show(&self, player: &Player) -> Outcome {
        let point = self.world.respawn_point(player);
        enshwick::info!("respawn point of {}: {point:?}", player.name);
        let mut shown = self.shown.relock();
        match &point {
            Ok(Some(found)) => shown.insert(player.key, found.entity),
            _ => shown.remove(&player.key),
        };
        match point {
            Ok(found) => Outcome::done(format!("{found:?}")),
            Err(why) => Outcome::refused(why.to_string()),
        }
    }

    /// The `set-respawn` action: moves `player`'s respawn point back to the
    /// one `show` last showed them in this world; refused when there is none
    /// or the set is.
    pub(crate) fn set(&self, player: &Player) -> Outcome {
        let Some(spawn_point) = self.shown.relock().get(&player.key).copied() else {
            return Outcome::refused("no respawn point shown yet: use the respawn action first");
        };
        let set = self.world.set_respawn_point(player, spawn_point);
        enshwick::info!(
            "set respawn point of {} to {spawn_point}: {set:?}",
            player.name
        );
        match set {
            Ok(()) => Outcome::done(format!("respawns at {spawn_point}")),
            Err(why) => Outcome::refused(why.to_string()),
        }
    }

    /// Forgets every point shown, since a world numbers its entities anew at
    /// each load.
    pub(crate) fn forget(&self) {
        self.shown.relock().clear();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use enshwick::abi::{PlayerInfo, RespawnPointInfo, Status};
    use enshwick::{Entity, Load, Mod, Off, PlayerKey};

    use super::RespawnPoints;

    const CHARACTER: Entity = Entity::from_engine(10);
    const ALTAR: Entity = Entity::from_engine(40);
    const SPIRE: Entity = Entity::from_engine(41);

    thread_local! {
        /// The world of this test's mod: init runs on the test's thread, and
        /// `cargo test` runs tests as threads of one process.
        static WORLD: Cell<Option<enshwick::world::World>> = const { Cell::new(None) };
    }

    struct Homes;

    impl Mod for Homes {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            WORLD.set(Some(load.world()?));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Homes,
        r#"
        id = "probe.respawns"
        side = "server"
        "#
    );

    fn started() -> (
        enshwick::test::Installed,
        RespawnPoints,
        enshwick::world::Player,
    ) {
        let loader = enshwick::test::FakeHost::new()
            .world()
            .player(enshwick::abi::headed!(PlayerInfo {
                slot: 0,
                entity: CHARACTER,
                key: PlayerKey::new(7),
                permissions: 0,
                name_len: 0,
                name: [0; PlayerInfo::NAME_MAX],
            }))
            .spawn_point(ALTAR, RespawnPointInfo::BASE, [1.0; 3])
            .spawn_point(SPIRE, RespawnPointInfo::SAVE_POINT, [2.0; 3])
            .respawns_at(CHARACTER, SPIRE)
            .install();
        // SAFETY: both functions are the exports of this module's `declare_mod!(Homes)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        let world = WORLD.get().unwrap();
        let player = world.players().remove(0);
        (loader, RespawnPoints::new(world), player)
    }

    #[test]
    fn set_moves_a_player_back_to_the_point_shown() {
        let (_loader, points, player) = started();
        assert!(points.show(&player).is_done());
        points.world.set_respawn_point(&player, ALTAR).unwrap();
        assert!(points.set(&player).is_done());
        let now = points.world.respawn_point(&player).unwrap().unwrap();
        assert_eq!(now.entity, SPIRE);
    }

    #[test]
    fn a_new_world_forgets_the_points_shown_in_the_last() {
        let (_loader, points, player) = started();
        assert!(points.show(&player).is_done());
        points.forget();
        assert!(!points.set(&player).is_done());
    }

    #[test]
    fn a_point_that_no_longer_reads_is_forgotten() {
        let (_loader, points, mut player) = started();
        assert!(points.show(&player).is_done());
        player.entity = Entity::from_engine(9);
        assert!(
            points.show(&player).is_done(),
            "no point is an answer, not a refusal"
        );
        player.entity = CHARACTER;
        assert!(!points.set(&player).is_done());
    }
}
