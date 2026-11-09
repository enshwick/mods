//! The world's ban list and allow-list, kept in the mod's world data so they
//! survive a restart and reach accounts while they are offline.

use enshwick::PlayerKey;
use serde::{Deserialize, Serialize};

/// The world store's file name, `accounts.toml`.
pub(crate) const STORE: &str = "accounts";

/// The accounts the world bans and allows.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Accounts {
    /// Banned accounts, in the order banned.
    pub bans: Vec<Ban>,
    /// Accounts allowed to join while the allow-list is on.
    pub allowed: Vec<Known>,
}

/// A banned account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Ban {
    /// The account.
    #[serde(with = "hex_key")]
    pub key: PlayerKey,
    /// Its player's name when banned.
    pub name: String,
    /// Who banned it.
    pub by: String,
    /// When, in seconds since 1970 (UTC).
    pub at: u64,
}

/// An account with the name its player last had.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Known {
    /// The account.
    #[serde(with = "hex_key")]
    pub key: PlayerKey,
    /// Its player's name.
    pub name: String,
}

impl Accounts {
    /// Whether `key` is banned.
    pub(crate) fn banned(&self, key: PlayerKey) -> bool {
        self.bans.iter().any(|ban| ban.key == key)
    }

    /// Whether `key` is on the allow-list.
    pub(crate) fn allowed(&self, key: PlayerKey) -> bool {
        self.allowed.iter().any(|known| known.key == key)
    }

    /// Bans `ban.key`, replacing an older ban of it; the older ban.
    pub(crate) fn ban(&mut self, ban: Ban) -> Option<Ban> {
        let older = self.unban(ban.key);
        self.bans.push(ban);
        older
    }

    /// Lifts the ban of `key`; the ban there was.
    pub(crate) fn unban(&mut self, key: PlayerKey) -> Option<Ban> {
        let at = self.bans.iter().position(|ban| ban.key == key)?;
        Some(self.bans.remove(at))
    }

    /// Puts the bans of `key` back as they were before a change the game
    /// refused: `older`, or none.
    pub(crate) fn restore(&mut self, key: PlayerKey, older: Option<Ban>) {
        self.unban(key);
        self.bans.extend(older);
    }

    /// Puts `key` on the allow-list or takes it off.
    pub(crate) fn allow(&mut self, key: PlayerKey, name: &str, on: bool) {
        self.allowed.retain(|known| known.key != key);
        if on {
            self.allowed.push(Known {
                key,
                name: name.to_owned(),
            });
        }
    }
}

/// An account key as 16 hex digits: TOML integers stop at `i64::MAX`, and
/// keys use all 64 bits.
mod hex_key {
    use enshwick::PlayerKey;
    use serde::{Deserialize, Deserializer, Serializer};

    #[expect(
        clippy::trivially_copy_pass_by_ref,
        reason = "serde's `with` hands a reference"
    )]
    pub(super) fn serialize<S: Serializer>(key: &PlayerKey, out: S) -> Result<S::Ok, S::Error> {
        out.serialize_str(&crate::account_id(*key))
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(input: D) -> Result<PlayerKey, D::Error> {
        let text = String::deserialize(input)?;
        u64::from_str_radix(&text, 16)
            .map(PlayerKey::new)
            .map_err(|_| {
                serde::de::Error::custom(format!("{text} is no account key of 16 hex digits"))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_past_the_largest_toml_integer_round_trips_as_hex() {
        let mut accounts = Accounts::default();
        accounts.ban(Ban {
            key: PlayerKey::new(u64::MAX - 1),
            name: "Bob".into(),
            by: "Ann".into(),
            at: 1_700_000_000,
        });
        accounts.allow(PlayerKey::new(2), "Cid", true);
        let text = toml::to_string(&accounts).unwrap();
        assert!(text.contains("fffffffffffffffe"), "{text}");
        assert_eq!(toml::from_str::<Accounts>(&text).unwrap(), accounts);
    }

    #[test]
    fn a_ban_replaces_an_older_one_and_unban_says_whether_there_was_one() {
        let mut accounts = Accounts::default();
        let ban = |by: &str| Ban {
            key: PlayerKey::new(7),
            name: "Bob".into(),
            by: by.into(),
            at: 0,
        };
        assert_eq!(accounts.ban(ban("Ann")), None);
        assert_eq!(accounts.ban(ban("Cid")), Some(ban("Ann")));
        assert_eq!(accounts.bans, [ban("Cid")]);
        assert!(accounts.banned(PlayerKey::new(7)));
        assert_eq!(accounts.unban(PlayerKey::new(7)), Some(ban("Cid")));
        assert_eq!(accounts.unban(PlayerKey::new(7)), None);
    }

    #[test]
    fn restore_puts_back_the_ban_a_refused_change_replaced_or_lifted() {
        let mut accounts = Accounts::default();
        let ban = |by: &str| Ban {
            key: PlayerKey::new(7),
            name: "Bob".into(),
            by: by.into(),
            at: 0,
        };
        let older = accounts.ban(ban("Ann"));
        accounts.restore(PlayerKey::new(7), older);
        assert!(!accounts.banned(PlayerKey::new(7)), "a new ban goes again");
        accounts.ban(ban("Ann"));
        let older = accounts.ban(ban("Cid"));
        accounts.restore(PlayerKey::new(7), older);
        assert_eq!(accounts.bans, [ban("Ann")]);
        let lifted = accounts.unban(PlayerKey::new(7));
        accounts.restore(PlayerKey::new(7), lifted);
        assert_eq!(accounts.bans, [ban("Ann")]);
    }

    #[test]
    fn the_allow_list_takes_an_account_once_and_lets_it_go() {
        let mut accounts = Accounts::default();
        accounts.allow(PlayerKey::new(3), "Dee", true);
        accounts.allow(PlayerKey::new(3), "Dee", true);
        assert_eq!(accounts.allowed.len(), 1);
        accounts.allow(PlayerKey::new(3), "Dee", false);
        assert!(!accounts.allowed(PlayerKey::new(3)));
    }

    #[test]
    fn a_key_that_is_not_hex_is_refused_with_its_text() {
        let refused =
            toml::from_str::<Accounts>("[[bans]]\nkey = \"xyz\"\nname = \"\"\nby = \"\"\nat = 0\n")
                .unwrap_err();
        assert!(refused.to_string().contains("xyz"), "{refused}");
    }
}
