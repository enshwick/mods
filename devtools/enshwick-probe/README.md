# Probe

A development mod, never released: it exercises the platform API on a test
server without a player and logs what it sees to
`logs/enshwick.probe_<date>_<time>.log`, each check as a `PASS`, `FAIL` or
`SKIP` line. Keep it off servers people play on.

## What it checks

Ten seconds after the world has loaded, and again on the `check` action:

- the world key, and that a per-world store opens under it; the store keeps
  a sample of placed entities, and the next check counts how many keep their
  id and template across the save and load in between;
- entity queries: every placed entity, its position against its transform,
  and the entities near it;
- the vitals of actors with replicated health, against that health, and the
  progress of a character;
- component writes: a write to the replicated health is refused; an
  actor's health is set one lower, healed by the most negative amount (it
  stays at 1), its replicated copy checked a second later, and put back;
- spawns: a copy of an entity in the world, one with health if there is one,
  three metres above it; that it arrives where asked, its health writes when
  it has health, a teleport five metres away, and its destruction;
- mod messages: that a channel is refused to the probe, which is no join
  mod; messages themselves need a client;
- actions: a ping far from anything, which the game must hand back as a game
  event in the next step; that a type off the allow list and a teleport of
  an entity not in the world are refused;
- that destroy requests refuse entity 0 and an entity not in the world;
- the clock: a reading, a time an hour past the next turn of day or night
  that reads back (the turn's event follows), the time put back, a factor of 10 on the speed that the clock's speed shows, and the
  factor put back; the turns of day and night are logged as they come; these
  run once the weather checks are done, since moving the clock ends the fronts;
- ticks: over five seconds, an every-step handler that sees one step per
  call, `Ticked` about every 100 ms and no more often than the steps, and a
  handler of its own about every second; then its period set to 250 ms
  while the game runs, counted again, and put back;
- game settings: a typed read, the perk cost raised by a half that reads
  back with the preset custom, a night of zero refused, and the settings put
  back; then the bosses' health kept at 1.25, read back and forgotten, the
  world's own value back. With `keep_across_restart` the run keeps it for
  the next start instead, and the next run with that setting on sees it come
  back with the world and forgets it: until then the world plays with that
  boss health. Whatever the probe still keeps when the server stops in order
  is forgotten then, unless kept across the restart on purpose; a killed
  server keeps it until the next run;
- buffs: the types of the running game; on the first character, survivor
  or animal that holds buffs, its buffs, a shown buff given for 30 seconds that reads back, and
  its removal, gone a second later (skipped without such an entity);
- knowledge: the quests of the running game, a world counter that reads,
  a grant of the wrong kind and one of an unknown id refused, and the
  counter raised by one, read back a second later; knowledge changes and
  completed quests are logged as they come;
- weather: a reading at the first placed entity and the fronts; rain for two
  minutes that shows in the fronts, then clear skies, then the game's own
  choice again; chunk loads and unloads are logged as they come;
- stats: a built-in kind's definition; on the first character, survivor or
  animal with health, the probe's own kind and 100 extra maximum health set as
  modifiers, both read back a second later (the maximum through the
  entity's vitals), then removed, the maximum back where it was; stat
  changes are logged as they come;
- affixes: the first weapon or armour in any container rolled two affixes
  from a seed and again the same, its lines named, and its old stats put
  back (skipped when no such item is found, or no player was online yet);
- terrain: the materials, a dry dig at the first placed entity, a mound
  added 40 metres above it and dug away again, and a radius of 100 metres
  refused;
- a save on demand, followed by `WorldSaved`; the save on disk read, the
  first `WorldSaving`, and half a minute later the saves held, a save asked
  meanwhile refused, and the saves let go.

Its spawns stay out of the world's save. From start, its chat filter drops a
line of exactly `drop me` and keeps every other line, and it logs each line
that reaches the chat with its sender's account, its kind and its length,
never its text.

While enabled, from start it counts the damage its damage filter sees by cause,
and every game event by type, named as the game's reflection names it, and logs the counts every minute. It logs each
typed game event it receives, each fall and fog damage, and the first 200
hits a player takes or deals, with the amount before and after the filter; a
typed event whose fields this game build lacks shows as `FAIL typed <type>`.

## Actions

On the mod's page of the Mods screen, for administrators.

| Action      | What it does                                                                                                |
| ----------- | ----------------------------------------------------------------------------------------------------------- |
| `census`    | the game events counted so far, by type                                                                     |
| `check`     | runs the checks again                                                                                       |
| `key`       | your account key                                                                                            |
| `vitals`    | an entity's vitals and progress, against its replicated health                                              |
| `my-vitals` | your character's raw health, stamina and mana sets, every slot, the replicated copies and the API's reading |
| `teleport`  | moves your character by the offset given and logs where it stands four times a second for five seconds      |
| `destroy`   | asks the world to destroy an entity; on a copy of a world only                                              |
| `spawn`     | spawns a copy of an entity above it and follows it as above                                                 |
| `notice`    | shows a notice on your screen, or in chat without Enshwick                                                  |
| `knowledge` | a knowledge value of yours, or of the world for its ids                                                     |
| `learn`     | grants it to you: a bool set, a counter raised by the bit or amount, that flag set                          |
| `unlearn`   | takes it back the same way                                                                                  |
| `stat`      | adds a value to one of your stats                                                                           |
| `stat-off`  | removes what the probe added to that stat                                                                   |
| `stats`     | your stats that differ from the defaults, and their sources                                                 |
| `affix`     | rolls one to three affixes anew on everything you wear                                                      |

## Settings

`settings.toml` in the mod's own folder in `mods/`, this machine's switches for
every world. The probe reads the file itself as the game starts, so a change
takes effect when the game starts again. Without the file, the defaults apply:

```toml
enabled = false
# Refuse every craft request or every placement, to see the vetoes in game;
# the probe logs each craft request and the first placement either way.
refuse_crafting = false
refuse_building = false
# Halve every damage, to see the damage filter in game; the probe counts
# the damage it sees by cause and logs the counts every minute either way.
soften_damage = false
# Keep the bosses' health at 1.25 for the next start, to see a kept game
# setting come back with the world; the next run with this on forgets it.
keep_across_restart = false
# Run none of the checks when the world loads, only the world's scenario.
scenario_only = false
```

`enabled` is off by default: until it is `true` the probe does nothing, no
checks, counts, scenarios or actions, and the other settings have no effect.

## Check scenarios

A scenario is an in-game check written in Luau: the probe runs it in a Luau
VM of its own ten seconds after the world has loaded, one step after another,
through the platform's Luau API only. Each step logs a line, and the last line
is `scenario <name>: PASS, <n> steps` or `scenario <name>: FAIL at step <n>
<step>: <reason>`, the reason naming the values compared. A check that needs
a player tells them each thing to do in chat, then waits for what the server
sees.

The world's own setting `scenario` names the scenario, in
`<stem>-mod-settings/enshwick.probe/settings.toml` beside the world's save or
behind the probe's gear on the Mods screen; empty, the default, runs none.

| Scenario | What it checks | Needs |
| --- | --- | --- |
| `chat` | the probe's chat filter drops a line of exactly `drop me` and keeps every other line, which names its sender | one player |
| `saves` | a save held by the probe is refused, a save once let go runs `WorldSaving` before `WorldSaved`, and the save on disk follows | nothing |
| `weather` | the [test content mod](../enshwick-test-content/README.md)'s rain comes as a front in the first weather layer | the test content mod |
| `item-actions` | each item action a player takes reaches the server as its kind, and a hand craft as a craft event | one player with food, a healing potion, armour, a prop and craft materials |
| `craft-rewards` | a craft reward goes to the player who brewed, once per unit | two players and a pack that rewards each brew with an item |
| `join-throw` | a throw a pack refuses stays refused right after a join, and a pack's first-drink experience comes once | two players and such a pack |

### Run a scenario

1. Create the empty file `mods/enshwick/developer-tools` in the server's game
   folder, so the loader runs developer mods.
2. Put the probe in a folder of its own in `mods/`, and beside it its
   `settings.toml` with `enabled = true` and `scenario_only = true`.
3. Put the mods the scenario needs in `mods/`.
4. Write `scenario = "<name>"` into
   `<stem>-mod-settings/enshwick.probe/settings.toml` beside the world's
   save, where `<stem>` is the save's name without `.ksd`.
5. Start the server, or host the world in the game.
6. Read the lines that start with `scenario` in
   `logs/enshwick.probe_<date>_<time>.log` in the game folder.

### Write a scenario

A scenario is a file in [`scenarios/`](scenarios/), listed in
`src/scenarios.rs`, that returns its `about` line and its `run` function:

```luau
return {
    about = "a save on demand is written",
    run = function(s)
        s.step("a save asked runs WorldSaved", function()
            enshwick.world.save()
            s.event("WorldSaved", 120)
        end)
    end,
}
```

`run` calls the platform's Luau API through `enshwick`, and these calls of
`s`:

| Call | What it does |
| --- | --- |
| `s.step(title, body)` | runs `body` as the next step and logs its `PASS` once it returns |
| `s.expect(ok, what)` | fails the step with `what` unless `ok` |
| `s.equal(got, want, what)` | fails the step unless `got` equals `want`, naming both |
| `s.wait(seconds)` | waits that long |
| `s.ticks(steps)` | waits that many server steps |
| `s.event(event, seconds, test?)` | waits for the event whose data meets `test` and returns its data; fails the step without it |
| `s.next(event, seconds, test?)` | the same, returning `nil` instead of failing |
| `s.no_event(event, seconds, test?)` | fails the step if the event comes within `seconds` |
| `s.players(count, seconds)` | the players online once at least `count` are; fails the step without them |
| `s.tell(text)` | tells the players in chat what to do, and logs it |
| `s.give(root, item, count)`, `s.take(root, item, count)` | gives or takes items in an inventory; fails the step unless the game committed it |
| `s.items(root)` | what an inventory holds, a count per item |
| `s.changed(before, after)`, `s.describe(delta)` | the counts that changed between two `s.items`, and that as a log line |
| `s.log(text)` | logs a line beside the scenario's |

The events a scenario waits for are the loader's events by name, such as
`WorldSaved`, `PlayerJoined` and `Ticked`; the typed game events
`CraftRecipeEvent`, `CraftRecipeSuccessEvent`, `LootInteractionEvent`,
`HandleEquipmentSlotChangedEvent`, `BuildingPlaceEvent`,
`BuildingPickUpEvent`, `XpGainedEvent` and `LevelUpEvent` where the game build
has them; `ChatSaid` for each chat line; `ItemAction` for each item action;
and `FrontChanged` for each weather front that starts or ends. A failed
check, an error or a wait that runs out ends the scenario with `FAIL`; the
probe runs on.

Licensed under [MIT](../LICENSE).
