//! The mod's windows: the panel, its sections and the window of one
//! player, each a list of rows with buttons that run the mod's actions.
//!
//! The server builds a window for the one player who opens it, with only
//! the actions the world's rights give them, and the platform draws it on
//! their client. A window is a view: every button runs an action, which
//! the server checks again.

use std::sync::Arc;

use enshwick::actions::{Action, Arg, Outcome, Request, Row, Value, Window};
use enshwick::controls::Key;
use enshwick::world::Player;
use enshwick::{Off, PlayerKey, Relock as _};

use crate::builds::Change;
use crate::players::{ban_values, borrowed};
use crate::server::FORECASTS;
use crate::{Tools, ids, texts};

/// The key that opens the panel unless the player binds another.
const PANEL_KEY: &str = "Shift+F9";

/// The window ids.
const PANEL: &str = "panel";
const PLAYERS: &str = "players";
const ACCOUNTS: &str = "accounts";
const SERVER: &str = "server";
const BUILDS: &str = "builds";
const PLAYER: &str = "player";
const CHANGES: &str = "changes";

/// A section of the panel, each a window of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Section {
    Players,
    Accounts,
    Server,
    Builds,
}

/// The panel's sections: each its word in a command, which is also its
/// window's id, its label's key and the section.
const SECTIONS: [(&str, &str, Section); 4] = [
    (PLAYERS, texts::WINDOW_PLAYERS, Section::Players),
    (ACCOUNTS, texts::WINDOW_ACCOUNTS, Section::Accounts),
    (SERVER, texts::WINDOW_SERVER, Section::Server),
    (BUILDS, texts::WINDOW_BUILDS, Section::Builds),
];

/// The most rows of players or bans a window lists, so its text stays
/// within what the platform carries; the commands list the rest.
const MAX_ROWS: usize = 16;

/// The times of day the server window offers with one press.
const TIMES: [&str; 4] = ["06:00", "12:00", "18:00", "00:00"];

/// A window a player opened, which a change they make shows again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shown {
    /// A section of the panel.
    Section(Section),
    /// The window of the player with this account.
    Player(PlayerKey),
}

/// The actions that take a player online off the server.
const REMOVES: [&str; 4] = [ids::KICK, ids::BAN, ids::BANNED, ids::ACCOUNT];

impl Shown {
    /// The window's id: a player holds one window of each.
    fn id(self) -> &'static str {
        match self {
            Self::Section(Section::Players) => PLAYERS,
            Self::Section(Section::Accounts) => ACCOUNTS,
            Self::Section(Section::Server) => SERVER,
            Self::Section(Section::Builds) => BUILDS,
            Self::Player(_) => PLAYER,
        }
    }

    /// Whether a run of the action `id` changes what the window shows.
    fn stale_after(self, id: &str) -> bool {
        let lists: &[&str] = match self {
            Self::Section(Section::Players) | Self::Player(_) => &REMOVES,
            Self::Section(Section::Accounts) => {
                &[ids::BAN, ids::BANNED, ids::ALLOWED, ids::ACCOUNT]
            }
            Self::Section(Section::Server) => &[ids::SET_MOTD, ids::RESTART],
            Self::Section(Section::Builds) => &[],
        };
        lists.contains(&id)
    }
}

/// Declares the actions that open the windows, and the panel's key.
pub(crate) fn add(tools: &Arc<Tools>) -> Result<(), Off> {
    tools.add(
        Action::new(ids::PANEL, texts::ACTION_PANEL).help(texts::ACTION_PANEL_HELP),
        |tools, request| tools.open(request, None),
    )?;
    tools.add(
        Action::new(ids::OPEN, texts::ACTION_OPEN)
            .help(texts::ACTION_OPEN_HELP)
            .arg(Arg::choice(
                texts::ARG_SECTION,
                &SECTIONS.map(|(word, label, _)| (word, label)),
            )),
        |tools, request| match request.args {
            [Value::Choice(at)] => match SECTIONS.get(*at) {
                Some((.., section)) => tools.open(request, Some(Shown::Section(*section))),
                None => crate::players::bad_args(request),
            },
            _ => crate::players::bad_args(request),
        },
    )?;
    tools.add(
        Action::new(ids::PLAYER, texts::ACTION_PLAYER)
            .help(texts::ACTION_PLAYER_HELP)
            .arg(Arg::player(texts::ARG_PLAYER)),
        |tools, request| match request.args {
            [Value::Player { key, .. }] => tools.open(request, Some(Shown::Player(*key))),
            _ => crate::players::bad_args(request),
        },
    )?;
    // Without its key the panel still opens by command.
    if let Err(why) = Key::parse(PANEL_KEY)
        .map_err(Off::from)
        .and_then(|key| tools.actions.control(ids::PANEL, key))
    {
        enshwick::warn!("the panel has no key: {why}");
    }
    Ok(())
}

/// A button of a row: its label's key, its action and the action's first
/// values.
type Button<'a> = (&'a str, &'a str, &'a [Value]);

/// A window under way for one viewer: an item goes in only when the viewer
/// may run every action it names.
struct Build<'a> {
    tools: &'a Tools,
    viewer: &'a Player,
    window: Window,
}

impl<'a> Build<'a> {
    fn new(tools: &'a Tools, viewer: &'a Player, id: &str, title: &str) -> Self {
        Self {
            tools,
            viewer,
            window: Window::new(id, title),
        }
    }

    fn may(&self, id: &str) -> bool {
        self.tools.declared(id) && self.tools.admits(id, self.viewer)
    }

    fn text(mut self, key: &str, values: &[(&str, &str)]) -> Self {
        self.window = self.window.text(key, values);
        self
    }

    /// Adds the action `id` whole.
    fn action(mut self, id: &str) -> Self {
        if self.may(id) {
            self.window = self.window.action(id);
        }
        self
    }

    /// Adds a row reading `key` with the `buttons` the viewer may press,
    /// each a label, an action and its first values; a row left without a
    /// button stays out unless `bare` rows read on their own.
    fn row(
        mut self,
        key: &str,
        values: &[(&str, &str)],
        buttons: &[Button<'_>],
        bare: bool,
    ) -> Self {
        let mut row = Row::new(key, values);
        let mut any = false;
        for (label, action, args) in buttons {
            if self.may(action) {
                row = row.button(label, action, args);
                any = true;
            }
        }
        if any || bare {
            self.window = self.window.row(row);
        }
        self
    }
}

/// `player` as a button hands them to an action.
fn value(player: &Player) -> Value {
    Value::Player {
        key: player.key,
        name: player.name.clone(),
    }
}

impl Tools {
    /// Opens the panel, or the window `shown`, for the caller.
    fn open(&self, request: &Request<'_>, shown: Option<Shown>) -> Outcome {
        if request.player.key.0 == 0 {
            return Outcome::refused(request.say(texts::REPLY_NO_CHARACTER, &[]));
        }
        match self.show(request.player, shown) {
            Ok(()) => Outcome::done(""),
            Err(why) => Self::failed(request, ids::PANEL, &why),
        }
    }

    /// Builds the panel, or the window `shown`, for `viewer` and shows it;
    /// the window is remembered beside the others they hold, so their next
    /// change shows it again.
    fn show(&self, viewer: &Player, shown: Option<Shown>) -> Result<(), Off> {
        let window = match shown {
            None => self.panel(viewer),
            Some(Shown::Section(Section::Players)) => self.players_window(viewer),
            Some(Shown::Section(Section::Accounts)) => self.accounts_window(viewer),
            Some(Shown::Section(Section::Server)) => self.server_window(viewer),
            Some(Shown::Section(Section::Builds)) => self.builds_window(viewer),
            Some(Shown::Player(key)) => {
                let Some(player) = self.world.player_by_key(key) else {
                    // They left: the list they were on takes their window's place.
                    self.forget(viewer.key, Shown::Player(key));
                    return self.show(viewer, Some(Shown::Section(Section::Players)));
                };
                self.player_window(viewer, &player)
            }
        };
        self.actions.show(window, &[viewer.key])?;
        if let Some(shown) = shown {
            let mut held = self.shown.relock();
            let open = held.entry(viewer.key).or_default();
            open.retain(|had| had.id() != shown.id());
            open.push(shown);
        }
        Ok(())
    }

    /// Forgets the window `shown` that `viewer` held.
    fn forget(&self, viewer: PlayerKey, shown: Shown) {
        if let Some(open) = self.shown.relock().get_mut(&viewer) {
            open.retain(|had| *had != shown);
        }
    }

    /// Shows each window the caller holds again whose content their run of
    /// the action `id` changed.
    pub(crate) fn refresh(&self, viewer: &Player, id: &str) {
        let stale: Vec<Shown> = self
            .shown
            .relock()
            .get(&viewer.key)
            .map(|open| {
                open.iter()
                    .copied()
                    .filter(|shown| shown.stale_after(id))
                    .collect()
            })
            .unwrap_or_default();
        for shown in stale {
            if let Err(why) = self.show(viewer, Some(shown)) {
                enshwick::warn!("a window not shown again: {why}");
            }
        }
    }

    /// Forgets every window `player` held, as when they leave.
    pub(crate) fn forget_window(&self, player: PlayerKey) {
        self.shown.relock().remove(&player);
    }

    /// The panel: how many are online and a row for each section.
    fn panel(&self, viewer: &Player) -> Window {
        let count = self.world.players().len().to_string();
        let mut build = Build::new(self, viewer, PANEL, texts::WINDOW_PANEL)
            .text(texts::REPLY_ONLINE, &[(texts::value::COUNT, &count)]);
        for (at, (_, label, _)) in SECTIONS.iter().enumerate() {
            let open: &[Value] = &[Value::Choice(at)];
            build = build.row(label, &[], &[(texts::BUTTON_OPEN, ids::OPEN, open)], false);
        }
        build.window
    }

    /// The players online, each with the actions used most.
    fn players_window(&self, viewer: &Player) -> Window {
        let players = self.world.players();
        let count = players.len().to_string();
        let mut build = Build::new(self, viewer, PLAYERS, texts::WINDOW_PLAYERS)
            .text(texts::REPLY_ONLINE, &[(texts::value::COUNT, &count)]);
        let joined = self.joined.relock();
        for player in players.iter().take(MAX_ROWS) {
            let minutes = joined
                .get(&player.key)
                .map_or(0, |at| at.elapsed().as_secs() / 60)
                .to_string();
            let account = crate::account_id(player.key);
            let values = [
                (texts::value::NAME, player.name.as_str()),
                (texts::value::ACCOUNT, account.as_str()),
                (texts::value::MINUTES, minutes.as_str()),
            ];
            let them: &[Value] = &[value(player)];
            let buttons: &[(&str, &str, &[Value])] = if player.key == viewer.key {
                &[(texts::BUTTON_ACTIONS, ids::PLAYER, them)]
            } else {
                &[
                    (texts::BUTTON_ACTIONS, ids::PLAYER, them),
                    (texts::BUTTON_GOTO, ids::GOTO, them),
                    (texts::BUTTON_BRING, ids::BRING, them),
                    (texts::BUTTON_KICK, ids::KICK, them),
                ]
            };
            build = build.row(texts::REPLY_ONLINE_ROW, &values, buttons, true);
        }
        build.window
    }

    /// One player: moving them, moderating them and caring for them.
    fn player_window(&self, viewer: &Player, player: &Player) -> Window {
        let account = crate::account_id(player.key);
        let them: &[Value] = &[value(player)];
        let (on, off): (&[Value], &[Value]) = (
            &[value(player), Value::Toggle(true)],
            &[value(player), Value::Toggle(false)],
        );
        let mutes = [
            (texts::BUTTON_MUTE, ids::MUTED, on),
            (texts::BUTTON_UNMUTE, ids::MUTED, off),
        ];
        let removes = [
            (texts::BUTTON_KICK, ids::KICK, them),
            (texts::BUTTON_BAN, ids::BAN, them),
        ];
        let mark = (texts::BUTTON_MARK, ids::MARK, them);
        let meet = [
            (texts::BUTTON_GOTO, ids::GOTO, them),
            (texts::BUTTON_BRING, ids::BRING, them),
        ];
        // Their own window offers nothing that moves them to or removes them.
        let (moves, moderate): (Vec<Button<'_>>, Vec<Button<'_>>) = if player.key == viewer.key {
            (vec![mark], mutes.to_vec())
        } else {
            (
                meet.into_iter().chain([mark]).collect(),
                removes.into_iter().chain(mutes).collect(),
            )
        };
        Build::new(self, viewer, PLAYER, texts::WINDOW_PLAYER)
            .text(
                texts::WINDOW_PLAYER_WHO,
                &[
                    (texts::value::NAME, &player.name),
                    (texts::value::ACCOUNT, &account),
                ],
            )
            .row(texts::WINDOW_ROW_MOVE, &[], &moves, false)
            .row(
                texts::WINDOW_ROW_MOVE_TO,
                &[],
                &[
                    (texts::BUTTON_SEND, ids::SEND, them),
                    (texts::BUTTON_PLACE, ids::PLACE, them),
                ],
                false,
            )
            .row(texts::WINDOW_ROW_MODERATE, &[], &moderate, false)
            .row(
                texts::WINDOW_ROW_CARE,
                &[],
                &[
                    (texts::BUTTON_HEAL, ids::HEAL, them),
                    (texts::BUTTON_REVIVE, ids::REVIVE, them),
                    (texts::BUTTON_STATS, ids::STATS, them),
                ],
                false,
            )
            .row(
                texts::WINDOW_ROW_MAP,
                &[],
                &[(texts::BUTTON_UNCOVER, ids::UNCOVER, them)],
                false,
            )
            .row(
                texts::WINDOW_ROW_GOD,
                &[],
                &[
                    (texts::BUTTON_GOD_ON, ids::GOD, on),
                    (texts::BUTTON_GOD_OFF, ids::GOD, off),
                ],
                false,
            )
            .row(
                texts::WINDOW_ROW_ITEMS,
                &[],
                &[
                    (texts::BUTTON_GIVE, ids::GIVE, them),
                    (texts::BUTTON_CHEST, ids::CHEST, them),
                    (texts::BUTTON_LEARN, ids::LEARN, them),
                    (texts::BUTTON_CLEAR, ids::CLEAR, them),
                ],
                false,
            )
            .window
    }

    /// Banning, the allow-list and changes by account id, then the newest
    /// bans with a button to lift each: the platform cuts a window too
    /// big from its end, so the forms come first.
    fn accounts_window(&self, viewer: &Player) -> Window {
        let build = Build::new(self, viewer, ACCOUNTS, texts::WINDOW_ACCOUNTS);
        let Some(bans) = self.accounts.with(|accounts| accounts.bans.clone()) else {
            // Lists that do not read are no empty lists.
            let why = self.accounts.shut_reason().unwrap_or_default();
            return build
                .text(texts::REPLY_STORE_SHUT, &[(texts::value::WHY, &why)])
                .window;
        };
        let count = bans.len().to_string();
        let allowlist = if self.settings.load().allowlist {
            texts::WINDOW_ALLOWLIST_ON
        } else {
            texts::WINDOW_ALLOWLIST_OFF
        };
        let mut build = build
            .action(ids::BAN)
            .text(allowlist, &[])
            .action(ids::ALLOWED)
            .action(ids::ACCOUNT)
            .text(texts::REPLY_BANS, &[(texts::value::COUNT, &count)]);
        for ban in bans.iter().rev().take(MAX_ROWS) {
            let lift: &[Value] = &[
                Value::Player {
                    key: ban.key,
                    name: ban.name.clone(),
                },
                Value::Toggle(false),
            ];
            build = build.row(
                texts::REPLY_BANS_ROW,
                &borrowed(&ban_values(ban)),
                &[(texts::BUTTON_UNBAN, ids::BANNED, lift)],
                true,
            );
        }
        build.window
    }

    /// The message of the day, announcements, the restart warning, the
    /// time of day, the weather and saves.
    fn server_window(&self, viewer: &Player) -> Window {
        let motd = self.motd();
        let build = Build::new(self, viewer, SERVER, texts::WINDOW_SERVER);
        let build = if motd.is_empty() {
            build.text(texts::REPLY_MOTD_NONE, &[])
        } else {
            build.text(texts::WINDOW_MOTD, &[(texts::value::TEXT, &motd)])
        };
        let times = TIMES.map(|time| (time, ids::TIME, [Value::Text(time.to_owned())]));
        let times = times
            .each_ref()
            .map(|(label, id, args)| (*label, *id, &args[..]));
        let kinds: Vec<[Value; 1]> = (0..FORECASTS.len()).map(|at| [Value::Choice(at)]).collect();
        let weathers: Vec<(&str, &str, &[Value])> = FORECASTS
            .iter()
            .zip(&kinds)
            .map(|((_, label, _), args)| (*label, ids::WEATHER, &args[..]))
            .collect();
        let stop: &[Value] = &[Value::Number(0.0)];
        let build = build
            .action(ids::SET_MOTD)
            .action(ids::SAY)
            .action(ids::RESTART);
        let build = if self.ticker.relock().restarting() {
            build.row(
                texts::WINDOW_ROW_RESTART,
                &[],
                &[(texts::BUTTON_STOP, ids::RESTART, stop)],
                false,
            )
        } else {
            build
        };
        build
            .row(texts::ACTION_TIME, &[], &times, false)
            .action(ids::TIME)
            .row(texts::ACTION_WEATHER, &[], &weathers, false)
            .row(
                texts::WINDOW_ROW_WORLD,
                &[],
                &[
                    (texts::BUTTON_SAVE, ids::SAVE, &[]),
                    (texts::BUTTON_BACKUP, ids::BACKUP, &[]),
                ],
                false,
            )
            .window
    }

    /// Whether the build log is on, and the lookup around the viewer.
    fn builds_window(&self, viewer: &Player) -> Window {
        let state = if self.settings.load().build_log {
            texts::WINDOW_BUILDS_ON
        } else {
            texts::REPLY_BUILDS_OFF
        };
        Build::new(self, viewer, BUILDS, texts::WINDOW_BUILDS)
            .text(state, &[])
            .action(ids::WHO)
            .window
    }

    /// Shows the caller what a lookup of the build log found, each change
    /// with a button that takes them there.
    pub(crate) fn show_changes(&self, request: &Request<'_>, found: &[Change]) -> Result<(), Off> {
        let viewer = request.player;
        let mut build = Build::new(self, viewer, CHANGES, texts::WINDOW_CHANGES);
        for change in found {
            let kind = request.say(change.kind_text(), &[]);
            let [x, y, z] = change.at.map(Value::Number);
            let there: &[Value] = &[value(viewer), x, y, z];
            build = build.row(
                texts::REPLY_WHO_ROW,
                &borrowed(&crate::builds::row(change, kind)),
                &[(texts::BUTTON_GO_THERE, ids::PLACE, there)],
                true,
            );
        }
        self.actions.show(build.window, &[viewer.key])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_is_shown_again_only_after_a_change_to_what_it_lists() {
        let accounts = Shown::Section(Section::Accounts);
        assert!(accounts.stale_after(ids::BANNED));
        assert!(!accounts.stale_after(ids::KICK));
        let players = Shown::Section(Section::Players);
        assert!(players.stale_after(ids::KICK));
        assert!(!players.stale_after(ids::MUTED), "the list shows no mutes");
        assert!(Shown::Section(Section::Server).stale_after(ids::RESTART));
        assert!(Shown::Player(PlayerKey::new(1)).stale_after(ids::BAN));
        assert!(!Shown::Player(PlayerKey::new(1)).stale_after(ids::GOD));
        assert!(!Shown::Section(Section::Builds).stale_after(ids::BAN));
    }

    #[test]
    fn each_section_s_window_id_is_its_word() {
        for (word, _, section) in SECTIONS {
            assert_eq!(Shown::Section(section).id(), word);
        }
    }

    #[test]
    fn every_section_has_a_window_id_the_platform_takes() {
        for id in [PANEL, PLAYER, CHANGES]
            .into_iter()
            .chain(SECTIONS.map(|(word, ..)| word))
        {
            assert!(enshwick::actions::is_valid_id(id), "{id}");
        }
    }
}
