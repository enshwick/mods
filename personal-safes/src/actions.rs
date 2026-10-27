//! Who may open a Safe, as the owner manages it: a list of the server's
//! players with a switch each, for the Safe at hand, on the Mods screen and
//! in a window a HUD hint opens beside the Safe, and a list action that
//! names them.
//!
//! The Safe at hand is the one within reach the player may manage: their
//! own nearest one first, else the nearest other they may manage, as an
//! administrator while `admins_can_open` is on, or one another mod lets
//! them manage.

use std::sync::atomic::{AtomicU32, Ordering};

use enshwick::actions::{Action, Ask, Outcome, Request, Subject, Value};
use enshwick::i18n::{Language, Texts};
use enshwick::world::World;
use enshwick::{Load, Off};

use crate::access::{AllowOutcome, Cell, Safe, Seen, centre, nearest_own, snapshots};
use crate::keys;

/// How far from the player their Safe may be, in metres; `texts.toml` says
/// "4 m" in its replies.
pub const REACH: f64 = 4.0;

/// The action that switches players on and off at the Safe at hand.
const ACCESS_ACTION: &str = "access";
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
fn whose(say: &Say<'_>, safe: &Safe, key: u64) -> String {
    if safe.owner == key {
        say(keys::YOURS, &[])
    } else if safe.owner_name.is_empty() {
        say(keys::UNKNOWN, &[])
    } else {
        say(keys::THEIRS, &[(keys::value::OWNER, &safe.owner_name)])
    }
}

/// The Safe at hand for the player who asks: within reach, one they may
/// manage, their own first; or why there is none.
fn at_hand(world: World, ask: &Ask<'_>) -> Result<Subject, String> {
    let me = ask.player;
    let position = world
        .position(me.entity)
        .ok_or_else(|| ask.say(keys::NO_SAFE, &[]))?;
    let seen = Seen {
        key: me.key,
        admin: me.is_admin(),
    };
    if let Some((at, safe)) = snapshots().nearest_managed(seen, me.entity, position, REACH) {
        let say = |key: &str, values: &[(&str, &str)]| ask.say(key, values);
        let label = ask.say(
            keys::SUBJECT,
            &[
                (keys::value::SAFE, &whose(&say, &safe, me.key)),
                (keys::value::AT, &place(at)),
            ],
        );
        return Ok(Subject::new(pack(at), label));
    }
    // An own Safe in reach that another mod's link keeps them from.
    Err(
        match nearest_own(&snapshots().owners.load(), me.key, position, REACH) {
            Some(own) => ask.say(keys::REFUSED, &[(keys::value::AT, &place(own))]),
            None => ask.say(keys::NO_SAFE, &[]),
        },
    )
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
        return Outcome::refused(request.say(keys::NO_SAFE, &[]));
    };
    let say = |key: &str, values: &[(&str, &str)]| request.say(key, values);
    let whose = whose(&say, &record, request.player.key);
    let here = place(at);
    let values = [
        (keys::value::NAME, name.as_str()),
        (keys::value::SAFE, whose.as_str()),
        (keys::value::AT, here.as_str()),
    ];
    if !on {
        return match snapshots().change(at, |safe| safe.deny_key(*key)).flatten() {
            Some(gone) => {
                save();
                Outcome::done(request.say(
                    keys::DENIED,
                    &[
                        (keys::value::NAME, &gone.name),
                        (keys::value::SAFE, &whose),
                        (keys::value::AT, &here),
                    ],
                ))
            }
            None => Outcome::refused(request.say(keys::NOT_ALLOWED, &values)),
        };
    }
    if *key == request.player.key {
        return Outcome::refused(request.say(keys::SELF, &[]));
    }
    match snapshots().change(at, |safe| safe.allow(*key, name)) {
        Some(AllowOutcome::Allowed) => {
            save();
            Outcome::done(request.say(keys::ALLOWED, &values))
        }
        Some(AllowOutcome::Already) => Outcome::refused(request.say(keys::ALREADY, &values)),
        Some(AllowOutcome::Owner) => Outcome::refused(request.say(keys::OWNER, &values)),
        None => Outcome::refused(request.say(keys::NO_SAFE, &[])),
    }
}

/// Names who may use the Safe at hand.
fn list(request: &Request<'_>) -> Outcome {
    let Some((at, safe)) = safe_of(request) else {
        return Outcome::refused(request.say(keys::NO_SAFE, &[]));
    };
    let say = |key: &str, values: &[(&str, &str)]| request.say(key, values);
    let whose = whose(&say, &safe, request.player.key);
    let here = place(at);
    let names: Vec<&str> = safe.allowed.iter().map(|a| a.name.as_str()).collect();
    Outcome::done(if names.is_empty() {
        request.say(
            keys::NAMES_EMPTY,
            &[(keys::value::SAFE, &whose), (keys::value::AT, &here)],
        )
    } else {
        request.say(
            keys::NAMES,
            &[
                (keys::value::SAFE, &whose),
                (keys::value::AT, &here),
                (keys::value::NAMES, &names.join(", ")),
            ],
        )
    })
}

/// Declares who may open a Safe: the list with its hint, and the list as
/// text; saves through `save` when it changed who may.
///
/// # Errors
///
/// [`Off`] when the loader refuses the actions.
pub fn declare(
    load: &mut Load<'_>,
    world: World,
    texts: Texts,
    save: impl Fn() + Send + Sync + 'static,
) -> Result<(), Off> {
    let actions = load.actions(texts)?;
    actions.add(
        Action::per_player(ACCESS_ACTION, keys::ACCESS, |_ask, subject| {
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
        .help(keys::ACCESS_HELP)
        .at(move |ask| at_hand(world, ask))
        .hint(),
        move |request| access(request, &save),
    )?;
    actions.add(
        Action::new(LIST_ACTION, keys::LIST).at(move |ask| at_hand(world, ask)),
        list,
    )
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
    fn every_text_the_mod_says_exists_in_all_languages() -> Result<(), Off> {
        let texts = Texts::parse(crate::content::TEXTS)?;
        assert_eq!(texts.untranslated(), []);
        for &key in keys::ALL {
            assert!(texts.get(key, Language::FALLBACK).is_some(), "{key}");
        }
        Ok(())
    }

    #[test]
    fn every_language_of_the_file_holds_the_keys_the_mod_says_and_no_others() {
        let tables: toml::Table = toml::from_str(crate::content::TEXTS).unwrap();
        let mut said: Vec<&str> = keys::ALL.to_vec();
        said.sort_unstable();
        for (language, table) in &tables {
            let mut held: Vec<&str> = table
                .as_table()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect();
            held.sort_unstable();
            assert_eq!(held, said, "{language}");
        }
    }
}
