# Admin Tools

Admin Tools lets a server's administrators mute players in chat from the
game's interface.

Built on [Enshwick](https://github.com/enshwick/enshwick).

## In the game

An administrator opens the pause menu, **Mods**, **Actions** and **Muted in
chat**: a list of the players online with a switch each. A muted player's chat
lines reach no other player, and the muted player sees a notice that says so.
Administrators and the host are never muted, and a mute lasts across restarts
until it is lifted. Other players never see the list, and the server checks
every switch again.

## Install

Download
[`enshwick-admin-tools.zip`](https://github.com/enshwick/mods/releases/download/dev/enshwick-admin-tools.zip).
Extract it into the folder with `enshrouded_server.exe`. The mod needs the
Enshwick loader. Get
[`enshwick.zip`](https://github.com/enshwick/enshwick/releases/download/dev/enshwick.zip)
and extract it into the same folder. These files are development builds of the
newest commit. Until the first release, 1.0.0, there are no other builds.

## Who needs it

Only the server. Players need Enshwick to see the list.

Licensed under [MIT](LICENSE).
