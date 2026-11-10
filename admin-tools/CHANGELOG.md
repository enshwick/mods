# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry says what players notice; a release's notes show its section.

## [Unreleased]

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
  password Admin Tools generates; a world setting opens it to other
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
- A restart warning saves the world when its time is up; a game a player
  hosts offers no restart warning.
- Administrators give a player items by the game's name in any case, its
  GUID, a mod's content id or its number, in as many stacks as needed, and
  read how many did not fit and were left out. An unknown name answers with the names near it.
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
- The audit log also keeps every refused run of an action that changes
  something, and ends each line with `done` or `refused`.
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
- The save before a restart waits for a running backup instead of being
  lost, and the console and RCON tools read where a backup's result is
  logged; a player reads it in their own language.
- The remote console waits without polling, closes its connections at once
  when its settings change, takes its port again reliably, and lets at most
  two connections come from one address.
- `/adm world` names each game setting and value as the game's world
  settings do, in the reader's language, and long replies such as
  `/adm status` arrive as a list in chat.
- Short replies read naturally, such as "Time: 06:30" and "World saved."
