//! Owners per world: each world keeps its own file, and the mod-wide file
//! from before moves into the first world that loads without owners.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::content::EngineFacts;
    use enshwick::content::Guid;
    use enshwick::events::{ShutdownStarted, WorldLoaded};
    use enshwick::test::FakeHost;
    use enshwick::test::facts::{self, Fact};
    use enshwick::{Entity, ItemId, PlayerKey};
    use enshwick_personal_safes::access::snapshots;
    use enshwick_personal_safes::content;

    /// An entity of the fake world by its engine id.
    const fn entity(id: u32) -> Entity {
        Entity::from_raw(id).unwrap()
    }

    /// A loader that knows the item and recipe layouts the Safe needs.
    fn host() -> FakeHost {
        bare()
            .workshops()
            .vanilla_items(&[(Guid([9; 16]), content::CHEST)])
    }

    /// The same without the vanilla chest in the item registry.
    fn bare() -> FakeHost {
        FakeHost::new().world().items().recipes()
    }

    #[test]
    fn without_the_wooden_chest_the_mod_turns_off_naming_it() {
        let loader = bare()
            .workshops()
            .vanilla_items(&[(Guid([7; 16]), "Prop_Storage_24_T1_Box")])
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_ne!(status, Status::OK);
        let faults = loader.faults();
        assert!(
            faults
                .iter()
                .any(|fault| fault.contains("the wooden chest") && fault.contains(content::CHEST)),
            "{faults:?}"
        );
    }

    /// The hash `ty` has on `host`.
    fn hash(host: &FakeHost, ty: Fact) -> u32 {
        host.hash(ty.id()).unwrap()
    }

    /// A current transform `x` metres along, laid out as `host` places it.
    fn at(host: &FakeHost, x: f64) -> Vec<u8> {
        let offset = |fact: Fact| host.offset(fact.id()).unwrap();
        let place =
            offset(facts::CURRENT_TRANSFORM_TRANSFORM) + offset(facts::WORLD_TRANSFORM_POSITION);
        let mut bytes = vec![0u8; place + 24];
        for (axis, value) in enshwick::ecs::fixed([x, 0.0, 0.0]).iter().enumerate() {
            bytes[place + 8 * axis..][..8].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    /// The component every container has and each kind's marker, so a Safe
    /// with its item counts as placed.
    fn containers(host: FakeHost) -> FakeHost {
        host.inventory().container_kinds()
    }

    #[test]
    fn a_world_s_owners_load_with_the_world() {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-worlds-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("worlds/world0-aa")).unwrap();
        std::fs::write(
            dir.join("worlds/world0-aa/owners.toml"),
            "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\nname = \"Ann\"\n",
        )
        .unwrap();
        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .data_dir(dir.join("worlds/world0-aa").to_str().unwrap())
            .world_key("world0-aa")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(snapshots().owners.load().is_empty(), "no world yet");
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        let owners = snapshots().owners.load();
        assert_eq!(
            owners.get(&(1, 2, 3)).map(|safe| safe.owner),
            Some(PlayerKey::new(0xff))
        );
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(snapshots().owners.load().len(), 1, "the world's own file");
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_safe_keeps_its_owner_on_disk_until_the_world_saves() {
        use enshwick::abi::BuildingData;
        use enshwick::events::{BuildingPlaced, BuildingRemoved, WorldSaved};

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-removed-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        let world = dir.join("worlds/world0-cc");
        std::fs::create_dir_all(&world).unwrap();
        let file = world.join("owners.toml");
        std::fs::write(
            &file,
            "[[safes]]\ncell = [4, 0, 4]\nowner = \"00000000000000ff\"\nname = \"Ann\"\n",
        )
        .unwrap();
        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .data_dir(dir.join("worlds/world0-cc").to_str().unwrap())
            .world_key("world0-cc")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(
            snapshots().seen(entity(900), (4, 0, 4)),
            Some((PlayerKey::new(0xff), (4, 0, 4)))
        );
        let removed = enshwick::abi::headed!(BuildingData {
            entity: entity(900),
            character: entity(10),
            item: ItemId::new(0),
            kind: BuildingData::PICKED_UP,
            replay: 0,
            template: Guid([0; 16]),
            position: [0.0; 3],
        });
        assert!(loader.raise::<BuildingRemoved>(&removed) >= 1);
        assert!(snapshots().owners.load().is_empty(), "gone from the checks");
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "kept on disk while the world has not saved"
        );
        // Placed again elsewhere, as when a Safe moves: written at once.
        let placed = enshwick::abi::headed!(BuildingData {
            kind: BuildingData::PLACED,
            entity: entity(901),
            item: ItemId::from_guid(content::SAFE.guid()),
            position: [10.0, 0.0, 10.0],
            ..removed
        });
        let write = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while std::fs::metadata(&file).unwrap().modified().unwrap() == write
            && std::time::Instant::now() < deadline
        {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "a write before the world saves still keeps the removed Safe's owner"
        );
        assert!(loader.raise::<WorldSaved>(&()) >= 1);
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert!(
            !std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "written with the world"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_safe_nobody_looked_at_lends_its_stock_to_no_stranger() {
        use std::time::Duration;

        use enshwick::interactions::{Interaction, Kind};
        use enshwick_personal_safes::access::{Safe, Seen};

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-stock-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let item = ItemId::from_guid(content::SAFE.guid()).get().to_le_bytes();
        let host = containers(host());
        let (setup, used) = (
            hash(&host, facts::INVENTORY_SETUP),
            hash(&host, facts::USED_ITEM),
        );
        let loader = host
            .placed(setup, entity(700), [20.0, 1.0, 20.0])
            .component(entity(700), used, &item)
            .settings(
                "cost_multiplier = 2
admins_can_open = true
",
            )
            .data_dir(dir.join("worlds/world0-bb").to_str().unwrap())
            .world_key("world0-bb")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots().load_owners(rustc_hash::FxHashMap::from_iter([(
            (40, 2, 40),
            Safe::new(PlayerKey::new(1), "Ann"),
        )]));
        let stranger = Seen {
            key: PlayerKey::new(3),
            admin: false,
        };
        snapshots()
            .players
            .store(std::sync::Arc::new(rustc_hash::FxHashMap::from_iter([(
                entity(10),
                stranger,
            )])));
        let stock = Interaction::new(Kind::Stock, entity(10), entity(700));
        assert!(
            enshwick_personal_safes::access::filter(&stock),
            "not seen yet"
        );
        // The sweep runs every 2 s of the scheduler's clock.
        assert!(
            loader.drive_until(Duration::from_secs(3), || {
                !enshwick_personal_safes::access::filter(&stock)
            }),
            "the sweep found the Safe: {:?}",
            loader.logs()
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_character_the_players_snapshot_lacks_is_caught_up_at_the_next_tick() {
        use std::sync::atomic::Ordering;
        use std::time::Duration;

        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        snapshots().unregistered.store(true, Ordering::Relaxed);
        assert!(loader.tick(Duration::from_millis(100)) >= 1);
        assert!(
            !snapshots().unregistered.load(Ordering::Relaxed),
            "the snapshot was retaken"
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn other_mods_change_who_may_use_a_safe_at_its_point() {
        use rustc_hash::FxHashMap;
        use std::sync::Arc;

        use enshwick::points::Verdict;
        use enshwick_personal_safes::access::{Action, Seen};
        use enshwick_personal_safes::points::{Access, SafeAction};

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-point-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let loader = host()
            .settings(
                "cost_multiplier = 2
admins_can_open = true
",
            )
            .data_dir(dir.join("worlds/world0-dd").to_str().unwrap())
            .world_key("world0-dd")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.defined_points(), ["enshwick.personal-safes:access"]);
        let (owner, stranger) = (
            Seen {
                key: PlayerKey::new(1),
                admin: false,
            },
            Seen {
                key: PlayerKey::new(3),
                admin: false,
            },
        );
        snapshots().players.store(Arc::new(FxHashMap::from_iter([
            (entity(10), owner),
            (entity(12), stranger),
        ])));
        snapshots().placed(entity(700), (40, 2, 40), owner.key, "Ann");
        assert!(
            snapshots().refused_seen(entity(700), entity(12)),
            "vanilla Safes"
        );
        assert!(snapshots().may_manage((40, 2, 40), owner, entity(10)));
        assert!(!snapshots().may_manage((40, 2, 40), stranger, entity(12)));
        assert!(loader.extend_point(|access: &Access, may: &bool| {
            match access.action {
                SafeAction::USE if access.player == PlayerKey::new(3) => Verdict::Set(true),
                SafeAction::REMOVE | SafeAction::MANAGE => Verdict::Refuse,
                _ => Verdict::Set(*may),
            }
        }));
        assert!(
            !snapshots().refused_seen(entity(700), entity(12)),
            "the link lets them in"
        );
        assert!(
            !snapshots().may(
                owner.key,
                (40, 2, 40),
                entity(700),
                entity(10),
                Action::Remove
            ),
            "the link refuses even the owner"
        );
        assert!(snapshots().may(owner.key, (40, 2, 40), entity(700), entity(10), Action::Use));
        assert!(
            !snapshots().may_manage((40, 2, 40), owner, entity(10)),
            "the access action hears the refusal too"
        );
        assert!(
            snapshots().refused_seen(entity(700), entity(14)),
            "a player not registered yet stays refused"
        );
        assert!(snapshots().removed(entity(700)).is_some());
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    fn player(entity: u32, key: u64, name: &str) -> enshwick::abi::PlayerInfo {
        use enshwick::abi::PlayerInfo;
        let mut bytes = [0u8; PlayerInfo::NAME_MAX];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        enshwick::abi::headed!(PlayerInfo {
            slot: entity,
            entity: Entity::from_raw(entity).unwrap(),
            key: PlayerKey::new(key),
            permissions: PlayerInfo::CAN_ACCESS_INVENTORIES,
            name_len: u32::try_from(name.len()).unwrap(),
            name: bytes,
        })
    }

    #[test]
    fn each_safe_names_its_owner_in_the_language_of_the_game_that_placed_it() {
        use enshwick::abi::BuildingData;
        use enshwick::events::BuildingPlaced;
        use enshwick::i18n::Language;

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-labels-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let german = Language::from_code("de").unwrap();
        let loader = host()
            .player(player(10, 1, "Ann"))
            .player(player(11, 2, "Bob"))
            .player(player(12, 3, "Cat"))
            .player_language(PlayerKey::new(1), Language::UK_UA)
            .player_language(PlayerKey::new(2), german)
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .data_dir(dir.join("worlds/world0-ll").to_str().unwrap())
            .world_key("world0-ll")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        for (placer, safe, x) in [(10, 800, 10.0), (11, 801, 20.0), (12, 802, 30.0)] {
            let placed = enshwick::abi::headed!(BuildingData {
                entity: entity(safe),
                character: entity(placer),
                item: ItemId::from_guid(content::SAFE.guid()),
                kind: BuildingData::PLACED,
                replay: 0,
                template: Guid([0; 16]),
                position: [x, 0.0, 0.0],
            });
            assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
        }
        assert_eq!(
            loader.labels(),
            vec![
                (800, "Сейф · Ann".to_owned()),
                (801, "Tresor · Bob".to_owned()),
                (802, "Safe · Cat".to_owned()),
            ],
            "Cat's game runs no Enshwick, so English"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_safe_named_later_takes_its_owner_s_language_while_online_else_english() {
        use std::time::Duration;

        use enshwick::i18n::Language;
        use enshwick_personal_safes::access::Safe;

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-later-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let item = ItemId::from_guid(content::SAFE.guid()).get().to_le_bytes();
        let host = containers(host());
        let (setup, used) = (
            hash(&host, facts::INVENTORY_SETUP),
            hash(&host, facts::USED_ITEM),
        );
        let loader = host
            .placed(setup, entity(700), [20.0, 1.0, 20.0])
            .component(entity(700), used, &item)
            .placed(setup, entity(701), [30.0, 1.0, 30.0])
            .component(entity(701), used, &item)
            .player(player(10, 1, "Ann"))
            .player_language(PlayerKey::new(1), Language::UK_UA)
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .data_dir(dir.join("worlds/world0-ln").to_str().unwrap())
            .world_key("world0-ln")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots().load_owners(rustc_hash::FxHashMap::from_iter([
            ((40, 2, 40), Safe::new(PlayerKey::new(1), "Ann")),
            ((60, 2, 60), Safe::new(PlayerKey::new(2), "Bob")),
        ]));
        // The labels run every 2 s of the scheduler's clock.
        assert!(
            loader.drive_until(Duration::from_secs(5), || loader.labels().len() == 2),
            "both Safes named: {:?}",
            loader.logs()
        );
        assert_eq!(
            loader.labels(),
            vec![
                (700, "Сейф · Ann".to_owned()),
                (701, "Safe · Bob".to_owned()),
            ],
            "Bob is offline, so English"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_access_actions_act_on_the_nearest_safe_the_player_may_manage() {
        use enshwick::abi::ActionCall;
        use enshwick::points::Verdict;
        use enshwick_personal_safes::points::{Access, SafeAction};

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-manage-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (ann, bob, cat) = (
            player(10, 1, "Ann"),
            player(11, 2, "Bob"),
            player(12, 3, "Cat"),
        );
        let host = host();
        let transform = hash(&host, facts::CURRENT_TRANSFORM);
        let places = [0.0, 100.0, 20.0].map(|x| at(&host, x));
        let loader = host
            .player(ann)
            .player(bob)
            .player(cat)
            .component(entity(10), transform, &places[0])
            .component(entity(11), transform, &places[1])
            .component(entity(12), transform, &places[2])
            .settings(
                "cost_multiplier = 2
admins_can_open = true
",
            )
            .data_dir(dir.join("worlds/world0-ee").to_str().unwrap())
            .world_key("world0-ee")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots().placed(entity(700), (2, 0, 0), PlayerKey::new(1), "Ann");
        snapshots().placed(entity(701), (42, 0, 0), PlayerKey::new(2), "Bob");
        let run = |who, id, args: &str| {
            let reply = loader
                .ask_actions(ActionCall::RUN, who, 0, id, args)
                .unwrap();
            reply[1..].to_owned()
        };
        let access = |who, key: u64, on: bool| {
            run(who, "access", &format!("p{key:016x}\tb{}", u8::from(on)))
        };

        assert_eq!(
            access(&ann, 3, true),
            "Cat may now open your Safe at (1, 0, 0)."
        );
        assert_eq!(
            run(&cat, "list", ""),
            "No Safe you manage within 4 m.",
            "Bob's Safe is not Cat's to manage"
        );
        assert!(loader.extend_point(|access: &Access, may: &bool| {
            match (access.action, access.player) {
                (SafeAction::MANAGE, PlayerKey(3)) => Verdict::Set(true),
                (SafeAction::MANAGE, PlayerKey(1)) => Verdict::Refuse,
                _ => Verdict::Set(*may),
            }
        }));
        assert_eq!(
            access(&cat, 1, true),
            "Ann may now open Bob's Safe at (21, 0, 0)."
        );
        assert_eq!(
            run(&cat, "list", ""),
            "Allowed at Bob's Safe at (21, 0, 0): Ann."
        );
        assert_eq!(
            access(&ann, 3, false),
            "You may not change who opens your Safe at (1, 0, 0)."
        );
        let targeted = loader
            .ask_action_at(
                &ann,
                0,
                "access",
                "p0000000000000003\tb0",
                enshwick::actions::ActionTarget::at([1.0, 0.0, 0.0]),
            )
            .unwrap();
        assert_eq!(
            &targeted[1..],
            "You may not change who opens your Safe at (1, 0, 0).",
            "the open Safe names the link's refusal too"
        );
        let owners = snapshots().owners.load();
        assert_eq!(owners[&(2, 0, 0)].allowed.len(), 1, "Cat stays allowed");
        assert_eq!(owners[&(42, 0, 0)].allowed[0].key, PlayerKey::new(1));
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn access_changes_the_player_s_own_safe_at_hand_before_a_nearer_one() {
        use enshwick::abi::{ActionCall, PlayerInfo};

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-access-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut root = player(10, 1, "Root");
        root.permissions |= PlayerInfo::CAN_KICK_BAN;
        let (bob, cat) = (player(11, 2, "Bob"), player(12, 3, "Cat"));
        let host = host();
        let transform = hash(&host, facts::CURRENT_TRANSFORM);
        let places = [0.0, 100.0, 200.0].map(|x| at(&host, x));
        let loader = host
            .player(root)
            .player(bob)
            .player(cat)
            .component(entity(10), transform, &places[0])
            .component(entity(11), transform, &places[1])
            .component(entity(12), transform, &places[2])
            .settings(
                "cost_multiplier = 2
admins_can_open = true
",
            )
            .data_dir(dir.join("worlds/world0-ff").to_str().unwrap())
            .world_key("world0-ff")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        // Bob's Safe is nearer, the administrator's own a little farther.
        snapshots().placed(entity(700), (1, 0, 0), PlayerKey::new(2), "Bob");
        snapshots().placed(entity(701), (6, 0, 0), PlayerKey::new(1), "Root");

        let ran = loader
            .ask_actions(ActionCall::RUN, &root, 0, "access", "p0000000000000003	b1")
            .unwrap();
        assert_eq!(ran, "1Cat may now open your Safe at (3, 0, 0).");
        assert_eq!(
            snapshots().owners.load()[&(6, 0, 0)].allowed[0].key,
            PlayerKey::new(3)
        );
        assert!(snapshots().owners.load()[&(1, 0, 0)].allowed.is_empty());

        // Off again, and Cat may be switched off while away.
        let ran = loader
            .ask_actions(ActionCall::RUN, &root, 0, "access", "p0000000000000003	b0")
            .unwrap();
        assert!(ran.starts_with("1Cat may no longer open"), "{ran}");
        let bobs = loader
            .ask_actions(ActionCall::RUN, &bob, 0, "access", "p0000000000000003	b1")
            .unwrap();
        assert_eq!(
            bobs, "0No Safe you manage within 4 m.",
            "Bob stands at no Safe"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn shift_l_asks_the_server_who_may_open_the_safe_at_hand() {
        use std::time::Duration;

        use enshwick::abi::Side;
        use enshwick::controls::Key;

        let loader = host().side(Side::CLIENT).install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let key = Key::parse("Shift+L").unwrap();
        assert!(
            loader
                .controls()
                .iter()
                .any(|(name, label, keys)| name == "list"
                    && label == "Name who may open it"
                    && keys[0] == Some(key)),
            "{:?}",
            loader.controls()
        );
        assert_eq!(loader.keys(&[key], Duration::from_millis(16)), 1);
        assert_eq!(loader.action_runs(), [("list".to_owned(), String::new())]);
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn safes_stay_locked_while_their_owners_do_not_read() {
        use std::time::Duration;

        use enshwick::events::WorldSaved;
        use enshwick::interactions::{Interaction, Kind};
        use enshwick_personal_safes::access::Seen;

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-unread-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        let world = dir.join("worlds/world0-dd");
        std::fs::create_dir_all(&world).unwrap();
        let file = world.join("owners.toml");
        let damaged = "[[safes]]\ncell = [2, 0, 2]\nowner = \"00000000000000ff\"\nname = ";
        std::fs::write(&file, damaged).unwrap();
        let safe = ItemId::from_guid(content::SAFE.guid());
        let host = containers(host());
        let (setup, used) = (
            hash(&host, facts::INVENTORY_SETUP),
            hash(&host, facts::USED_ITEM),
        );
        let loader = host
            .placed(setup, entity(700), [20.0, 1.0, 20.0])
            .component(entity(700), used, &safe.get().to_le_bytes())
            .settings("cost_multiplier = 2\nadmins_can_open = true\n")
            .data_dir(dir.join("worlds/world0-dd").to_str().unwrap())
            .world_key("world0-dd")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots()
            .players
            .store(std::sync::Arc::new(rustc_hash::FxHashMap::from_iter([(
                entity(10),
                Seen {
                    key: PlayerKey::new(3),
                    admin: false,
                },
            )])));
        let open = Interaction::new(Kind::Open, entity(10), entity(900))
            .with_item(safe)
            .with_position([20.0, 1.0, 20.0]);
        assert!(
            !enshwick_personal_safes::access::filter(&open),
            "a Safe whose owner is unknown stays shut"
        );
        let stock = Interaction::new(Kind::Stock, entity(10), entity(700));
        assert!(
            enshwick_personal_safes::access::filter(&stock),
            "lent until the sweep has seen the Safe"
        );
        // The sweep runs every 2 s of the scheduler's clock.
        assert!(
            loader.drive_until(Duration::from_secs(3), || {
                !enshwick_personal_safes::access::filter(&stock)
            }),
            "lends no stock either"
        );
        assert!(loader.raise::<WorldSaved>(&()) >= 1);
        assert!(loader.raise::<WorldSaved>(&()) >= 1);
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            damaged,
            "kept for a fix"
        );
        let unsaved: Vec<String> = loader
            .logs()
            .into_iter()
            .map(|(_, line)| line)
            .filter(|line| line.starts_with("owners not saved"))
            .collect();
        assert_eq!(unsaved.len(), 1, "said once: {unsaved:?}");
        assert!(
            unsaved[0].starts_with("owners not saved until the world's owners file is fixed: "),
            "{unsaved:?}"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
