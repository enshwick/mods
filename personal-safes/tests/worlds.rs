//! Owners per world: each world keeps its own file, and the mod-wide file
//! from before moves into the first world that loads without owners.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::events::{ShutdownStarted, WorldLoaded};
    use enshwick::test::FakeHost;
    use enshwick::{Entity, ItemId, PlayerKey};
    use enshwick_personal_safes::access::snapshots;

    /// A loader that knows the item and recipe layouts the Safe needs.
    fn host() -> FakeHost {
        let mut host = FakeHost::new()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2);
        for (ty, hash) in [
            (enshwick::content::types::ITEM_INFO, 10),
            (enshwick::content::types::ITEM_REGISTRY_RESOURCE, 11),
            (enshwick::content::types::RECIPE_REGISTRY_RESOURCE, 12),
        ] {
            host = host.type_hash(ty.id(), hash);
        }
        for (id, size) in [
            ("item_stat", 0x28),
            ("recipe", 0xa8),
            ("recipe_ingredient", 0x18),
            ("recipe_stack", 0x18),
        ] {
            host = host.type_size(id, size);
        }
        for (id, at) in [
            ("item_info.id", 0),
            ("item_info.object", 4),
            ("item_info.name", 0x48),
            ("item_info.icon_image", 0x58),
            ("item_info.description", 0x68),
            ("item_info.flags", 0x78),
            ("item_info.pid_entity", 0x80),
            ("item_info.ui_values", 0x90),
            ("item_stat.label", 0),
            ("item_stat.format", 4),
            ("item_stat.value", 8),
            ("item_stat.signed", 0x24),
            ("recipe.guid", 0),
            ("recipe.id", 0x10),
            ("recipe.workshop_guid", 0x14),
            ("recipe.workshop", 0x24),
            ("recipe.input", 0x28),
            ("recipe.output", 0x30),
            ("recipe.completion", 0x5c),
            ("recipe.required_props", 0x38),
            ("recipe.debug_name", 0x40),
            ("recipe_registry.recipes", 0),
            ("recipe_stack.item", 0x10),
            ("recipe_stack.count", 0x14),
            ("item_registry.refs", 0),
            ("item_registry.names", 0x38),
        ] {
            host = host.constant(id, at);
        }
        host
    }

    /// The component every container has, with each kind's marker, so a
    /// Safe with its item counts as placed.
    fn containers(host: FakeHost) -> FakeHost {
        [
            ("inventory_setup", 3),
            ("player_input", 4),
            ("npc", 5),
            ("animal", 6),
            ("destroy_on_loot", 7),
            ("loot_fill", 8),
            ("resource_node", 9),
        ]
        .into_iter()
        .fold(host, |host, (id, hash)| host.type_hash(id, hash))
    }

    #[test]
    fn the_old_owners_file_moves_into_the_world_that_loads() {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-worlds-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("owners.toml"),
            "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\nname = \"Ann\"\n",
        )
        .unwrap();
        let loader = host()
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-aa")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
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
        assert!(dir.join("worlds/world0-aa/owners.toml").exists());
        assert!(!dir.join("owners.toml").exists());
        assert!(dir.join("owners.toml.moved").exists());
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(snapshots().owners.load().len(), 1, "the world's own file");
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_removed_safe_keeps_its_owner_on_disk_until_the_world_saves() {
        use enshwick::abi::{BuildingData, Header};
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
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-cc")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        assert_eq!(
            snapshots().seen(Entity(900), (4, 0, 4)),
            Some((PlayerKey::new(0xff), (4, 0, 4)))
        );
        let removed = BuildingData {
            header: Header::of::<BuildingData>(),
            entity: Entity(900),
            player: Entity(10),
            item: ItemId::new(0),
            kind: BuildingData::PICKED_UP,
            replay: 0,
            template: [0; 16],
            position: [0.0; 3],
        };
        assert!(loader.raise::<BuildingRemoved>(&removed) >= 1);
        assert!(snapshots().owners.load().is_empty(), "gone from the checks");
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("00000000000000ff"),
            "kept on disk while the world has not saved"
        );
        // Placed again elsewhere, as when a Safe moves: written at once.
        let placed = BuildingData {
            kind: BuildingData::PLACED,
            entity: Entity(901),
            item: ItemId::from_guid(enshwick_personal_safes::content::SAFE.guid()),
            position: [10.0, 0.0, 10.0],
            ..removed
        };
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
        let item = ItemId::from_guid(enshwick_personal_safes::content::SAFE.guid())
            .get()
            .to_le_bytes();
        let loader = containers(host())
            .placed(3, Entity(700), [20.0, 1.0, 20.0])
            .component(Entity(700), 2, &item)
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-bb")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
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
                Entity(10),
                stranger,
            )])));
        let stock = Interaction {
            kind: Kind::Stock,
            player: Entity(10),
            target: Entity(700),
            item: None,
            position: None,
        };
        assert!(
            enshwick_personal_safes::access::filter(&stock),
            "not seen yet"
        );
        // The sweep runs every 2 s of wall time, on the next tick after.
        std::thread::sleep(Duration::from_millis(2100));
        let _ran = loader.tick(Duration::from_millis(100));
        assert!(
            !enshwick_personal_safes::access::filter(&stock),
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
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
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
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-dd")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
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
            (Entity(10), owner),
            (Entity(12), stranger),
        ])));
        snapshots().placed(Entity(700), (40, 2, 40), owner.key, "Ann");
        assert!(
            snapshots().refused_seen(Entity(700), Entity(12)),
            "vanilla Safes"
        );
        assert!(snapshots().may_manage((40, 2, 40), owner, Entity(10)));
        assert!(!snapshots().may_manage((40, 2, 40), stranger, Entity(12)));
        assert!(loader.extend_point(|access: &Access, may: &bool| {
            match access.action {
                SafeAction::USE if access.player == PlayerKey::new(3) => Verdict::Set(true),
                SafeAction::REMOVE | SafeAction::MANAGE => Verdict::Refuse,
                _ => Verdict::Set(*may),
            }
        }));
        assert!(
            !snapshots().refused_seen(Entity(700), Entity(12)),
            "the link lets them in"
        );
        assert!(
            !snapshots().may(
                owner.key,
                (40, 2, 40),
                Entity(700),
                Entity(10),
                Action::Remove
            ),
            "the link refuses even the owner"
        );
        assert!(snapshots().may(owner.key, (40, 2, 40), Entity(700), Entity(10), Action::Use));
        assert!(
            !snapshots().may_manage((40, 2, 40), owner, Entity(10)),
            "the access action hears the refusal too"
        );
        assert!(
            snapshots().refused_seen(Entity(700), Entity(14)),
            "a player not registered yet stays refused"
        );
        assert!(snapshots().removed(Entity(700)).is_some());
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    fn player(entity: u32, key: u64, name: &str) -> enshwick::abi::PlayerInfo {
        use enshwick::abi::{Header, PlayerInfo};
        let mut bytes = [0u8; 64];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: Entity(entity),
            key: PlayerKey::new(key),
            permissions: PlayerInfo::CAN_ACCESS_INVENTORIES,
            name_len: u8::try_from(name.len()).unwrap(),
            name: bytes,
        }
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
        let at = |x: f64| -> Vec<u8> {
            enshwick::ecs::fixed([x, 0.0, 0.0])
                .iter()
                .flat_map(|axis| axis.to_le_bytes())
                .collect()
        };
        let (ann, bob, cat) = (
            player(10, 1, "Ann"),
            player(11, 2, "Bob"),
            player(12, 3, "Cat"),
        );
        let loader = host()
            .player(ann)
            .player(bob)
            .player(cat)
            .component(Entity(10), 1, &at(0.0))
            .component(Entity(11), 1, &at(100.0))
            .component(Entity(12), 1, &at(20.0))
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-ee")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots().placed(Entity(700), (2, 0, 0), PlayerKey::new(1), "Ann");
        snapshots().placed(Entity(701), (42, 0, 0), PlayerKey::new(2), "Bob");
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
            "Cat may now open your Safe at (1, 0, 0), the nearest you manage within 4 m."
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
            "Ann may now open Bob's Safe at (21, 0, 0), the nearest you manage within 4 m."
        );
        assert_eq!(
            run(&cat, "list", ""),
            "Allowed at Bob's Safe at (21, 0, 0): Ann."
        );
        assert_eq!(
            access(&ann, 3, false),
            "You may not change who opens your Safe at (1, 0, 0)."
        );
        let owners = snapshots().owners.load();
        assert_eq!(owners[&(2, 0, 0)].allowed.len(), 1, "Cat stays allowed");
        assert_eq!(owners[&(42, 0, 0)].allowed[0].key, PlayerKey::new(1));
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_access_window_lists_players_for_the_safe_at_hand_own_first() {
        use enshwick::abi::{ActionCall, PlayerInfo};
        use enshwick::actions::sheet::decode;
        use enshwick::i18n::Language;

        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-window-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let at = |x: f64| -> Vec<u8> {
            enshwick::ecs::fixed([x, 0.0, 0.0])
                .iter()
                .flat_map(|axis| axis.to_le_bytes())
                .collect()
        };
        let mut root = player(10, 1, "Root");
        root.permissions |= PlayerInfo::CAN_KICK_BAN;
        let (bob, cat) = (player(11, 2, "Bob"), player(12, 3, "Cat"));
        let loader = host()
            .player(root)
            .player(bob)
            .player(cat)
            .component(Entity(10), 1, &at(0.0))
            .component(Entity(11), 1, &at(100.0))
            .component(Entity(12), 1, &at(200.0))
            .settings(
                "cost_multiplier = 2
admins_can_open = true
label_language = \"en\"
",
            )
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-ff")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        // Bob's Safe is nearer, the administrator's own a little farther.
        snapshots().placed(Entity(700), (1, 0, 0), PlayerKey::new(2), "Bob");
        snapshots().placed(Entity(701), (6, 0, 0), PlayerKey::new(1), "Root");

        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &root, Language::UK_UA.0, "", "")
                .unwrap(),
        );
        let access = sheet.iter().find(|e| e.id == "access").unwrap();
        assert_eq!(access.label, "Хто може відкривати цей сейф");
        assert_eq!(access.subject, "ваш сейф у (3, 0, 0)", "own Safe first");
        assert!(access.offered && access.hint);
        let names: Vec<&str> = access.players.iter().map(|p| p.1.as_str()).collect();
        assert_eq!(names, ["Bob", "Cat"]);

        let ran = loader
            .ask_actions(ActionCall::RUN, &root, 0, "access", "p0000000000000003	b1")
            .unwrap();
        assert_eq!(
            ran,
            "1Cat may now open your Safe at (3, 0, 0), the nearest you manage within 4 m."
        );
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
        assert_eq!(
            loader
                .ask_actions(ActionCall::HINTS, &bob, 0, "", "")
                .as_deref(),
            Some(""),
            "Bob stands at no Safe"
        );
        assert!(loader.faults().is_empty());
        let _gone = std::fs::remove_dir_all(&dir);
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
        let safe = ItemId::from_guid(enshwick_personal_safes::content::SAFE.guid());
        let loader = containers(host())
            .placed(3, Entity(700), [20.0, 1.0, 20.0])
            .component(Entity(700), 2, &safe.get().to_le_bytes())
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .data_dir(dir.to_str().unwrap())
            .world_key("world0-dd")
            .install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.raise::<WorldLoaded>(&()), 2, "the store and the mod");
        snapshots()
            .players
            .store(std::sync::Arc::new(rustc_hash::FxHashMap::from_iter([(
                Entity(10),
                Seen {
                    key: PlayerKey::new(3),
                    admin: false,
                },
            )])));
        let open = Interaction {
            kind: Kind::Open,
            player: Entity(10),
            target: Entity(900),
            item: Some(safe),
            position: Some([20.0, 1.0, 20.0]),
        };
        assert!(
            !enshwick_personal_safes::access::filter(&open),
            "a Safe whose owner is unknown stays shut"
        );
        let stock = Interaction {
            kind: Kind::Stock,
            target: Entity(700),
            item: None,
            position: None,
            ..open
        };
        // The sweep runs every 2 s of wall time, on the next tick after.
        std::thread::sleep(Duration::from_millis(2100));
        let _ran = loader.tick(Duration::from_millis(100));
        assert!(
            !enshwick_personal_safes::access::filter(&stock),
            "lends no stock either"
        );
        assert!(loader.raise::<WorldSaved>(&()) >= 1);
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            damaged,
            "kept for a fix"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }
}
