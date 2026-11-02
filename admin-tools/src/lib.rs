//! Admin Tools: actions for a server's administrators in the game's
//! interface. "Muted in chat" lists the players online with a switch each;
//! a muted player's chat lines reach nobody.

use enshwick::actions::{Action, Outcome, Request, Value};
use enshwick::world::World;
use enshwick::{Load, Mod, Off, WorldError};

enshwick::texts!();

/// The action that mutes and unmutes players in chat.
const MUTED_ACTION: &str = "muted";

/// The mod's state.
#[derive(Debug)]
pub struct AdminTools;

impl Mod for AdminTools {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = load.texts()?;
        let world = load.world()?;
        let actions = load.actions(texts)?;
        actions.add(
            Action::per_player(MUTED_ACTION, texts::ACTION_MUTED, move |_ask, _subject| {
                muted_online(world)
            })
            .help(texts::ACTION_MUTED_HELP)
            .admins(),
            move |request| mute(world, request),
        )?;
        Ok(Self)
    }
}

/// The players online who are muted now, switched on in the list.
fn muted_online(world: World) -> Vec<(enshwick::PlayerKey, String)> {
    world
        .players()
        .into_iter()
        .filter(|player| world.muted(player.key).unwrap_or(false))
        .map(|player| (player.key, player.name))
        .collect()
}

/// Mutes the player a switch names, or lifts the mute; the reply names them.
fn mute(world: World, request: &Request<'_>) -> Outcome {
    let [Value::Player { key, name }, Value::Toggle(on)] = request.args else {
        return Outcome::refused(request.say(texts::REPLY_NO_PLAYER, &[]));
    };
    let values = [(texts::value::NAME, name.as_str())];
    match world.set_muted(*key, *on) {
        Ok(()) if *on => Outcome::done(request.say(texts::REPLY_MUTED, &values)),
        Ok(()) => Outcome::done(request.say(texts::REPLY_HEARD, &values)),
        Err(WorldError::Refused) if *on => {
            Outcome::refused(request.say(texts::REPLY_PROTECTED, &values))
        }
        Err(why) => {
            let why = why.to_string();
            Outcome::refused(request.say(
                texts::REPLY_FAILED,
                &[
                    (texts::value::NAME, name.as_str()),
                    (texts::value::WHY, &why),
                ],
            ))
        }
    }
}

enshwick::declare_mod!(AdminTools);
