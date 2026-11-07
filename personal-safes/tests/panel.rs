//! The panel beside a Safe's chest screen: the server tells the player's
//! client which Safe they opened and who may open it, and the client draws
//! a switch per player that runs the access action.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side, Status, UiExtension};
    use enshwick::actions::ActionTarget;
    use enshwick::content::{EngineFacts, Guid};
    use enshwick::events::WorldLoaded;
    use enshwick::net::Peer;
    use enshwick::test::facts::{self, Fact};
    use enshwick::test::{Drawn, FakeHost, Input};
    use enshwick::ui::HudScreenType;
    use enshwick::{Entity, PlayerKey};
    use enshwick_personal_safes::access::snapshots;
    use enshwick_personal_safes::content;
    use enshwick_personal_safes::panel::{Listed, Shown};

    /// The panel's channel as the loader names it.
    const CHANNEL: &str = "panel@1";

    /// The accepted interaction's engine ids, laid out at offsets of their
    /// own: the event, then its player's, its target's and its offer's
    /// fields.
    const ACCEPT: [&str; 4] = [
        "accept_offered_interaction_event",
        "accept_offered_interaction_event.guest_entity_id",
        "accept_offered_interaction_event.host_entity_id",
        "accept_offered_interaction_event.offer_id",
    ];

    /// A loader that knows the item and recipe layouts the Safe needs and
    /// raises the accepted interaction.
    fn host() -> FakeHost {
        FakeHost::new()
            .world()
            .items()
            .recipes()
            .workshops()
            .vanilla_items(&[(Guid([9; 16]), content::CHEST)])
            .game_event_type(ACCEPT[0], 0x0a11, 24)
            .constant(ACCEPT[1], 8)
            .constant(ACCEPT[2], 12)
            .constant(ACCEPT[3], 16)
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
    }

    fn player(entity: u32, key: u64, name: &str) -> PlayerInfo {
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

    /// The accepted interaction of `character` with `chest`, as `host`
    /// lays it out.
    fn accepted(host: &FakeHost, character: u32, chest: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; 24];
        for (field, entity) in [(ACCEPT[1], character), (ACCEPT[2], chest)] {
            let at = host.offset(field).unwrap();
            bytes[at..at + 4].copy_from_slice(&entity.to_le_bytes());
        }
        bytes
    }

    /// A current transform at the world's origin, laid out as `host` places
    /// it.
    fn origin(host: &FakeHost) -> Vec<u8> {
        let offset = |fact: Fact| host.offset(fact.id()).unwrap();
        vec![
            0u8;
            offset(facts::CURRENT_TRANSFORM_TRANSFORM)
                + offset(facts::WORLD_TRANSFORM_POSITION)
                + 24
        ]
    }

    /// What the panel's channel told each client so far.
    fn told(loader: &enshwick::test::Installed) -> Vec<(PlayerKey, Shown)> {
        loader
            .net_sent()
            .into_iter()
            .filter(|(channel, _, _)| channel == CHANNEL)
            .map(|(_, to, bytes)| (to, postcard::from_bytes(&bytes).unwrap()))
            .collect()
    }

    #[test]
    fn the_server_tells_the_owner_their_safe_s_list_and_clears_it_at_another_chest() {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-panel-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let (ann, cat) = (player(10, 1, "Ann"), player(12, 3, "Cat"));
        let host = host();
        let (own, plain, by_cat) = (
            accepted(&host, 10, 700),
            accepted(&host, 10, 800),
            accepted(&host, 12, 700),
        );
        let (transform, here) = (
            host.hash(facts::CURRENT_TRANSFORM.id()).unwrap(),
            origin(&host),
        );
        let loader = host
            .player(ann)
            .player(cat)
            .component(Entity(10), transform, &here)
            .net_peer(Peer::Player(PlayerKey::new(1)))
            .net_peer(Peer::Player(PlayerKey::new(3)))
            .data_dir(dir.join("worlds/world0-pp").to_str().unwrap())
            .world_key("world0-pp")
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
        snapshots().placed(Entity(700), (2, 0, 0), PlayerKey::new(1), "Ann");

        assert_eq!(
            loader.features(),
            [
                ("panel".to_owned(), None),
                ("world store owners".to_owned(), None)
            ]
        );
        assert_eq!(loader.raise_game_event(0x0a11, &own), 1);
        let cat_off = Listed {
            key: PlayerKey::new(3),
            name: "Cat".into(),
            allowed: false,
        };
        assert_eq!(
            told(&loader),
            [(
                PlayerKey::new(1),
                Shown::Safe {
                    at: (2, 0, 0),
                    players: vec![cat_off.clone()],
                }
            )]
        );

        let ran = loader
            .ask_actions(ActionCall::RUN, &ann, 0, "access", "p0000000000000003\tb1")
            .unwrap();
        assert!(ran.starts_with("1Cat may now open your Safe"), "{ran}");
        assert_eq!(
            told(&loader).last(),
            Some(&(
                PlayerKey::new(1),
                Shown::Safe {
                    at: (2, 0, 0),
                    players: vec![Listed {
                        allowed: true,
                        ..cat_off
                    }],
                }
            )),
            "a fresh list after the switch"
        );

        assert_eq!(loader.raise_game_event(0x0a11, &by_cat), 1);
        assert_eq!(told(&loader).len(), 2, "Cat may open but not manage it");
        assert_eq!(loader.raise_game_event(0x0a11, &plain), 1);
        assert_eq!(
            told(&loader).last(),
            Some(&(PlayerKey::new(1), Shown::Nothing)),
            "another chest clears the panel"
        );
        assert_eq!(loader.raise_game_event(0x0a11, &plain), 1);
        assert_eq!(told(&loader).len(), 3, "said once");
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    /// A running server where Ann stands at the origin with her Safes 1 m
    /// and 6 m away and Bob's 6 m away, and the accepted interactions of
    /// her opening the far one and the near one.
    fn three_safes(dir: &std::path::Path) -> (enshwick::test::Installed, [Vec<u8>; 2]) {
        let host = host();
        let opened = [accepted(&host, 10, 701), accepted(&host, 10, 700)];
        let (transform, here) = (
            host.hash(facts::CURRENT_TRANSFORM.id()).unwrap(),
            origin(&host),
        );
        let loader = host
            .player(player(10, 1, "Ann"))
            .player(player(12, 3, "Cat"))
            .component(Entity(10), transform, &here)
            .net_peer(Peer::Player(PlayerKey::new(1)))
            .data_dir(dir.join("worlds/world0-pp").to_str().unwrap())
            .world_key("world0-pp")
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
        snapshots().placed(Entity(700), (2, 0, 0), PlayerKey::new(1), "Ann");
        snapshots().placed(Entity(701), (12, 0, 0), PlayerKey::new(1), "Ann");
        snapshots().placed(Entity(702), (0, 0, 12), PlayerKey::new(2), "Bob");
        (loader, opened)
    }

    #[test]
    fn a_switch_acts_only_on_the_open_safe_and_a_run_without_one_on_the_nearest() {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-personal-safes-target-{}",
            std::process::id()
        ));
        let _gone = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ann = player(10, 1, "Ann");
        let (loader, [open_far, open_near]) = three_safes(&dir);
        let cat_on = "p0000000000000003\tb1";

        let far = ActionTarget::at([6.0, 0.0, 0.0]);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, far)
            .unwrap();
        assert_eq!(
            ran, "0Open the Safe at (6, 0, 0) to change who may open it.",
            "never a Safe the server did not see Ann open"
        );
        assert_eq!(told(&loader), [], "no Safe open, so no list to send");
        let open_at = |loader: &enshwick::test::Installed| match told(loader).last() {
            Some((_, Shown::Safe { at, .. })) => Some(*at),
            _ => None,
        };
        assert_eq!(loader.raise_game_event(0x0a11, &open_far), 1);
        let near = ActionTarget::at([1.0, 0.0, 0.0]);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, near)
            .unwrap();
        assert_eq!(
            ran, "0Open the Safe at (1, 0, 0) to change who may open it.",
            "never another Safe Ann manages than the one open"
        );
        assert!(told(&loader).len() > 1, "the refusal resends the list");
        assert_eq!(open_at(&loader), Some((12, 0, 0)), "of the open Safe");
        let before = told(&loader).len();
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, far)
            .unwrap();
        assert!(
            ran.starts_with("1Cat may now open your Safe at (6, 0, 0)."),
            "{ran}"
        );
        assert_eq!(
            told(&loader).len(),
            before + 1,
            "a switch that runs sends one list"
        );
        let beside = ActionTarget::at([6.4, 0.0, 0.0]);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, beside)
            .unwrap();
        assert_eq!(
            ran, "0Cat may already open your Safe at (6, 0, 0).",
            "a target in the next cell finds the Safe kept beside it"
        );
        let ran = loader
            .ask_actions(ActionCall::RUN, &ann, 0, "access", cat_on)
            .unwrap();
        assert!(
            ran.starts_with("1Cat may now open your Safe at (1, 0, 0)."),
            "{ran}"
        );
        assert_eq!(loader.raise_game_event(0x0a11, &open_near), 1);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, far)
            .unwrap();
        assert_eq!(
            ran, "0Open the Safe at (6, 0, 0) to change who may open it.",
            "a panel left on the Safe opened before is stale"
        );
        assert_eq!(
            open_at(&loader),
            Some((2, 0, 0)),
            "and hears the open Safe's list at once"
        );
        let bobs = ActionTarget::at([0.0, 0.0, 6.0]);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, bobs)
            .unwrap();
        assert_eq!(
            ran, "0You manage no Safe at (0, 0, 6).",
            "never a Safe Ann does not manage"
        );
        let empty = ActionTarget::at([0.0, 0.0, 1.0]);
        let ran = loader
            .ask_action_at(&ann, 0, "access", cat_on, empty)
            .unwrap();
        assert_eq!(
            ran, "0You manage no Safe at (0, 0, 1).",
            "never the nearest instead"
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_client_draws_switches_only_for_a_safe_and_a_switch_runs_the_access_action() {
        let loader = host().side(Side::CLIENT).net_peer(Peer::Server).install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "no Safe open"
        );
        let players = vec![
            Listed {
                key: PlayerKey::new(2),
                name: "Bob".into(),
                allowed: false,
            },
            Listed {
                key: PlayerKey::new(3),
                name: "Cat".into(),
                allowed: true,
            },
        ];
        let safe = Shown::Safe {
            at: (2, 0, 0),
            players,
        };
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &safe));
        let toggles = |drawn: &[(u32, Vec<Drawn>)]| -> Vec<(String, bool)> {
            drawn
                .iter()
                .flat_map(|(edge, list)| {
                    assert_eq!(*edge, UiExtension::RIGHT);
                    list.iter().filter_map(|widget| match widget {
                        Drawn::Toggle { label, on, .. } => Some((label.clone(), *on)),
                        _ => None,
                    })
                })
                .collect()
        };
        let drawn = loader.draw_screen(HudScreenType::StorageChest);
        assert!(
            matches!(&drawn[0].1[0], Drawn::Label { text, .. } if text == "Who may open this Safe"),
            "{drawn:?}"
        );
        assert_eq!(
            toggles(&drawn),
            [("Bob".to_owned(), false), ("Cat".to_owned(), true)]
        );
        assert!(
            loader.draw_screen(HudScreenType::Backpack).is_empty(),
            "only beside the chest screen"
        );

        loader.input("Bob", Input::Toggle(true));
        let drawn = loader.draw_screen(HudScreenType::StorageChest);
        assert_eq!(
            loader.action_runs(),
            [("access".to_owned(), "p0000000000000002\tb1".to_owned())]
        );
        assert_eq!(
            loader.action_targets(),
            [Some(ActionTarget::at([1.0, 0.0, 0.0]))],
            "the open Safe's centre"
        );
        assert_eq!(toggles(&drawn)[0], ("Bob".to_owned(), true));
        assert_eq!(
            toggles(&loader.draw_screen(HudScreenType::StorageChest))[0],
            ("Bob".to_owned(), true),
            "shown on until the server answers"
        );

        // Closed past the grace of the last message, then opened again.
        std::thread::sleep(std::time::Duration::from_millis(1300));
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "a chest opened again waits for the server's word"
        );
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &safe));
        assert_eq!(
            toggles(&loader.draw_screen(HudScreenType::StorageChest)).len(),
            2
        );
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &Shown::Nothing));
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "a plain chest shows no panel"
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    #[test]
    fn a_plain_chest_opened_right_after_a_safe_shows_no_list_before_the_server_s_word() {
        let loader = host().side(Side::CLIENT).net_peer(Peer::Server).install();
        // SAFETY: the exports of the mod's `declare_mod!`.
        let status = unsafe {
            loader.start(
                enshwick_personal_safes::enshwick_mod_query,
                enshwick_personal_safes::enshwick_mod_init,
                enshwick_personal_safes::enshwick_mod_ready,
            )
        };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let safe = Shown::Safe {
            at: (2, 0, 0),
            players: vec![Listed {
                key: PlayerKey::new(2),
                name: "Bob".into(),
                allowed: false,
            }],
        };
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &safe));
        assert!(!loader.draw_screen(HudScreenType::StorageChest).is_empty());
        // Closed and another chest opened within the grace, its Nothing still
        // on the way.
        std::thread::sleep(std::time::Duration::from_millis(400));
        assert!(
            loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "the Safe's list was for the chest closed before"
        );
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &safe));
        assert!(
            !loader.draw_screen(HudScreenType::StorageChest).is_empty(),
            "a fresh message for the chest open now"
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }

    /// Each switch the panel draws, by label.
    fn switches(drawn: &[(u32, Vec<Drawn>)]) -> Vec<(String, bool)> {
        drawn
            .iter()
            .flat_map(|(_, list)| {
                list.iter().filter_map(|widget| match widget {
                    Drawn::Toggle { label, on, .. } => Some((label.clone(), *on)),
                    _ => None,
                })
            })
            .collect()
    }

    #[test]
    fn a_stalled_frame_keeps_the_panel_and_an_unanswered_switch_falls_back() {
        // A client build without the server's event of an accepted
        // interaction still hears the server.
        let loader = FakeHost::new()
            .world()
            .items()
            .recipes()
            .workshops()
            .vanilla_items(&[(Guid([9; 16]), content::CHEST)])
            .settings("cost_multiplier = 2\nadmins_can_open = true\nlabel_language = \"en\"\n")
            .side(Side::CLIENT)
            .net_peer(Peer::Server)
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
        assert!(
            format!("{:?}", loader.logs()).contains("follows no opened chest"),
            "{:?}",
            loader.logs()
        );
        let safe = Shown::Safe {
            at: (2, 0, 0),
            players: vec![Listed {
                key: PlayerKey::new(2),
                name: "Bob".into(),
                allowed: false,
            }],
        };
        assert!(loader.net_deliver(CHANNEL, Peer::Server, &safe));
        let frame = || {
            let _hud = loader.draw_frame();
            switches(&loader.draw_screen(HudScreenType::StorageChest))
        };
        assert_eq!(frame(), [("Bob".to_owned(), false)]);

        loader.input("Bob", Input::Toggle(true));
        assert_eq!(frame(), [("Bob".to_owned(), true)]);
        std::thread::sleep(std::time::Duration::from_millis(1500));
        assert_eq!(
            frame(),
            [("Bob".to_owned(), true)],
            "one long frame is a stall, not a chest opened again"
        );
        std::thread::sleep(std::time::Duration::from_millis(1600));
        assert_eq!(
            frame(),
            [("Bob".to_owned(), false)],
            "with no answer the switch shows the server's word again"
        );

        let _closed = loader.draw_frame();
        assert!(
            frame().is_empty(),
            "a frame without the panel closed the chest; the list was for it"
        );
        assert!(loader.faults().is_empty(), "{:?}", loader.faults());
    }
}
