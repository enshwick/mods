# Survivors per Altar

A survivor can have a copy at every Flame Altar. Each player's base can have its
own Blacksmith, Alchemist and other survivors.

Built on [Enshwick](https://github.com/enshwick/enshwick).

- A cluster of N altars holds at most N copies of a survivor, one per altar
  zone, and each copy stands inside a zone.
- The survivor's own summoning staff works as in vanilla. It moves the survivor
  itself, the original, however many copies stand in the world. In an altar zone
  that holds a copy, the original moves in and that copy is removed, so a zone
  holds one survivor. Copies elsewhere stay where they are.
- Copy staffs make copies. Each workshop survivor has its own copy staff, such
  as "Copy staff: Oswald Anders". Its icon is the survivor's staff in violet.
  The survivor's name comes from the game in the player's language, spelled as
  the game spells it ("Посох копії: Освальд Андерс" in Ukrainian). The staff
  acts on the altar zone it is aimed at:
  - a zone without a copy and without the original gets a new copy, and the
    staff spends a charge;
  - a zone with a copy: the staff moves that copy to where it is aimed, for
    free;
  - a zone where the original stands: the staff is refused and spends nothing;
  - outside every altar zone: the staff is refused and spends nothing.

  A survivor never has more entities than there are altars, the original
  included.
- Each copy works where it stands and sleeps in a bed of its own altar. The mod
  never gives it a bed at another altar. A bed it held at another altar is
  cleared.
- The survivor menu shows the worst status of the copies. "No bed" means that at
  least one copy still needs a bed.
- When an altar is removed, the last one too, its copies go with its zone ten
  seconds later. The original stays where it is. After a load, a copy stays
  while the game has not listed its altar yet. It also stays when its spot lies
  in the grown zone of another altar that is already listed.
- Without the mod, the game removes the copies.
- A full copy staff holds 3 charges. Its slot shows them as a bar, and its
  tooltip names how many it holds when full. When the last charge places a copy,
  the staff is used up.
- Bosses drop a copy staff at a 10 % chance. Chests that need a key or a
  lockpick hold one at a 2 % chance. Each staff is for a random survivor. A
  dropped staff is full, or holds a random count when `copy_staff_charges_min`
  is above 0.
- A staff from before charges gets full charges when the world loads and when
  its player joins. In a built chest or a world chest, it gets them within a
  minute. This works on a dedicated server, in single player and in a hosted
  game.
- Without the mod, the game removes the staffs from inventories.

## Settings

`mod-configs/enshwick.survivors-per-altar.toml`, written on first start. Each
world keeps its own copy of it (see
[Settings](https://github.com/enshwick/enshwick#settings)):

```toml
per_altar = true
copy_staffs = true
copy_staff_charges = 3
copy_staff_charges_min = 0
copy_staff_boss_chance = 0.10
copy_staff_locked_chest_chance = 0.02
vanilla_staff_places_copies = false
```

- `per_altar`: `false` keeps survivors and staffs as in vanilla in that world.
  Copy staffs neither drop nor work there.
- `copy_staffs`: whether copy staffs drop and work.
- `copy_staff_charges`: the charges of a full copy staff, 1 to 100.
- `copy_staff_charges_min`: a dropped staff holds a random count from this value
  up to `copy_staff_charges`. 0 drops every staff full. It may not be above
  `copy_staff_charges`. The Mods screen refuses such a value and says why in the
  player's language.
- `copy_staff_boss_chance`: the chance that a boss's corpse holds a staff, from
  0 to 1. 0 turns this source off.
- `copy_staff_locked_chest_chance`: the chance that a chest which needs a key or
  a lockpick holds a staff, from 0 to 1. 0 turns this source off.
- For both chances, a value above 1 counts as 1 and a value below 0 counts as 0.
  The log shows a warning.
- `vanilla_staff_places_copies`: `true` lets the survivor's own staff place a
  copy at an altar without one and move the copy an altar has, as before copy
  staffs. `false` moves the survivor itself, as in vanilla. A new copy keeps to
  the same limit as with a copy staff: the staff is refused when the copy would
  give the survivor more entities than there are altars.

`copy_staffs` belongs to the server, and a joined player runs with the server's
value. A change applies while the game runs. When it is off, copy staffs neither
drop nor work. Players keep the staffs they hold.

The chances, `copy_staff_charges_min` and `vanilla_staff_places_copies` also
apply while the game runs. Each world's own values apply when it loads.

`per_altar` applies when the world loads. A joined player runs with the server's
value, so the bed cursor and the survivor menu follow the server's world.

`copy_staff_charges` is part of the staff item that the game builds at start.
The mod reads it from `mod-configs/enshwick.survivors-per-altar.toml` for every
world. A change applies when the game starts again.

## Install

Download
[`enshwick-survivors-per-altar.zip`](https://github.com/enshwick/mod-survivors-per-altar/releases/download/dev/enshwick-survivors-per-altar.zip)
and extract it into the folder that holds `enshrouded.exe` or
`enshrouded_server.exe`. It needs the Enshwick loader:
[`enshwick-loader.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick-loader.zip)
alone, or
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
with every mod. These are development builds of the newest commit. Until the
first release, 1.0.0, there are no other builds.

## Who needs it

The server and every player. The mod counts in the join check. It reads the
altar zones from the platform. It works with or without Better Altars and
follows the grown zones of Better Altars when that mod runs.

## For mod authors

Other mods change how many copies of a survivor the altars hold at the extension
point `enshwick.survivors-per-altar:copy-limit`. They change how many charges a
new copy costs at `enshwick.survivors-per-altar:charge-cost`, and there they may
also refuse the staff's use. Both name the survivor by its `NpcId`. See
[Extending other mods](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md).

Licensed under [MIT](LICENSE).
