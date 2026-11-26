# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry says what players notice; a release's notes show its section.

## [Unreleased]

### Added

- Protected zones also stop digging, bombs, breaking placed pieces and
  changing Flame Altars for everyone but zone builders while they stand in
  a zone or within its margin, 10 metres by default, and can stop opening
  chests in bases there too. The game itself refuses the edits, so players
  without Enshwick are refused too, and a player reads a notice on coming
  near. `/adm zonerules` and the Protected zones window set what each zone
  stops, and the zone list shows it; **Zone margin** sets the margin. Zones
  made before stop digging and breaking as well.

### Changed

- A chest or prop that an action places is filled or kept once it stands
  in the world, right after the step the game places it in; one that does
  not stand within 10 seconds is taken back and reported, as before.
- Typed commands take the game's template and item debug names in any
  case, an exact match first. A name that several templates or items share
  lists up to eight of their GUIDs to pick from.
- Admin Tools needs an Enshwick with the stable ABI 1.0;
  Enshwick 1.0.0-alpha.17 and older do not load it.

### Fixed

- A chest of items that takes none of its items is given them once more a
  moment later. If it still takes none, it is taken back and the admin
  reads that no chest went up, or that it stands empty when the game cannot
  take it back, instead of an empty chest staying unnoticed.
- God mode stays on after the player respawns, until they leave. Before,
  it ended at the respawn.
- A zone builder who respawns on a server builds in a zone from the next
  server step, instead of being refused for up to a second.
- A zone builder who hosts the game sees the build cursor allowed in a zone,
  also right after a respawn.
- A new remote console password takes effect on the open port, so a tool
  logging in meanwhile gets an answer instead of a dropped connection. Tools
  logged in with the old password are closed at once, and failed logins are
  forgotten, as when the console opened again.

## [0.2.0] - 2026-11-15

### Changed

- In the window, a spawn's creature comes from a list of the game's
  templates, searched as the item list is, instead of a typed name. Commands
  still take a template name or GUID, and now a mod's template id; a name
  must match the game's case.

## [0.1.1] - 2026-11-15

### Added

- Protected zones: an administrator marks a circle or a box around them
  where only zone builders, admins by default, may build. The server refuses
  every other placement there, also from players without Enshwick, and the
  builder reads why. Zones are listed, switched on and off and removed in
  the window and by command, and kept with the world. While the zones file
  does not read, only zone builders build anywhere.
- Administrators spawn creatures by template name or GUID around a player
  or at a position, with a count, a level and a lifetime. Spawned creatures
  leave with the next world load unless kept.
- Administrators place a prop, such as a workbench, beside a player or at a
  position, and take back their last one.

### Fixed

- The bans window lists the bans only to players who may read the ban list.
- A window whose list is longer than it shows says how many rows it left out.

## [0.1.0] - 2026-11-14

### Added

- First alpha. A server's administrators mute players in chat from the
  dedicated server's console.
- Every action is a command in chat and the console: `/adm` or
  `/admin-tools`, with `/adm help` listing what the caller may use.
- Administrators list the players online, kick them, and heal them.
- A ban holds when the account comes back, also after a restart, and lifts
  while the player is away.
- An allow-list, on in the world's settings, keeps out accounts not on it.
- Players read a message of the day when they join, and announcements repeat
  on a schedule.
- A restart warning counts down for every player and can be called off.
- Administrators set the time of day and the weather.
- Administrators take themselves to a player, bring a player to them, send
  one player to another, or move a player to a position.
- Each world decides who may use each action, by the permissions of the
  game's user groups.
- An audit log beside the world's save keeps every change an administrator
  made.
- The players list shows each player's account id, and `/adm status` shows
  the uptime, the world and the Enshwick and mod versions.
- RCON tools such as BattleMetrics or rcon-cli run every command on a
  dedicated server, on port 25575 of the server's own computer with a
  password Admin Tools writes into the world's settings at the first start,
  where it is never shown in the game or logged; a world setting opens it to other
  addresses, and a game a player hosts has it off unless the world turns it
  on.
- A chat log beside the world's save keeps every chat line with the
  player's name and account id; it is on for a dedicated server.
- `/adm backup` copies the world's save, mod settings and mod data while the
  server runs, and keeps the newest 10 backups.
- `/adm save` saves the world now, and `/adm world` shows its last save and
  its game settings.
- Shift+F9 opens the Admin Tools window: the players online, each with a
  window of every action on them, the bans and the allow-list, the server,
  and the build log. Each administrator sees only the actions they may use.
- A ban keeps its reason, `/adm bans` lists the newest 50 bans, and players who are
  away can be banned, unbanned and allowed by the name they last joined
  with, or any account by its id.
- `/adm say` shows every player a line now, and `/adm setmotd` sets the
  message of the day in game.
- `/adm time` takes a time such as `18:30`.
- `/adm mark` moves a player to the waypoint you set on your map.
- A build log, off by default, keeps who placed or removed a building or
  building blocks or changed terrain, and `/adm who` lists the changes near
  you.
- A moderator who is not an administrator cannot kick or ban themselves or
  take themselves off the allow-list.
- A restart warning stops the dedicated server when its time is up, as
  Ctrl+C in its console does: the game saves the world and the server exits
  for its host to start it again. It waits for a running backup, and saves
  the world instead where the game cannot be stopped. A game a player hosts
  offers no restart warning.
- Administrators give a player items in as many stacks as needed, and read
  how many did not fit and were left out. An unknown name answers with the
  names near it.
- Administrators place a wooden chest of items beside a player or at a
  position.
- Administrators clear a player's inventory or copy one player's inventory
  to another. A copy gives what fits, says how many items stayed out, and
  leaves the look and currency slots out.
- A chest the game does not place is taken back, so no empty chest follows.
- A count or a value with a fraction is refused, and `learn` refuses
  knowledge that belongs to the world.
- Administrators set a player's health, stamina or mana, and teach a player
  a recipe or any knowledge by its number.
- God mode keeps all damage off a player until they leave or the world
  stops.
- `/adm online` lists the players in chat, one player a line.
- The audit log also keeps the runs Admin Tools refuses of an action that
  changes something, and ends each line with `done` or `refused`. A run
  Enshwick refuses first, for an unknown player or item or a caller without
  the right, is not in it.
- A give, a chest, a clear or a copy the game later turns down, in whole or
  in part, adds a line to the audit log, so the console and RCON tools see
  it too.
- The item comes before the count in `give`, `chest` and `chestat`, and the
  count may be left out for one item.
- In a game a player hosts, items are also named by the name the game shows,
  in any language it offers, and an unknown name lists those names near it.
- `/adm help` lists the actions under groups, one line each, and lists offer
  the administrator too, for actions they may use on themselves.
- A banned or unlisted account is turned away the moment it joins.
- A backup copies only the world's save, mod settings and mod data, never
  the game's rolling copies or folders set aside, and waits for a save that
  is being written.
- The console and RCON tools read where a backup's result is logged; a
  player reads it in their own language.
- The remote console waits without polling, closes its connections at once
  when its settings change, takes its port again reliably, and lets at most
  two connections come from one address.
- `/adm world` names each game setting and value as the game's world
  settings do, in the reader's language, and long replies such as
  `/adm status` arrive as a list in chat.
- Short replies read naturally, such as "Time: 06:30" and "The world is saving."
- Give and Chest in the window list the game's items by the names the
  administrator's own game shows, with a search field, also on a dedicated
  server. The list holds the game's own items; a mod's item is given with a
  typed command. A typed command names an item by its debug name, GUID or a
  mod's content id; an item number no longer names one. The audit log
  names an item by its debug name.
- Administrators set a player's level with `/adm stats <player> level`, and
  revive a dead player where they lie, with full health, with `/adm revive`.
- God mode has its own row in a player's window.
- Administrators uncover the whole map of a player online with
  `/adm uncover` or the Map row of the player's window. The player's game
  needs Enshwick; otherwise the answer says the map stays covered.
- Every text reads in each language the game offers: English, German,
  French, Italian, Spanish, Russian, Polish, Japanese, Korean, Brazilian
  Portuguese, Thai, Turkish, Simplified and Traditional Chinese, and
  Ukrainian. World settings use the game's own names in each language.
  Typed words, such as `clear` or `everyone`, stay English. Restart
  warnings read in the server's language, English on a dedicated server.

