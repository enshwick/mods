//! Admin Tools through the fake loader: each action as an administrator, a
//! player and the console run it, the check at the door, the schedule and
//! the audit log.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use enshwick::PlayerKey;
    use enshwick::abi::{ActionCall, DaytimeInfo, Header, PlayerData, PlayerInfo, Side, Status};
    use enshwick::actions::sheet::decode;
    use enshwick::console;
    use enshwick::content::EngineFacts as _;
    use enshwick::events::{PlayerJoined, SettingsReplaced, WorldLoaded};
    use enshwick::test::{Installed, facts};
    use enshwick_admin_tools::{enshwick_mod_init, enshwick_mod_query, enshwick_mod_ready};

    const ADMIN: u8 = PlayerInfo::CAN_KICK_BAN;

    /// The fake's type hash of a health attribute set; past the world
    /// preset's own hashes.
    const HEALTH: u32 = 0x4000_0001;

    /// A health attribute set at `current` of `max`, in the fake's layout:
    /// its id, the values' offset at 4, flags at 6, count at 7, then the
    /// values with the maximum in slot 7.
    fn health(current: i32, max: i32) -> Vec<u8> {
        let mut bytes = 10u32.to_le_bytes().to_vec();
        bytes.extend(8u16.to_le_bytes());
        bytes.extend([1, 8]);
        bytes.extend(
            [current, 0, 0, 0, 0, 0, 0, max]
                .iter()
                .flat_map(|v| v.to_le_bytes()),
        );
        bytes
    }

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

    fn ann() -> PlayerInfo {
        player(10, 1, "Ann", ADMIN)
    }

    fn bob() -> PlayerInfo {
        player(11, 2, "Bob", 0)
    }

    /// A fresh data folder for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("admin-tools-{name}-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A dedicated server with Ann (admin) and Bob online, `settings` for
    /// the world, its data in `dir`, and the mod started with the world
    /// loaded.
    fn server(settings: &str, dir: &Path) -> Installed {
        server_with(settings, dir, &[])
    }

    /// [`server`] with `more` players online beside Ann and Bob.
    fn server_with(settings: &str, dir: &Path, more: &[PlayerInfo]) -> Installed {
        let host = enshwick::test::FakeHost::new().side(Side::SERVER).world();
        let transform = host.hash(facts::CURRENT_TRANSFORM.id()).unwrap();
        let mut host = host
            .type_hash("attribute_set.health", HEALTH)
            .type_hash("attribute_set.stamina", HEALTH + 1)
            .type_hash("attribute_set.mana", HEALTH + 2)
            .type_hash("attribute_set.level", HEALTH + 3)
            .type_hash("attribute_set.experience", HEALTH + 4)
            .constant("ecs.level_data_offset", 4)
            .constant("ecs.level_flags", 6)
            .constant("ecs.attribute_count", 7)
            .constant("attribute.health", 10)
            .constant("attribute.health_max_slot", 7)
            .constant("attribute.stamina", 20)
            .constant("attribute.stamina_max_slot", 2)
            .constant("attribute.mana", 30)
            .constant("attribute.mana_max_slot", 1)
            .constant("attribute.level", 40)
            .constant("attribute.experience", 50)
            .constant("attribute.experience_needed_slot", 2)
            .world_key("world0")
            .data_dir(dir.to_str().unwrap())
            .settings(settings);
        for (who, place) in [(ann(), [40, 12, -3]), (bob(), [0, 20, 7])] {
            host = host
                .player(who)
                .template(who.entity, [7; 16])
                .component(who.entity, transform, &standing(place))
                .component(who.entity, HEALTH, &health(150, 200));
        }
        for who in more {
            host = host.player(*who);
        }
        let loader = host.install();
        // SAFETY: the exports of the mod under test.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader.set_daytime(DaytimeInfo {
            header: Header::of::<DaytimeInfo>(),
            time: 0,
            day_start: 6 * 3_600_000_000_000,
            day_end: 18 * 3_600_000_000_000,
            day_length: 1_800_000_000_000,
            night_length: 720_000_000_000,
            speed: 1.0,
            flags: 0,
        });
        loader.set_weather([1.0, 0.0, 0.0, 0.0], &[]);
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        assert!(loader.raise::<WorldLoaded>(&()) >= 1);
        loader
    }

    /// A `CurrentTransform` standing at `[x, y, z]` metres.
    fn standing([x, y, z]: [i64; 3]) -> Vec<u8> {
        [x << 32, y << 32, z << 32]
            .iter()
            .flat_map(|c| c.to_le_bytes())
            .collect()
    }

    fn run(loader: &Installed, who: &PlayerInfo, action: &str, args: &str) -> String {
        loader
            .ask_actions(ActionCall::RUN, who, 0, action, args)
            .unwrap()
    }

    fn joins(loader: &Installed, who: &PlayerInfo) {
        let data = PlayerData {
            header: Header::of::<PlayerData>(),
            slot: who.slot,
            entity: who.entity,
            key: who.key,
        };
        assert!(loader.raise::<PlayerJoined>(&data) >= 1);
        // The check runs after the step, at the scheduler's next turn.
        let _ran = loader.tick(Duration::from_secs(1));
    }

    #[test]
    fn a_player_sees_only_the_reads_and_an_administrator_sees_every_action() {
        let dir = scratch("sheet");
        let loader = server("", &dir);
        let ids = |who: &PlayerInfo| -> Vec<String> {
            decode(
                &loader
                    .ask_actions(ActionCall::SHEET, who, 0, "", "")
                    .unwrap(),
            )
            .into_iter()
            .map(|entry| entry.id)
            .collect()
        };
        assert_eq!(ids(&bob()), ["online", "motd"]);
        assert_eq!(ids(&ann()).len(), 15);
        assert!(run(&loader, &bob(), "kick", "p0000000000000001").starts_with('0'));
    }

    #[test]
    fn the_world_s_rights_give_an_action_to_a_permission_and_take_one_away() {
        let dir = scratch("rights");
        let loader = server(
            "[rights]\nheal = \"CanEditBase\"\nonline = \"nobody\"\n",
            &dir,
        );
        let builder = player(12, 3, "Cid", PlayerInfo::CAN_EDIT_BASE);
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &builder, 0, "", "")
                .unwrap(),
        );
        let ids: Vec<&str> = sheet.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["heal", "motd"]);
        assert!(
            run(&loader, &ann(), "online", "").starts_with('0'),
            "nobody includes admins"
        );
        assert_eq!(
            console::run("enshwick.admin-tools online").as_deref(),
            Ok("2 online: Ann (0 min), Bob (0 min)"),
            "the console always may"
        );
        assert_eq!(
            console::run("adl online").as_deref(),
            Ok("2 online: Ann (0 min), Bob (0 min)"),
            "the alias names the mod on the console"
        );
    }

    #[test]
    fn an_administrator_kicks_a_player_but_not_another_administrator() {
        let dir = scratch("kick");
        let loader = server("", &dir);
        assert_eq!(
            run(&loader, &ann(), "kick", "p0000000000000002"),
            "1Bob was kicked."
        );
        assert_eq!(loader.removed(), [2]);
        let reply = run(&loader, &ann(), "kick", "p0000000000000001");
        assert_eq!(
            reply,
            "0Ann is an administrator or the host and cannot be removed."
        );
    }

    #[test]
    fn an_administrator_bans_a_player_online_and_lifts_the_ban_while_they_are_away() {
        let dir = scratch("ban");
        let loader = server("", &dir);
        assert_eq!(
            run(&loader, &ann(), "banned", "p0000000000000002\tb1"),
            "1Bob is banned."
        );
        assert_eq!(loader.banned(), [PlayerKey::new(2)]);
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &ann(), 0, "", "")
                .unwrap(),
        );
        let banned = sheet.iter().find(|entry| entry.id == "banned").unwrap();
        assert!(
            banned
                .players
                .iter()
                .any(|(key, _, on)| *key == PlayerKey::new(2) && *on),
            "the list keeps Bob while he is away"
        );
        assert_eq!(
            run(&loader, &ann(), "banned", "p0000000000000002\tb0"),
            "1Bob is no longer banned."
        );
        assert_eq!(
            run(&loader, &ann(), "banned", "p0000000000000001\tb1"),
            "0Ann is an administrator or the host and cannot be removed."
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_ban_kept_from_an_earlier_run_bans_the_account_again_when_it_joins() {
        let dir = scratch("kept-ban");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("accounts.toml"),
            "[[bans]]\nkey = \"0000000000000002\"\nname = \"Bob\"\nby = \"Ann\"\nat = 0\n",
        )
        .unwrap();
        let loader = server("", &dir);
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &ann(), 0, "", "")
                .unwrap(),
        );
        let banned = sheet.iter().find(|entry| entry.id == "banned").unwrap();
        assert!(
            banned
                .players
                .iter()
                .any(|(key, _, on)| *key == PlayerKey::new(2) && *on),
            "the kept ban reads: {:?}",
            banned.players
        );
        joins(&loader, &ann());
        assert!(loader.banned().is_empty());
        joins(&loader, &bob());
        assert_eq!(
            loader.banned(),
            [PlayerKey::new(2)],
            "banned at the door: {:?}",
            loader.logs()
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_allow_list_keeps_out_an_account_not_on_it_and_lets_administrators_in() {
        let dir = scratch("allow");
        let cid = player(12, 3, "Cid", 0);
        let loader = server_with("allowlist = true\n", &dir, &[cid]);
        assert_eq!(
            run(&loader, &ann(), "allowed", "p0000000000000002\tb1"),
            "1Bob may join while the allow-list is on."
        );
        joins(&loader, &bob());
        joins(&loader, &ann());
        assert!(
            loader.removed().is_empty(),
            "Bob is on the list, and administrators always join"
        );
        joins(&loader, &cid);
        assert_eq!(loader.removed(), [3], "Cid is not on the list");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_player_who_joins_reads_the_message_of_the_day_and_can_ask_for_it_again() {
        let dir = scratch("motd");
        let loader = server("motd = \"Welcome to the Isle\"\n", &dir);
        joins(&loader, &bob());
        assert!(
            loader
                .chat()
                .iter()
                .chain(&loader.notices())
                .any(|(key, text)| { *key == PlayerKey::new(2) && text == "Welcome to the Isle" })
        );
        assert_eq!(run(&loader, &bob(), "motd", ""), "1Welcome to the Isle");
    }

    #[test]
    fn a_restart_warning_reaches_every_player_and_stops_at_zero() {
        let dir = scratch("restart");
        let loader = server("restart_text = \"Back soon.\"\n", &dir);
        assert_eq!(
            run(&loader, &ann(), "restart", "n2"),
            "1Every player is warned of a restart in 2 min."
        );
        let said = |loader: &Installed| -> Vec<String> {
            loader
                .chat()
                .into_iter()
                .chain(loader.notices())
                .map(|(_, text)| text)
                .collect()
        };
        assert!(said(&loader).contains(&"The server restarts in 2 min. Back soon.".to_owned()));
        for _ in 0..70 {
            let _ran = loader.tick(Duration::from_secs(1));
        }
        assert!(said(&loader).contains(&"The server restarts in 1 min. Back soon.".to_owned()));
        assert_eq!(
            run(&loader, &ann(), "restart", "n0"),
            "1The restart warning is stopped."
        );
        assert_eq!(
            run(&loader, &ann(), "restart", "n0"),
            "0No restart warning is under way."
        );
    }

    #[test]
    fn announcements_come_in_turn_at_the_world_s_interval() {
        let dir = scratch("announce");
        let loader = server(
            "announcements = [\"Vote\", \"Rules\"]\nannounce_minutes = 1\n",
            &dir,
        );
        for _ in 0..125 {
            let _ran = loader.tick(Duration::from_secs(1));
        }
        let texts: Vec<String> = loader
            .chat()
            .into_iter()
            .chain(loader.notices())
            .filter(|(key, _)| *key == PlayerKey::new(1))
            .map(|(_, text)| text)
            .collect();
        assert_eq!(texts, ["Vote", "Rules"]);
    }

    #[test]
    fn teleports_go_to_bring_and_send_players_and_the_console_has_no_character() {
        let dir = scratch("teleport");
        let loader = server("", &dir);
        assert_eq!(
            run(&loader, &ann(), "bring", "p0000000000000002"),
            "1Bob is on the way to Ann."
        );
        assert_eq!(
            run(
                &loader,
                &ann(),
                "send",
                "p0000000000000001\tp0000000000000002"
            ),
            "1Ann is on the way to Bob."
        );
        assert_eq!(
            run(
                &loader,
                &ann(),
                "place",
                "p0000000000000002\tn100\tn50\tn-20"
            ),
            "1Bob is on the way to 100, 50, -20."
        );
        let moved: Vec<_> = loader
            .teleports()
            .iter()
            .map(|request| request.entity)
            .collect();
        assert_eq!(
            moved,
            [
                enshwick::Entity(11),
                enshwick::Entity(10),
                enshwick::Entity(11)
            ]
        );
        assert_eq!(
            console::run("enshwick.admin-tools goto Bob").as_deref(),
            Ok("Run this as a player in the world: the console has no character.")
        );
    }

    #[test]
    fn heal_restores_a_player_to_their_full_health_and_no_more() {
        let dir = scratch("heal");
        let loader = server("", &dir);
        assert_eq!(
            run(&loader, &ann(), "heal", "p0000000000000002"),
            "1Bob has full health."
        );
        let bytes = loader.component_bytes(bob().entity, HEALTH).unwrap();
        let current = i32::from_le_bytes(bytes[8..12].try_into().unwrap());
        assert_eq!(current, 200);
        assert!(run(&loader, &bob(), "heal", "p0000000000000002").starts_with('0'));
    }

    #[test]
    fn the_time_of_day_and_the_weather_change_from_their_actions() {
        let dir = scratch("world");
        let loader = server("", &dir);
        assert_eq!(
            run(&loader, &ann(), "time", "n6.5"),
            "1The time is now 06:30."
        );
        assert!(loader.daytime().is_some());
        assert_eq!(
            run(&loader, &ann(), "weather", "c1"),
            "1The weather changes to Rain."
        );
        assert_eq!(loader.forecasts().len(), 1);
    }

    #[test]
    fn every_change_goes_to_the_audit_log_and_reads_back_and_reads_stay_out() {
        let dir = scratch("audit");
        let loader = server("", &dir);
        let _reply = run(&loader, &ann(), "online", "");
        let _reply = run(&loader, &ann(), "kick", "p0000000000000002");
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut read = String::new();
        while std::time::Instant::now() < deadline && read.is_empty() {
            read = run(&loader, &ann(), "audit", "n10");
            if read.ends_with("empty.") {
                read.clear();
                std::thread::yield_now();
            }
        }
        assert!(
            read.contains("Ann (0000000000000001)\tinterface\tkick\tBob\tBob was kicked."),
            "{read}"
        );
        assert!(!read.contains("\tonline\t"), "reads stay out: {read}");
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_audit_log_is_off_where_the_world_turns_it_off() {
        let dir = scratch("audit-off");
        let loader = server("audit = \"off\"\n", &dir);
        let _reply = run(&loader, &ann(), "kick", "p0000000000000002");
        assert_eq!(
            run(&loader, &ann(), "audit", "n10"),
            "0The audit log is off in this world's settings."
        );
    }

    #[test]
    fn a_settings_change_while_the_world_runs_goes_to_the_audit_log_and_its_load_does_not() {
        let dir = scratch("audit-settings");
        let loader = server("audit = \"on\"\nmotd = \"Hi\"\n", &dir);
        loader.set_settings("settings", "audit = \"on\"\nmotd = \"Welcome\"\n");
        assert!(loader.raise::<SettingsReplaced>(&()) >= 1);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut read = String::new();
        while std::time::Instant::now() < deadline && !read.contains("motd") {
            read = run(&loader, &ann(), "audit", "n10");
            std::thread::yield_now();
        }
        assert!(read.contains("settings\tmotd = \"Welcome\""), "{read}");
        assert!(
            !read.contains("\"Hi\""),
            "the world's load is no change: {read}"
        );
        let _gone = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commands_under_adl_run_the_actions_from_chat_with_the_player_s_rights() {
        let dir = scratch("chat");
        let loader = server("", &dir);
        assert_eq!(
            loader
                .type_in_chat(&ann(), 0, "/adl muted Bob on")
                .as_deref(),
            Some("1Bob is muted in chat.")
        );
        assert_eq!(loader.muted(), [PlayerKey::new(2)]);
        assert_eq!(
            loader
                .type_in_chat(&bob(), 0, "/admin-tools online")
                .as_deref(),
            Some("12 online: Ann (0 min), Bob (0 min, muted)")
        );
        let refused = loader.type_in_chat(&bob(), 0, "/adl kick Ann").unwrap();
        assert!(refused.starts_with('0'), "{refused}");
        assert!(loader.removed().is_empty());
    }
}
