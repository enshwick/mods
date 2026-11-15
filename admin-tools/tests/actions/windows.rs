//! The mod's windows as the server carries them to a player's client: the
//! panel, its sections, one player's window and the build log's changes.

use enshwick::PlayerKey;
use enshwick::abi::{BuildingData, Header, PlayerInfo};
use enshwick::actions::Value;
use enshwick::actions::window::{ButtonView, ItemView, MAX_LINE, View, decode};
use enshwick::events::{BuildingPlaced, ShutdownStarted};
use enshwick::i18n::Language;
use enshwick::test::Installed;
use enshwick::{Entity, ItemId};

use super::game::{
    ann, bob, file_until, hosted, player, run, scratch, server, server_with, without_inventories,
};

/// The window `id` on `who`'s client; panics with the windows they hold
/// when it is not among them.
pub(super) fn window(loader: &Installed, who: &PlayerInfo, id: &str) -> View {
    let held = loader.carried_windows(who, 0);
    let text = held
        .iter()
        .find(|(held, _)| held == id)
        .unwrap_or_else(|| panic!("no window {id} among {held:?}"));
    decode(&text.1).unwrap()
}

/// Each row of `view` as its text and its buttons' labels.
fn rows(view: &View) -> Vec<(String, Vec<String>)> {
    view.items
        .iter()
        .filter_map(|item| match item {
            ItemView::Row { text, buttons } => Some((
                text.clone(),
                buttons.iter().map(|button| button.label.clone()).collect(),
            )),
            _ => None,
        })
        .collect()
}

/// The button labelled `label` of the row of `view` whose text starts with
/// `row`.
fn button(view: &View, row: &str, label: &str) -> ButtonView {
    view.items
        .iter()
        .find_map(|item| match item {
            ItemView::Row { text, buttons } if text.starts_with(row) => {
                buttons.iter().find(|button| button.label == label).cloned()
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("no button {label} in a row {row}: {:?}", view.items))
}

/// The ids of the actions `view` draws whole.
fn whole(view: &View) -> Vec<&str> {
    view.items
        .iter()
        .filter_map(|item| match item {
            ItemView::Action(id) => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

/// The shape of `view` (its rows' button counts and its entries' count),
/// after asserting none of its texts was cut to [`MAX_LINE`].
fn whole_shape(view: &View, context: &str) -> (Vec<Option<usize>>, usize) {
    // A cut line ends within one character of the limit, at most 4 bytes.
    let fits = |text: &str| assert!(text.len() + 4 <= MAX_LINE, "{context}: cut {text}");
    fits(&view.title);
    let rows = view
        .items
        .iter()
        .map(|item| match item {
            ItemView::Text(text) => {
                fits(text);
                None
            }
            ItemView::Row { text, buttons } => {
                fits(text);
                for button in buttons {
                    fits(&button.label);
                }
                Some(buttons.len())
            }
            _ => None,
        })
        .collect();
    (rows, view.entries.len())
}

fn player_value(key: u64) -> Value {
    Value::Player {
        key: PlayerKey::new(key),
        name: String::new(),
    }
}

#[test]
fn the_panel_opens_for_an_administrator_with_a_row_to_open_each_section() {
    let dir = scratch("panel");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "panel", ""), "1");
    let panel = window(&loader, &ann(), "panel");
    assert_eq!(panel.title, "Admin Tools");
    assert_eq!(
        panel.items[0],
        ItemView::Text("Admin Tools: 2 online".into())
    );
    let open = vec!["Open".to_owned()];
    assert_eq!(
        rows(&panel),
        [
            ("Players".to_owned(), open.clone()),
            ("Bans and the allow-list".to_owned(), open.clone()),
            ("Server".to_owned(), open.clone()),
            ("Build log".to_owned(), open.clone()),
            ("Protected zones".to_owned(), open.clone()),
            ("Spawns and props".to_owned(), open),
        ]
    );
    let server = button(&panel, "Server", "Open");
    assert_eq!(
        (server.action.as_str(), &server.args[..]),
        ("open", &[Value::Choice(2)][..])
    );
    assert_eq!(run(&loader, &bob(), "panel", ""), "0You may not do that.");
    assert!(loader.carried_windows(&bob(), 0).is_empty());
    assert_eq!(
        enshwick::console::run("enshwick.admin-tools panel").as_deref(),
        Ok("Run this as a player in the world: the console has no character.")
    );
}

#[test]
fn the_panel_reads_in_ukrainian_for_a_player_whose_game_does() {
    let dir = scratch("panel-uk");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c0"), "1");
    let held = loader.carried_windows(&ann(), Language::UK_UA.0);
    let players = decode(&held[0].1).unwrap();
    assert_eq!(players.title, "Гравці");
    assert_eq!(
        rows(&players)[1].1,
        ["Дії", "Перейти", "Привести", "Вигнати"]
    );
}

#[test]
fn every_window_carries_whole_in_every_offered_language() {
    // A player reads in one language, so each language opens its own server.
    let shapes = |language: Language| {
        let dir = scratch(&format!("windows-{}", language.0));
        let loader = server("build_log = true\n", &dir);
        for (action, args) in [
            ("panel", ""),
            ("open", "c0"),
            ("open", "c1"),
            ("open", "c2"),
            ("open", "c3"),
            ("open", "c4"),
            ("open", "c5"),
            ("player", "p0000000000000002"),
        ] {
            assert_eq!(run(&loader, &ann(), action, args), "1", "{action}");
        }
        let held = loader.carried_windows(&ann(), language.0);
        assert_eq!(held.len(), 8, "{language:?}");
        ends(&loader);
        held.iter()
            .map(|(id, text)| {
                let context = format!("{language:?} {id}");
                let view = decode(text).unwrap_or_else(|| panic!("{context}: too big"));
                (id.clone(), whole_shape(&view, &context))
            })
            .collect::<Vec<_>>()
    };
    let english = shapes(Language::FALLBACK);
    for &language in others(Language::FALLBACK) {
        assert_eq!(shapes(language), english, "{language:?}");
    }
}

#[test]
fn the_players_window_lists_each_player_with_the_actions_used_most() {
    let dir = scratch("players-window");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c0"), "1");
    let players = window(&loader, &ann(), "players");
    assert_eq!(
        rows(&players),
        [
            (
                "Ann (0000000000000001, 0 min)".to_owned(),
                vec!["Actions".to_owned()]
            ),
            (
                "Bob (0000000000000002, 0 min)".to_owned(),
                ["Actions", "Go to", "Bring", "Kick"]
                    .map(str::to_owned)
                    .to_vec()
            ),
        ]
    );
    let kick = button(&players, "Bob", "Kick");
    assert_eq!(
        (kick.action.as_str(), &kick.args[..]),
        ("kick", &[player_value(2)][..])
    );
    // A press runs the action as any run, and the list shows again without Bob.
    assert_eq!(
        run(&loader, &ann(), "kick", "p0000000000000002"),
        "1Bob was kicked."
    );
    assert_eq!(loader.removed(), [2]);
}

#[test]
fn only_a_press_in_the_interface_shows_the_window_again_not_a_command_in_chat() {
    let dir = scratch("refresh-chat");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c0"), "1");
    // The mod is not told when a window closes, so a typed command must
    // not bring back one the player shut.
    assert_eq!(
        loader.type_in_chat(&ann(), 0, "/adm kick Bob").as_deref(),
        Some("1Bob was kicked.")
    );
    assert_eq!(rows(&window(&loader, &ann(), "players")).len(), 2);
}

#[test]
fn one_player_s_window_groups_every_action_on_them() {
    let dir = scratch("player-window");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "player", "p0000000000000002"), "1");
    let bob_s = window(&loader, &ann(), "player");
    assert_eq!(
        bob_s.items[0],
        ItemView::Text("Bob (0000000000000002)".into())
    );
    let labels = |names: &[&str]| {
        names
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        rows(&bob_s),
        [
            (
                "Move".to_owned(),
                labels(&["Go to", "Bring", "To my waypoint"])
            ),
            (
                "Move to".to_owned(),
                labels(&["To a player", "To a position"])
            ),
            (
                "Moderate".to_owned(),
                labels(&["Kick", "Ban", "Mute", "Unmute"])
            ),
            ("Health".to_owned(), labels(&["Heal", "Revive", "Set"])),
            ("Map".to_owned(), labels(&["Uncover"])),
            ("God mode".to_owned(), labels(&["On", "Off"])),
            (
                "Items and recipes".to_owned(),
                labels(&["Give", "Chest", "Teach", "Clear"])
            ),
        ]
    );
    let mute = button(&bob_s, "Moderate", "Mute");
    assert_eq!(
        (mute.action.as_str(), &mute.args[..]),
        ("muted", &[player_value(2), Value::Toggle(true)][..])
    );
    // A ban asks for its reason in a dialog: the button gives only the player.
    let ban = button(&bob_s, "Moderate", "Ban");
    assert_eq!(
        (ban.action.as_str(), &ban.args[..]),
        ("ban", &[player_value(2)][..])
    );
    assert!(
        bob_s
            .entries
            .iter()
            .any(|entry| entry.id == "ban" && entry.args.len() == 2)
    );
}

#[test]
fn a_viewer_s_own_window_offers_no_kick_or_ban_of_themselves() {
    let dir = scratch("own-window");
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "player", "p0000000000000001"), "1");
    let own = window(&loader, &ann(), "player");
    let moderate = rows(&own)
        .into_iter()
        .find(|(text, _)| text == "Moderate")
        .unwrap();
    assert_eq!(moderate.1, ["Mute", "Unmute"]);
    let moves = rows(&own)
        .into_iter()
        .find(|(text, _)| text == "Move")
        .unwrap();
    assert_eq!(moves.1, ["To my waypoint"], "no go to or bring of oneself");
}

#[test]
fn a_press_shows_again_every_window_it_changed_not_only_the_last_opened() {
    let dir = scratch("refresh-many");
    let loader = server_with("", &dir, &[player(13, 4, "Eve", 0)]);
    assert_eq!(run(&loader, &ann(), "open", "c0"), "1");
    assert_eq!(run(&loader, &ann(), "player", "p0000000000000002"), "1");
    // The panel opens beside them and leaves them open.
    assert_eq!(run(&loader, &ann(), "panel", ""), "1");
    assert!(run(&loader, &ann(), "kick", "p0000000000000004").starts_with('1'));
    let listed: Vec<String> = rows(&window(&loader, &ann(), "players"))
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert_eq!(listed.len(), 2, "Eve left the list: {listed:?}");
    assert!(run(&loader, &ann(), "kick", "p0000000000000002").starts_with('1'));
    assert_eq!(rows(&window(&loader, &ann(), "players")).len(), 1);
}

#[test]
fn the_accounts_window_says_the_lists_do_not_read_rather_than_none_banned() {
    let dir = scratch("accounts-unread");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("accounts.toml"), "bans = 7\n").unwrap();
    let loader = server("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c1"), "1");
    let accounts = window(&loader, &ann(), "accounts");
    assert_eq!(accounts.items.len(), 1, "{:?}", accounts.items);
    assert!(
        matches!(&accounts.items[0], ItemView::Text(text) if text.starts_with("The ban list and allow-list do not read")),
        "{:?}",
        accounts.items
    );
}

#[test]
fn a_window_leaves_out_the_actions_a_game_build_without_inventories_does_not_declare() {
    let dir = scratch("window-no-inventories");
    let loader = without_inventories("", &dir);
    assert_eq!(run(&loader, &ann(), "player", "p0000000000000002"), "1");
    let items = rows(&window(&loader, &ann(), "player"))
        .into_iter()
        .find(|(text, _)| text == "Items and recipes")
        .unwrap();
    assert_eq!(items.1, ["Teach"], "give, chest and clear need inventories");
}

#[test]
fn a_window_shows_a_player_only_the_actions_the_world_s_rights_give_them() {
    let dir = scratch("window-rights");
    let rights = "[rights]\npanel = \"everyone\"\nopen = \"everyone\"\nplayer = \"everyone\"\nheal = \"everyone\"\n";
    let loader = server(rights, &dir);
    assert_eq!(run(&loader, &bob(), "player", "p0000000000000001"), "1");
    let ann_s = window(&loader, &bob(), "player");
    assert_eq!(
        rows(&ann_s),
        [("Health".to_owned(), vec!["Heal".to_owned()])]
    );
    assert_eq!(run(&loader, &bob(), "open", "c1"), "1");
    let accounts = window(&loader, &bob(), "accounts");
    assert!(
        whole(&accounts).is_empty(),
        "no ban, allow-list or account action"
    );
    assert_eq!(run(&loader, &bob(), "open", "c0"), "1");
    assert_eq!(
        rows(&window(&loader, &bob(), "players"))[0],
        (
            "Ann (0000000000000001, 0 min)".to_owned(),
            vec!["Actions".to_owned()]
        ),
        "no go to, bring or kick"
    );
}

#[test]
fn the_accounts_window_lists_each_ban_with_its_reason_and_lifts_it_with_one_press() {
    let dir = scratch("accounts-window");
    let loader = server("", &dir);
    assert_eq!(
        run(
            &loader,
            &ann(),
            "ban",
            "p0000000000000002\ttEmptied the chests"
        ),
        "1Bob is banned."
    );
    assert_eq!(run(&loader, &ann(), "open", "c1"), "1");
    let accounts = window(&loader, &ann(), "accounts");
    // The forms come first: the platform cuts a window too big from its end.
    assert_eq!(accounts.items[0], ItemView::Action("ban".into()));
    let count = ItemView::Text("Admin Tools: 1 banned".into());
    let at = accounts.items.iter().position(|item| *item == count);
    assert_eq!(at, Some(4), "after the forms: {:?}", accounts.items);
    assert_eq!(whole(&accounts), ["ban", "allowed", "account"]);
    assert!(accounts.items.contains(&ItemView::Text(
        "The allow-list is off in the world's settings: everyone not banned may join.".into()
    )));
    let lift = button(
        &accounts,
        "Bob (0000000000000002): Emptied the chests. By Ann",
        "Unban",
    );
    assert_eq!(
        (lift.action.as_str(), &lift.args[..]),
        ("banned", &[player_value(2), Value::Toggle(false)][..])
    );
    assert_eq!(
        run(&loader, &ann(), "banned", "p0000000000000002\tb0"),
        "1Bob is no longer banned."
    );
    let after = window(&loader, &ann(), "accounts");
    assert!(
        after
            .items
            .contains(&ItemView::Text("Admin Tools: 0 banned".into()))
    );
    assert!(rows(&after).is_empty(), "the lifted ban left the window");
}

#[test]
fn the_server_window_offers_times_weathers_announcements_and_the_restart_warning() {
    let dir = scratch("server-window");
    let loader = server("motd = \"Hi\"\n", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c2"), "1");
    let view = window(&loader, &ann(), "server");
    assert_eq!(
        view.items[0],
        ItemView::Text("Message of the day: Hi".into())
    );
    assert_eq!(whole(&view), ["setmotd", "say", "restart", "time"]);
    let noon = button(&view, "Time of day", "12:00");
    assert_eq!(
        (noon.action.as_str(), &noon.args[..]),
        ("time", &[Value::Text("12:00".into())][..])
    );
    assert_eq!(run(&loader, &ann(), "time", "t12:00"), "1Time: 12:00");
    let rain = button(&view, "Weather", "Rain");
    assert_eq!(
        (rain.action.as_str(), &rain.args[..]),
        ("weather", &[Value::Choice(1)][..])
    );
    assert_eq!(rows(&view).last().unwrap().1, ["Save now", "Back up"]);
    assert!(
        rows(&view)
            .iter()
            .all(|(text, _)| !text.starts_with("Restart")),
        "no warning is under way"
    );
    assert!(run(&loader, &ann(), "restart", "n10").starts_with('1'));
    let warned = window(&loader, &ann(), "server");
    let stop = button(&warned, "Restart warning under way", "Stop");
    assert_eq!(
        (stop.action.as_str(), &stop.args[..]),
        ("restart", &[Value::Number(0.0)][..])
    );
    // The message set from the window shows in it at once.
    assert!(run(&loader, &ann(), "setmotd", "tWelcome").starts_with('1'));
    let after = window(&loader, &ann(), "server");
    assert_eq!(
        after.items[0],
        ItemView::Text("Message of the day: Welcome".into())
    );
}

#[test]
fn a_game_a_player_hosts_has_no_restart_warning_in_its_server_window() {
    let dir = scratch("server-window-hosted");
    let loader = hosted("", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c2"), "1");
    let view = window(&loader, &ann(), "server");
    assert_eq!(whole(&view), ["setmotd", "say", "time"]);
    assert!(
        rows(&view)
            .iter()
            .all(|(text, _)| !text.starts_with("Restart"))
    );
}

#[test]
fn a_lookup_from_the_interface_shows_the_changes_in_a_window_with_a_way_there() {
    let dir = scratch("changes-window");
    let loader = server("build_log = true\n", &dir);
    assert_eq!(run(&loader, &ann(), "open", "c3"), "1");
    let builds = window(&loader, &ann(), "builds");
    assert_eq!(
        builds.items[0],
        ItemView::Text("The build log is on.".into())
    );
    assert_eq!(whole(&builds), ["who"]);
    let placed = BuildingData {
        header: Header::of::<BuildingData>(),
        entity: Entity(900),
        character: bob().entity,
        item: ItemId::new(0),
        kind: BuildingData::PLACED,
        replay: 0,
        template: enshwick::content::Guid([0; 16]),
        position: [1.0, 20.0, 7.0],
    };
    assert!(loader.raise::<BuildingPlaced>(&placed) >= 1);
    let _log = file_until(&dir, "builds.log", |text| !text.is_empty());
    assert_eq!(
        run(&loader, &ann(), "who", "n60"),
        "1Admin Tools: 1 changes within 60 m, newest first"
    );
    let changes = window(&loader, &ann(), "changes");
    let (text, buttons) = &rows(&changes)[0];
    assert!(
        text.ends_with("UTC: Bob (0000000000000002) placed a building at 1, 20, 7"),
        "{text}"
    );
    assert_eq!(buttons, &["Go there"]);
    let there = button(&changes, text, "Go there");
    assert_eq!(there.action, "place");
    assert_eq!(
        there.args,
        [
            player_value(1),
            Value::Number(1.0),
            Value::Number(20.0),
            Value::Number(7.0)
        ]
    );
}

#[test]
fn the_zones_section_lists_each_zone_with_its_switch_and_carries_whole_when_full() {
    let longest = |n: usize| format!("{n:02}{}", "z".repeat(30));
    let shapes = |language: Language| {
        let dir = scratch(&format!("zones-window-{}", language.0));
        let loader = server("", &dir);
        // A window lists 16 rows; each name as long as the action takes.
        for n in 0..16 {
            let line = format!("t{}\tn1000\tn2000\tn2000", longest(n));
            let reply = run(&loader, &ann(), "zonebox", &line);
            assert!(reply.starts_with('1'), "{language:?} zone {n}: {reply}");
        }
        let off = format!("t{}\tb0", longest(0));
        assert!(run(&loader, &ann(), "zoneswitch", &off).starts_with('1'));
        assert_eq!(run(&loader, &ann(), "open", "c4"), "1");
        let held = loader.carried_windows(&ann(), language.0);
        ends(&loader);
        let view = decode(&held[0].1).unwrap_or_else(|| panic!("{language:?}: too big"));
        whole_shape(&view, &format!("{language:?} zones"))
    };
    let english = shapes(Language::FALLBACK);
    assert_eq!(english.0.iter().filter(|row| row.is_some()).count(), 16);
    for &language in others(Language::FALLBACK) {
        assert_eq!(shapes(language), english, "{language:?}");
    }
}

#[test]
fn the_zones_section_lists_the_zones_only_to_a_viewer_who_may_read_them() {
    let dir = scratch("zones-window-rights");
    let loader = server("[rights]\nzones = \"nobody\"\n", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    assert_eq!(run(&loader, &ann(), "open", "c4"), "1");
    let zones = window(&loader, &ann(), "zones");
    assert_eq!(whole(&zones), ["zone", "zonebox"]);
    assert_eq!(zones.items.len(), 2, "{:?}", zones.items);
}

#[test]
fn a_list_longer_than_a_window_says_how_many_rows_it_left_out() {
    let dir = scratch("zones-window-more");
    let loader = server("", &dir);
    for n in 0..17 {
        let line = format!("tz{n}\tn10");
        assert!(run(&loader, &ann(), "zone", &line).starts_with('1'));
    }
    assert_eq!(run(&loader, &ann(), "open", "c4"), "1");
    let zones = window(&loader, &ann(), "zones");
    assert_eq!(rows(&zones).len(), 16);
    assert!(
        matches!(zones.items.last(), Some(ItemView::Text(text)) if text == "And 1 more."),
        "{:?}",
        zones.items.last()
    );
}

#[test]
fn a_zone_s_buttons_switch_it_the_other_way_and_remove_it() {
    let dir = scratch("zones-buttons");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    assert_eq!(run(&loader, &ann(), "open", "c4"), "1");
    let zones = window(&loader, &ann(), "zones");
    assert_eq!(
        rows(&zones),
        [(
            "market (on): 10 m around 40, -3".to_owned(),
            vec!["Off".to_owned(), "Remove".to_owned()]
        )]
    );
    let off = button(&zones, "market", "Off");
    assert_eq!(
        (off.action.as_str(), &off.args[..]),
        (
            "zoneswitch",
            &[Value::Text("market".into()), Value::Toggle(false)][..]
        )
    );
    assert!(run(&loader, &ann(), "zoneswitch", "tmarket\tb0").starts_with('1'));
    let shown = window(&loader, &ann(), "zones");
    assert_eq!(
        rows(&shown)[0].1,
        ["On", "Remove"],
        "shown again after the change"
    );
}

#[test]
fn a_zone_change_is_on_disk_once_the_world_ends() {
    let dir = scratch("zones-ended");
    let loader = server("", &dir);
    assert!(run(&loader, &ann(), "zone", "tmarket\tn10").starts_with('1'));
    ends(&loader);
    let kept = std::fs::read_to_string(dir.as_ref().join("zones.toml")).unwrap_or_default();
    assert!(kept.contains("market"), "{kept:?}");
}

/// Ends `loader`'s world, which writes the mod's world files under their
/// lock, so no background write lands after its test removes the folder.
fn ends(loader: &Installed) {
    assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
}

/// The offered languages but `language`, so no two servers of a test share
/// a scratch folder.
fn others(language: Language) -> impl Iterator<Item = &'static Language> {
    Language::OFFERED
        .iter()
        .filter(move |&&other| other != language)
}
