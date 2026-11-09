//! Admin Tools through the fake loader: each action as an administrator, a
//! player and the console run it, the check at the door, the schedule, the
//! logs, backups and the remote console.

#[cfg(test)]
#[path = "actions/game.rs"]
mod game;
#[cfg(test)]
#[path = "actions/players.rs"]
mod players;
#[cfg(test)]
#[path = "actions/rcon.rs"]
mod rcon;
#[cfg(test)]
#[path = "actions/world.rs"]
mod world;
