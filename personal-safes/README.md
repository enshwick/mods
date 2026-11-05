# Personal Safes

Personal Safes adds the Safe, a chest that only its owner and the players they
allow can open. Only the owner picks it up.

Built on [Enshwick](https://github.com/enshwick/enshwick).

- You craft the Safe at the same workshop as the Wooden Chest. By default it
  costs twice the ingredients of the Wooden Chest.
- Only the player who placed a Safe, and the players they let in, can open or
  empty it. Only the owner picks it up or dismantles it.
- Crafting and building do not take items from another player's Safe.
- The server's administrators can open every Safe, unless the settings turn this
  off.

## How does a Safe show its owner?

A Safe gets its owner's name when the owner places it, such as "Safe · Ann". The
name is in the server's language and shows on hover. A Safe without a name
gets one a few seconds after its area loads. The server must know the
owner's name first, from its records or because the owner is online.

Only the owner can rename a Safe. Administrators can rename it too when the
settings allow.

## Letting other players in

Open one of your Safes. A panel beside the chest's screen lists the players
online and the players you already let in, each with a switch. The panel shows
only at a Safe you may manage, never at another chest.

The same list is on the Mods screen: stand within 4 m of the Safe, open the
pause menu, then Mods, then Actions under Personal Safes. The screen names the
Safe, such as "your Safe at (x, y, z)".

A player you switch on can open the Safe, take, put and sort items, and craft or
build from it, as you do. That player never picks it up, dismantles it, renames
it or changes who may use it. Switch the player off to take this back, also
while the player is away. The game confirms each change with a notice.

The screen shows one action at a time: the list, and "Name who may open it",
which answers with a notice.

The Safe at hand is the one you opened last, while it is within 4 m, else the
nearest one within 4 m that you may manage, your own first. You may manage your own Safes and the Safes another mod lets you manage
(see For mod authors). Administrators may manage every Safe while
`admins_can_open` is on. Nobody else removes a Safe, except administrators while
`admins_can_open` is on.

All of this works on a dedicated server, in a game a player hosts and in single
player. A player who has just joined cannot use any Safe, their own included,
until the server registers them. This takes a moment after they arrive.

## What happens without the mod?

A Safe is the vanilla Wooden Chest underneath. Without the mod it is an ordinary
chest, and every item stays in it. You cannot upgrade a Safe, because an upgrade
would replace it with an ordinary chest.

## Settings

Each world keeps its settings in
`<save>-mod-settings/enshwick.personal-safes/settings.toml` beside its save.
The loader writes the file as the world first loads (see
[Settings](https://github.com/enshwick/enshwick#settings)):

```toml
cost_multiplier = 2
admins_can_open = true
label_language = "en"
```

| Key               | Default | Range                                       | Meaning                                                       |
| ----------------- | ------- | ------------------------------------------- | ------------------------------------------------------------- |
| `cost_multiplier` | 2       | 1 to 100                                    | the Safe costs the ingredients of the Wooden Chest times this |
| `admins_can_open` | true    |                                             | administrators can open, remove and rename every Safe         |
| `label_language`  | `en`    | a language code, such as `en`, `uk` or `de` | the language of the owner's name on a Safe                    |

`cost_multiplier` is the server's value. A joined player's crafting menu shows
the server's cost. A change applies at once, without a restart of the game. This
is also true for a world with another value.

`admins_can_open` applies while the game runs.

`label_language` refuses any value that is not a language code. The Mods screen
then shows the reason in the player's language. A change applies while the game
runs, to Safes named after the change. The actions on the Mods screen use each
player's own language.

The texts come in every language of [texts.toml](src/texts.toml).

The mod keeps owners and allowed players per world, beside the world's save, in
`<save>-mod-data/enshwick.personal-safes/owners.toml`. Each world a player
hosts has its own file, and copying the save folder takes it along.

If this file cannot be read, the mod leaves it as it is and keeps every Safe
without a known owner locked for everyone. The log names the file. Fix or
restore it, then restart the server.

## Install

Download
`enshwick-personal-safes.zip` from its [newest release](https://github.com/enshwick/mods/releases?q=enshwick-personal-safes&expanded=true).
Extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. The mod needs the Enshwick loader. Get
`enshwick.zip` from the [Enshwick release](https://github.com/enshwick/enshwick/releases)
that the mod's release notes name, and extract it into the same folder. Until 1.0 both are alphas, and anything may change from one alpha
to the next. Back up your saves before you update.

## Who needs it

The server and every player need it. The item and its recipe come from the mod
on each side, and the mod counts in the join check.

## For mod authors

Other mods can change who may open, use, remove or rename a Safe, and who may
manage the list of players at it. They can also refuse an action. They do this
at the extension point `enshwick.personal-safes:access`. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](LICENSE).
