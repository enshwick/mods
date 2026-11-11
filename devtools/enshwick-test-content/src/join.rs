//! Where a server runs: what each joining player gets, and the enemy that
//! comes to the first player of each hosted game.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use core::time::Duration;

use enshwick::content::props::PropId;
use enshwick::content::templates::TemplateId;
use enshwick::effects::{Effects, Spawn};
use enshwick::events::{PlayerJoined, WorldLoaded};
use enshwick::inventory::Inventories;
use enshwick::knowledge::{Grant, Knowledge};
use enshwick::schedule::Scheduler;
use enshwick::typed::ComponentType;
use enshwick::typed::ecs::CurrentTransform;
use enshwick::world::World;
use enshwick::{Entity, ItemId, Load, Off, PlayerKey, RecipeId};

/// The features' names on the Mods screen.
pub(crate) const GIFTS: &str = "gifts at join";
pub(crate) const ENEMY_AT_JOIN: &str = "enemy at join";
/// How long after the first player joins the enemy comes.
pub(crate) const ENEMY_AFTER: Duration = Duration::from_secs(30);
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
    /// The gifts of the parts that are on; the chest's recipe is
    /// `<id>.recipe`, as `NewProp::recipe_cost` names it.
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

/// Gives `player` the altar and the chest, and teaches them the chest's
/// recipe.
fn welcome(inventories: &Inventories, knowledge: &Knowledge, player: Entity, gifts: Gifts) {
    for (what, item) in [("altar", gifts.altar), ("chest", gifts.chest)] {
        if let Some(item) = item {
            let _ticket = inventories.give(player, item, 1, move |outcome| {
                enshwick::info!("{what} given to {player}: {:?}", outcome.done);
            });
        }
    }
    if let Some(recipe) = gifts.chest_recipe {
        let learned = knowledge.grant(player, recipe.into(), Grant::Flag(0));
        enshwick::info!("chest recipe taught to {player}: {learned:?}");
    }
}

/// What sending the enemy takes.
struct Sender {
    enemy: TemplateId,
    world: World,
    effects: Effects,
    scheduler: Scheduler,
    transform: ComponentType<CurrentTransform>,
    /// Whether the enemy is on its way or came: it comes once a hosted
    /// game, to its first player still there when it comes.
    sent: AtomicBool,
}

impl Sender {
    fn new(load: &mut Load<'_>, enemy: TemplateId) -> Result<Self, Off> {
        Ok(Self {
            enemy,
            world: load.world()?,
            effects: load.effects(),
            scheduler: load.scheduler()?,
            transform: ComponentType::resolve(load)?,
            sent: AtomicBool::new(false),
        })
    }

    /// Spawns the enemy [`ENEMY_AHEAD`] in front of the player `key`,
    /// facing them; when it cannot, the next player to join gets it.
    fn send(&self, key: PlayerKey) {
        if let Err(why) = self.try_send(key) {
            enshwick::warn!("no enemy sent: {why}; it goes to the next player who joins");
            self.sent.store(false, Ordering::Relaxed);
        }
    }

    fn try_send(&self, key: PlayerKey) -> Result<(), Off> {
        let player = self
            .world
            .player_by_key(key)
            .ok_or_else(|| Off::new("the player left before it came"))?;
        let transform = match self.transform.read(self.world, player.entity) {
            Ok(Some(current)) => current.transform,
            other => {
                return Err(Off::new(format!(
                    "the player's transform does not read: {other:?}"
                )));
            }
        };
        let player_at = self
            .world
            .position(player.entity)
            .ok_or_else(|| Off::new("the player has no position"))?;
        let rotation = transform.orientation;
        let [dx, dz] = ahead([rotation.x, rotation.y, rotation.z, rotation.w]);
        let at = in_front(player_at, [dx, dz]);
        let how = Spawn::new()
            .orientation_yaw(facing_back([dx, dz]))
            .do_not_save();
        let spawned = self
            .effects
            .spawn(self.enemy.guid().0, at, how)
            .map_err(|why| Off::new(format!("the spawn failed: {why:?}")))?;
        enshwick::info!(
            "enemy {spawned:?} sent to {} at {at:?}, {ENEMY_AHEAD} m along {:?}",
            player.name,
            [dx, dz]
        );
        Ok(())
    }
}

/// The horizontal direction, `[x, z]` and of length 1, that the rotation
/// `[x, y, z, w]` turns the forward axis +z to; +z when it points straight
/// up or down.
// +z is an actor's forward: the game's actor rotation system (server
// 26430253, rva 0x6f330, under sys.actor_rotation_lerp) turns +z by
// `WorldTransform::orientation` to get the way an actor faces.
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

/// Hands out the gifts at each join, and sends the enemy `after` the first
/// player of each hosted game joins, each as a feature of its own; they
/// hear joins where a server runs.
pub(crate) fn link(load: &mut Load<'_>, gifts: Gifts, after: Duration) {
    let _gifts = load.feature(GIFTS, |load| {
        let (inventories, knowledge) = (load.inventories()?, load.knowledge());
        load.on::<PlayerJoined>(move |joined| {
            welcome(&inventories, &knowledge, joined.entity, gifts);
        })
    });
    let Some(enemy) = gifts.enemy else {
        return;
    };
    let _enemy = load.feature(ENEMY_AT_JOIN, |load| {
        let sender = Arc::new(Sender::new(load, enemy)?);
        let reset = Arc::clone(&sender);
        load.on::<WorldLoaded>(move |()| reset.sent.store(false, Ordering::Relaxed))?;
        load.on::<PlayerJoined>(move |joined| {
            if sender.sent.swap(true, Ordering::Relaxed) {
                return;
            }
            let (to, key) = (Arc::clone(&sender), joined.key);
            sender.scheduler.after(after, move || to.send(key));
        })
    });
}

#[cfg(test)]
mod tests {
    use core::f32::consts::{FRAC_PI_2, PI};

    use enshwick::abi::{Header, KnowledgeInfo, PlayerData, PlayerInfo, Status};
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::knowledge::KnowledgeId;
    use enshwick::resources::{Field, Kind, Type, fnv1a32};
    use enshwick::test::{FakeHost, Installed, facts};
    use enshwick::{Mod, PlayerKey};

    use super::*;

    /// The enemy the join feature sends in these tests.
    const WOLF: TemplateId = TemplateId::new(Guid([9; 16]));
    /// One server step.
    const STEP: Duration = Duration::from_millis(100);

    /// A prop the content added, with its item `item`.
    fn prop(item: u8) -> PropId {
        PropId {
            item: ItemId(item.into()),
            template: TemplateId::new(Guid([item + 1; 16])),
            crafting_prop: None,
        }
    }

    /// The join feature alone, every part on, the enemy without delay.
    struct Joins;

    impl Mod for Joins {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let gifts = Gifts::new(Some(WOLF), Some(prop(1)), Some(prop(3)));
            link(load, gifts, Duration::ZERO);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Joins,
        r#"
        id = "enshwick.test-content"
        side = "server"
        "#
    );

    /// A `CurrentTransform` at `[x, y, z]` metres, turned `turn` radians
    /// about the vertical axis.
    fn standing([x, y, z]: [i64; 3], turn: f32) -> Vec<u8> {
        let mut bytes: Vec<u8> = [x << 32, y << 32, z << 32]
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect();
        bytes.extend(yaw(turn).iter().flat_map(|c| c.to_le_bytes()));
        bytes.extend([1.0_f32; 3].iter().flat_map(|c| c.to_le_bytes()));
        bytes.resize(56, 0);
        bytes
    }

    /// The game's reflected name of `name`, as the fake host serves the
    /// typed `CurrentTransform` by it.
    fn keen(name: &str) -> &'static str {
        // The test stands in for the game's reflection, which names types so.
        // ast-grep-ignore: engine-names-stay-in-loader
        Box::leak(format!("keen::{name}").into_boxed_str())
    }

    /// The reflected types a `CurrentTransform` reads by, as server 26430253
    /// lays them out.
    fn transform_types() -> Vec<Type<'static>> {
        let field = |name, offset, ty: &str| Field::new(name, offset, keen(ty));
        let xyz = |ty, step| {
            vec![
                field("x", 0, ty),
                field("y", step, ty),
                field("z", 2 * step, ty),
            ]
        };
        let ty = |name, kind, size, align| Type::new(keen(name), kind, size, align);
        vec![
            ty("sint64", Kind::Sint64, 8, 8),
            ty("float32", Kind::Float32, 4, 4),
            ty("ecs::Component", Kind::Struct, 1, 1),
            ty("WorldPosition", Kind::Struct, 24, 8).with_fields(xyz("sint64", 8)),
            ty("float3", Kind::Struct, 12, 4).with_fields(xyz("float32", 4)),
            ty("Quaternion", Kind::Struct, 16, 4).with_fields(vec![
                field("x", 0, "float32"),
                field("y", 4, "float32"),
                field("z", 8, "float32"),
                field("w", 12, "float32"),
            ]),
            ty("WorldTransform", Kind::Struct, 56, 8).with_fields(vec![
                field("position", 0, "WorldPosition"),
                field("orientation", 24, "Quaternion"),
                field("scale", 40, "float3"),
            ]),
            ty("ecs::CurrentTransform", Kind::Struct, 56, 8)
                .with_inner(keen("ecs::Component"))
                .with_fields(vec![field("transform", 0, "WorldTransform")]),
        ]
    }

    fn join(loader: &Installed, entity: Entity, key: PlayerKey) {
        let _handled = loader.raise::<PlayerJoined>(&PlayerData {
            header: Header::of::<PlayerData>(),
            slot: 0,
            entity,
            key,
        });
    }

    #[test]
    fn the_enemy_comes_once_a_hosted_game_ten_metres_ahead_of_its_first_player_facing_them() {
        let (ann, key) = (Entity(5), PlayerKey::new(7));
        let host = FakeHost::new().world().inventory_requests();
        let current = host.hash(facts::CURRENT_TRANSFORM.id()).unwrap();
        let typed = fnv1a32(keen("ecs::CurrentTransform").as_bytes());
        let loader = host
            .types(transform_types())
            .type_hash("ecs::CurrentTransform", typed)
            .player(PlayerInfo {
                header: Header::of::<PlayerInfo>(),
                slot: 0,
                entity: ann,
                key,
                permissions: 0,
                name_len: 0,
                name: [0; 64],
            })
            .component(ann, current, &standing([40, 12, -3], FRAC_PI_2))
            .component(ann, typed, &standing([40, 12, -3], FRAC_PI_2))
            .install();
        // SAFETY: the exports of this module's `declare_mod!(Joins)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.faults());
        assert!(
            loader
                .features()
                .iter()
                .filter(
                    |(name, off)| [GIFTS, ENEMY_AT_JOIN].contains(&name.as_str()) && off.is_none()
                )
                .count()
                == 2,
            "{:?}",
            loader.features()
        );
        let recipe: KnowledgeId = Gifts::new(None, None, Some(prop(3)))
            .chest_recipe
            .unwrap()
            .into();
        loader.set_knowledge(
            &[(
                Some(ann),
                KnowledgeInfo {
                    header: Header::of::<KnowledgeInfo>(),
                    id: recipe,
                    kind: KnowledgeInfo::FLAGS,
                    scope: KnowledgeInfo::PLAYER,
                    reserved: 0,
                    value: 0,
                    flags: 0,
                },
            )],
            &[],
        );

        join(&loader, Entity(6), PlayerKey::new(8));
        let _ran = loader.tick(STEP);
        assert!(loader.spawns().is_empty(), "the first player left");
        join(&loader, ann, key);
        join(&loader, ann, key);
        let _ran = loader.tick(STEP);
        let spawns = loader.spawns();
        assert_eq!(
            spawns.len(),
            1,
            "to the next player, once however often they join"
        );
        let wolf = spawns[0].1;
        let want: [i64; 3] = [50 << 32, 14 << 32, -3 << 32];
        assert!(
            wolf.position
                .iter()
                .zip(want)
                .all(|(got, want)| (got - want).abs() < 1 << 22),
            "within a millimetre: {:?}",
            wolf.position
        );
        assert_eq!(wolf.level, 0, "the level the enemy's template holds");
        let faces = ahead(wolf.orientation);
        assert!(close(faces, [-1.0, 0.0]), "{faces:?}");

        let _handled = loader.raise::<WorldLoaded>(&());
        join(&loader, ann, key);
        let _ran = loader.tick(STEP);
        assert_eq!(loader.spawns().len(), 2, "again in the next hosted game");

        let gifts = loader.inventory_requests().len();
        assert_eq!(gifts, 2 * 4, "an altar and a chest at each of four joins");
        let taught = loader.grants();
        let taught: Vec<_> = taught.iter().map(|&(who, id, ..)| (who, id)).collect();
        assert_eq!(
            taught,
            [Entity(6), ann, ann, ann].map(|who| (Some(who), recipe)),
            "the chest's recipe at each join"
        );
    }

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
        // A turn that takes +z exactly onto the vertical, length 0.
        assert_eq!(ahead([0.5, 0.5, -0.5, 0.5]), [0.0, 1.0]);
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
