//! The panel beside a Safe's chest screen: the server tells the player's
//! client which Safe they opened and who may open it, and the client draws
//! a switch per player that runs the access action.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ActionCall, Header, PlayerInfo, Side, Status, UiExtension};
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
}
