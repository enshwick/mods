//! Who may open a Safe, as the owner manages it: a list of the server's
//! players with a switch each, for the Safe at hand, in the panel beside
//! the Safe's chest screen, and a list action on Shift+L that names them.
//!
//! The Safe at hand is the one the panel names, the open Safe, when the
//! player may manage it. Without one, from the list key, it is the one
//! within reach the player may manage: the one they opened last
//! first, then their own nearest one, else the nearest other they may
//! manage, as an administrator while `admins_can_open` is on, or one
//! another mod lets them manage.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::actions::{Action, Actions, Ask, FindSubjectError, Outcome, Request, Subject, Value};
use enshwick::controls::Key;
use enshwick::i18n::{Language, Texts};
use enshwick::world::World;
use enshwick::{Entity, Load, Off, PlayerKey};

use crate::access::{
    AllowOutcome, Cell, Safe, Seen, cell, centre, nearest_own, owner_near, snapshots,
};
use crate::texts;

/// How far from the player the Safe at hand may be, in metres, when no Safe
/// is named; `texts.toml` says "4 m" in its replies.
pub const REACH: f64 = 4.0;

/// The action that switches players on and off at the Safe at hand.
pub(crate) const ACCESS_ACTION: &str = "access";
/// The action that names who may open the Safe at hand.
const LIST_ACTION: &str = "list";

/// The mod's texts in the server's language, for the owner's name on a
/// Safe.
#[derive(Debug)]
pub struct Phrases {
    /// Every text the mod has.
    pub texts: Texts,
    /// The language picked in the settings in force.
    language: AtomicU32,
}

impl Phrases {
    /// `texts` in `language`.
    #[must_use]
    pub const fn new(texts: Texts, language: Language) -> Self {
        Self {
            texts,
            language: AtomicU32::new(language.0),
        }
    }

    /// Speaks `language` from now on.
    pub fn set_language(&self, language: Language) {
        self.language.store(language.0, Ordering::Relaxed);
    }

    /// The text `key` with each `{name}` replaced by its value; the key
    /// itself when there is no text.
    #[must_use]
    pub fn say(&self, key: &str, values: &[(&str, &str)]) -> String {
        let language = Language(self.language.load(Ordering::Relaxed));
        self.texts.fill(key, language, values)
    }
}

/// A cell as the player reads it: the Safe's position in whole metres.
fn place(at: Cell) -> String {
    let [x, y, z] = centre(at);
    format!("({x:.0}, {y:.0}, {z:.0})")
}

/// Packs a cell into a subject's id; 21 bits per axis, a range of ±500 km.
#[expect(clippy::cast_sign_loss, reason = "each axis is masked to its 21 bits")]
fn pack(at: Cell) -> u64 {
    let axis = |v: i64| (v as u64) & 0x1f_ffff;
    axis(at.0) << 42 | axis(at.1) << 21 | axis(at.2)
}

/// The cell a subject's id packs.
#[expect(
    clippy::cast_possible_wrap,
    reason = "each axis is sign-extended from its 21 bits"
)]
fn unpack(id: u64) -> Cell {
    let axis = |shift: u32| (((id >> shift) & 0x1f_ffff) << 43) as i64 >> 43;
    (axis(42), axis(21), axis(0))
}

/// A mod text filled in the asking player's language.
type Say<'a> = dyn Fn(&str, &[(&str, &str)]) -> String + 'a;

/// Whose Safe `safe` is, as `key` reads it in `say`'s language.
fn whose(say: &Say<'_>, safe: &Safe, key: PlayerKey) -> String {
    if safe.owner == key {
        say(texts::COMMAND_YOURS, &[])
    } else if safe.owner_name.is_empty() {
        say(texts::COMMAND_UNKNOWN, &[])
    } else {
        say(
            texts::COMMAND_THEIRS,
            &[(texts::value::OWNER, &safe.owner_name)],
        )
    }
}

/// The Safe at hand for the player who asks: the one their client named, or
/// else within reach, one they may manage, their own first; or why there is
/// none.
fn at_hand(world: World, ask: &Ask<'_>) -> Result<Subject, FindSubjectError> {
    let me = ask.player;
    let seen = Seen {
        key: me.key,
        admin: me.is_admin(),
    };
    // The server keeps a target within its reach of the player already.
    if let Some(position) = ask.target.and_then(|target| target.position) {
        let owners = snapshots().owners.load();
        let target = cell(position);
        let (at, safe) =
            owner_near(&owners, target).map_or((target, None), |(at, safe)| (at, Some(safe)));
        let place = place(at);
        let here = [(texts::value::AT, place.as_str())];
        return match safe {
            Some(safe) if snapshots().may_manage(at, seen, me.entity) => Ok(subject(ask, at, safe)),
            // An own Safe that another mod's link keeps them from.
            Some(safe) if safe.owner == me.key => {
                Err(ask.say(texts::COMMAND_REFUSED, &here).into())
            }
            _ => Err(ask.say(texts::COMMAND_NOT_MANAGED, &here).into()),
        };
    }
    let position = world
        .position(me.entity)
        .ok_or_else(|| FindSubjectError::from(ask.say(texts::COMMAND_NO_SAFE, &[])))?;
    if let Some((at, safe)) = snapshots().nearest_managed(seen, me.entity, position, REACH) {
        return Ok(subject(ask, at, &safe));
    }
    // An own Safe in reach that another mod's link keeps them from.
    Err(
        match nearest_own(&snapshots().owners.load(), me.key, position, REACH) {
            Some(own) => ask.say(texts::COMMAND_REFUSED, &[(texts::value::AT, &place(own))]),
            None => ask.say(texts::COMMAND_NO_SAFE, &[]),
        }
        .into(),
    )
}

/// The Safe `safe` at `at` as the subject the asking player reads.
fn subject(ask: &Ask<'_>, at: Cell, safe: &Safe) -> Subject {
    let say = |key: &str, values: &[(&str, &str)]| ask.say(key, values);
    let label = ask.say(
        texts::ACTION_SUBJECT,
        &[
            (texts::value::SAFE, &whose(&say, safe, ask.player.key)),
            (texts::value::AT, &place(at)),
        ],
    );
    Subject::new(pack(at), label)
}

/// The Safe a request acts on, its cell and record as they stand now.
fn safe_of(request: &Request<'_>) -> Option<(Cell, Safe)> {
    let at = unpack(request.subject?.id);
    Some((at, snapshots().owners.load().get(&at)?.clone()))
}

/// Switches a player on or off at the Safe at hand.
fn access(request: &Request<'_>, save: &impl Fn()) -> Outcome {
    let (Some((at, record)), [Value::Player { key, name }, Value::Toggle(on)]) =
        (safe_of(request), request.args)
    else {
        return Outcome::refused(request.say(texts::COMMAND_NO_SAFE, &[]));
    };
    let say = |key: &str, values: &[(&str, &str)]| request.say(key, values);
    let whose = whose(&say, &record, request.player.key);
    let here = place(at);
    let values = [
        (texts::value::NAME, name.as_str()),
        (texts::value::SAFE, whose.as_str()),
        (texts::value::AT, here.as_str()),
    ];
    if !on {
        return match snapshots().change(at, |safe| safe.deny_key(*key)).flatten() {
            Some(gone) => {
                save();
                Outcome::done(request.say(
                    texts::COMMAND_DENIED,
                    &[
                        (texts::value::NAME, &gone.name),
                        (texts::value::SAFE, &whose),
                        (texts::value::AT, &here),
                    ],
                ))
            }
            None => Outcome::refused(request.say(texts::COMMAND_NOT_ALLOWED, &values)),
        };
    }
    if *key == request.player.key {
        return Outcome::refused(request.say(texts::COMMAND_SELF, &[]));
    }
    match snapshots().change(at, |safe| safe.allow(*key, name)) {
        Some(AllowOutcome::Allowed) => {
            save();
            Outcome::done(request.say(texts::COMMAND_ALLOWED, &values))
        }
        Some(AllowOutcome::Already) => {
            Outcome::refused(request.say(texts::COMMAND_ALREADY, &values))
        }
        Some(AllowOutcome::Owner) => Outcome::refused(request.say(texts::COMMAND_OWNER, &values)),
        None => Outcome::refused(request.say(texts::COMMAND_NO_SAFE, &[])),
    }
}

/// Names who may use the Safe at hand.
fn list(request: &Request<'_>) -> Outcome {
    let Some((at, safe)) = safe_of(request) else {
        return Outcome::refused(request.say(texts::COMMAND_NO_SAFE, &[]));
    };
    let say = |key: &str, values: &[(&str, &str)]| request.say(key, values);
    let whose = whose(&say, &safe, request.player.key);
    let here = place(at);
    let names: Vec<&str> = safe.allowed.iter().map(|a| a.name.as_str()).collect();
    Outcome::done(if names.is_empty() {
        request.say(
            texts::COMMAND_LIST_EMPTY,
            &[(texts::value::SAFE, &whose), (texts::value::AT, &here)],
        )
    } else {
        request.say(
            texts::COMMAND_LIST,
            &[
                (texts::value::SAFE, &whose),
                (texts::value::AT, &here),
                (texts::value::NAMES, &names.join(", ")),
            ],
        )
    })
}

/// Declares who may open a Safe and the list of them; saves through `save`
/// when it changed who may, and calls `answered` with the player and their
/// character after every switch, changed or refused.
///
/// # Errors
///
/// [`Off`] when the loader refuses the actions.
pub fn declare(
    load: &mut Load<'_>,
    world: World,
    texts: Texts,
    save: impl Fn() + Send + Sync + 'static,
    answered: impl Fn(Seen, Entity) + Send + Sync + 'static,
) -> Result<Actions, Off> {
    let actions = load.actions(texts)?;
    actions.add(
        Action::per_player(ACCESS_ACTION, texts::ACTION_ACCESS, |_ask, subject| {
            let Some(at) = subject.map(|s| unpack(s.id)) else {
                return Vec::new();
            };
            snapshots()
                .owners
                .load()
                .get(&at)
                .map_or_else(Vec::new, |safe| {
                    safe.allowed
                        .iter()
                        .map(|a| (a.key, a.name.clone()))
                        .collect()
                })
        })
        .help(texts::ACTION_ACCESS_HELP)
        .at(move |ask| at_hand(world, ask)),
        move |request| {
            let outcome = access(request, &save);
            let me = request.player;
            answered(
                Seen {
                    key: me.key,
                    admin: me.is_admin(),
                },
                me.entity,
            );
            outcome
        },
    )?;
    actions.add(
        Action::new(LIST_ACTION, texts::ACTION_LIST).at(move |ask| at_hand(world, ask)),
        list,
    )?;
    Ok(actions)
}

/// The controls' name as a feature of the mod, on the Mods screen and in the
/// log.
pub const CONTROLS_FEATURE: &str = "controls";

/// The key that names who may open the Safe at hand, as a notice.
pub const LIST_KEY: &str = "Shift+L";

/// Puts the list action on [`LIST_KEY`] on the client, so a player asks
/// without the Mods screen.
///
/// # Errors
///
/// [`Off`] as [`Actions::control`] gives it, such as when the loader refuses
/// the control.
pub fn list_control(actions: &Actions) -> Result<(), Off> {
    actions.control(LIST_ACTION, Key::parse(LIST_KEY)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_survive_a_subject_id() {
        for at in [(0, 0, 0), (2, -4, 20), (-1_000_000, 1_000_000, -1)] {
            assert_eq!(unpack(pack(at)), at);
        }
        assert_eq!(place((4, -4, 20)), "(2, -2, 10)");
    }

    #[test]
    fn every_language_keeps_the_placeholders_of_each_text() {
        let tables: toml::Table = toml::from_str(texts::TOML).unwrap();
        let placeholders = |text: &str| {
            let mut names: Vec<String> = text
                .split('{')
                .skip(1)
                .filter_map(|rest| rest.split_once('}').map(|(name, _)| name.to_owned()))
                .collect();
            names.sort_unstable();
            names
        };
        let fallback = tables["en"].as_table().unwrap();
        for (language, table) in &tables {
            for (key, text) in table.as_table().unwrap() {
                let english = fallback[key].as_str().unwrap();
                assert_eq!(
                    placeholders(text.as_str().unwrap()),
                    placeholders(english),
                    "{language} {key}"
                );
            }
        }
    }

    #[test]
    fn every_text_the_mod_says_exists_in_all_languages() -> Result<(), Off> {
        let texts = Texts::parse(texts::TOML)?;
        assert_eq!(texts.untranslated(), []);
        for &key in texts::ALL {
            assert!(texts.get(key, Language::FALLBACK).is_some(), "{key}");
        }
        Ok(())
    }

    #[test]
    fn the_mod_and_every_setting_have_their_texts() {
        use enshwick::config::Config as _;
        use enshwick::i18n::{MOD_DESCRIPTION, MOD_NAME};

        let schema = crate::Settings::schema();
        let settings = std::iter::once(schema.text).chain(
            schema
                .fields
                .into_iter()
                .flat_map(|field| [format!("{}.doc", field.text), field.text]),
        );
        for key in settings.chain([MOD_NAME, MOD_DESCRIPTION].map(str::to_owned)) {
            assert!(texts::ALL.contains(&key.as_str()), "{key}");
        }
    }
}
