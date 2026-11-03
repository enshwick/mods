//! The mute action through the fake loader, as an administrator and a
//! player use it from the game's interface.

#[cfg(test)]
mod tests {
    use enshwick::PlayerKey;
    use enshwick::abi::{ActionCall, Header, PlayerInfo, Status};
    use enshwick::actions::sheet::decode;
    use enshwick_admin_tools::{enshwick_mod_init, enshwick_mod_query, enshwick_mod_ready};

    fn player(entity: u32, key: u64, name: &str, permissions: u8) -> PlayerInfo {
        let mut info = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: entity,
            entity: enshwick::Entity(entity),
            key: PlayerKey::new(key),
            permissions,
            name_len: u8::try_from(name.len()).unwrap(),
            name: [0; 64],
        };
        info.name[..name.len()].copy_from_slice(name.as_bytes());
        info
    }

    #[test]
    fn administrators_mute_and_unmute_a_player_and_no_one_else_may() {
        let admin = player(10, 1, "Ann", PlayerInfo::CAN_KICK_BAN);
        let bob = player(11, 2, "Bob", 0);
        // A mute is the running world's, so the server runs one.
        let loader = enshwick::test::FakeHost::new()
            .world()
            .world_key("world0")
            .player(admin)
            .player(bob)
            .install();
        // SAFETY: both functions are the exports of the mod under test.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let ask = |kind, who: &PlayerInfo, action: &str, args: &str| {
            loader.ask_actions(kind, who, 0, action, args).unwrap()
        };

        let sheet = decode(&ask(ActionCall::SHEET, &admin, "", ""));
        let muted = sheet
            .iter()
            .find(|e| e.id == "muted")
            .expect("the admin sees it");
        assert!(muted.per_player && muted.offered);
        assert!(
            muted.players.iter().all(|(_, _, on)| !on),
            "nobody muted yet"
        );
        assert!(
            decode(&ask(ActionCall::SHEET, &bob, "", "")).is_empty(),
            "a player never sees it"
        );

        let ran = ask(ActionCall::RUN, &bob, "muted", "p0000000000000001\tb1");
        assert!(
            ran.starts_with('0'),
            "the server refuses a player's run: {ran}"
        );
        assert!(loader.muted().is_empty());

        let ran = ask(ActionCall::RUN, &admin, "muted", "p0000000000000002\tb1");
        assert_eq!(ran, "1Bob is muted in chat.");
        assert_eq!(loader.muted(), [PlayerKey::new(2)]);
        let sheet = decode(&ask(ActionCall::SHEET, &admin, "", ""));
        let muted = sheet.iter().find(|e| e.id == "muted").unwrap();
        assert!(
            muted
                .players
                .iter()
                .any(|(key, _, on)| *key == PlayerKey::new(2) && *on)
        );

        let ran = ask(ActionCall::RUN, &admin, "muted", "p0000000000000001\tb1");
        assert_eq!(
            ran,
            "0Ann is an administrator or the host and cannot be muted."
        );

        let ran = ask(ActionCall::RUN, &admin, "muted", "p0000000000000002\tb0");
        assert_eq!(ran, "1Bob can write in chat again.");
        assert!(loader.muted().is_empty());
    }
}
