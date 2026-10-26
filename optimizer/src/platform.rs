//! Where the mod runs and which of its changes are on there by default.
//!
//! [`Change::on_by_default`] is the one table of what is on where. A change
//! is on for a platform only when A/B runs on test servers showed it saves
//! CPU there and changes nothing players notice; the README lists the
//! numbers. Everywhere else the change stays off unless its setting forces
//! it.

use core::fmt;

/// What the game is and what it runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    /// The dedicated server on Windows.
    WindowsServer,
    /// The dedicated server under Wine or Proton.
    WineServer,
    /// The game on Windows, hosting a game or not.
    WindowsClient,
    /// The game under Wine or Proton, hosting a game or not.
    WineClient,
}

impl Platform {
    /// The platform of a server or client under Wine or not.
    pub(crate) const fn of(server: bool, wine: bool) -> Self {
        match (server, wine) {
            (true, false) => Self::WindowsServer,
            (true, true) => Self::WineServer,
            (false, false) => Self::WindowsClient,
            (false, true) => Self::WineClient,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WindowsServer => "Windows server",
            Self::WineServer => "Wine server",
            Self::WindowsClient => "Windows client",
            Self::WineClient => "Wine client",
        })
    }
}

/// One of the mod's changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Sleeping the last fraction of a millisecond of the event loop's waits.
    ExactWaits,
    /// Capping the task workers to the container's CPU limit.
    TaskWorkers,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExactWaits => "exact waits",
            Self::TaskWorkers => "task workers capped to the CPU limit",
        })
    }
}

impl Change {
    /// The platforms the change is on by default on.
    pub(crate) const fn on_by_default(self) -> &'static [Platform] {
        match self {
            Self::ExactWaits => &[Platform::WindowsServer, Platform::WineServer],
            Self::TaskWorkers => &[Platform::WineServer],
        }
    }
}

/// Whether `change` is on by default on `platform`.
pub(crate) fn default_on(change: Change, platform: Platform) -> bool {
    change.on_by_default().contains(&platform)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_follow_the_measurements() {
        assert!(default_on(Change::ExactWaits, Platform::WindowsServer));
        assert!(default_on(Change::ExactWaits, Platform::WineServer));
        assert!(!default_on(Change::ExactWaits, Platform::WindowsClient));
        assert!(default_on(Change::TaskWorkers, Platform::WineServer));
        assert!(!default_on(Change::TaskWorkers, Platform::WindowsServer));
        assert_eq!(Platform::of(true, true), Platform::WineServer);
        assert_eq!(Platform::of(false, false).to_string(), "Windows client");
    }
}
