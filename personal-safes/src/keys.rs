//! The keys of the mod's texts in `texts.toml`, and the names of the values
//! they fill in.

/// The Safe's item name.
pub(crate) const SAFE_NAME: &str = "safe.name";
/// The Safe's item description.
pub(crate) const SAFE_DESCRIPTION: &str = "safe.description";
/// The owner's name on a Safe.
pub(crate) const LABEL: &str = "safe.label";
/// The access window's title and hint.
pub(crate) const ACCESS: &str = "action.access";
/// The line under the access window's title.
pub(crate) const ACCESS_HELP: &str = "action.access.help";
/// The action that names who may open the Safe at hand.
pub(crate) const LIST: &str = "action.list";
/// The Safe at hand, as an action names it.
pub(crate) const SUBJECT: &str = "action.subject";
/// No Safe the player manages within reach.
pub(crate) const NO_SAFE: &str = "command.no_safe";
/// The player tried to switch themselves.
pub(crate) const SELF: &str = "command.self";
/// The player switched on is the owner.
pub(crate) const OWNER: &str = "command.owner";
/// The player's own Safe.
pub(crate) const YOURS: &str = "command.yours";
/// A Safe of a named owner.
pub(crate) const THEIRS: &str = "command.theirs";
/// A Safe of an owner whose name is unknown.
pub(crate) const UNKNOWN: &str = "command.unknown";
/// A player may now open the Safe.
pub(crate) const ALLOWED: &str = "command.allowed";
/// A player could open the Safe already.
pub(crate) const ALREADY: &str = "command.already";
/// A player may no longer open the Safe.
pub(crate) const DENIED: &str = "command.denied";
/// A player switched off was not allowed.
pub(crate) const NOT_ALLOWED: &str = "command.not_allowed";
/// Who may open the Safe.
pub(crate) const NAMES: &str = "command.list";
/// Nobody else may open the Safe.
pub(crate) const NAMES_EMPTY: &str = "command.list_empty";
/// Another mod keeps the owner from managing their Safe.
pub(crate) const REFUSED: &str = "command.refused";
/// A label language that is no language code.
pub(crate) const UNKNOWN_LANGUAGE: &str = "config.label_language.unknown";

/// Every key the mod says.
#[cfg(test)]
pub(crate) const ALL: &[&str] = &[
    SAFE_NAME,
    SAFE_DESCRIPTION,
    LABEL,
    ACCESS,
    ACCESS_HELP,
    LIST,
    SUBJECT,
    NO_SAFE,
    SELF,
    OWNER,
    YOURS,
    THEIRS,
    UNKNOWN,
    ALLOWED,
    ALREADY,
    DENIED,
    NOT_ALLOWED,
    NAMES,
    NAMES_EMPTY,
    REFUSED,
    UNKNOWN_LANGUAGE,
];

/// The names of the values the texts fill in.
pub(crate) mod value {
    /// The owner's name.
    pub(crate) const OWNER: &str = "owner";
    /// A player's name.
    pub(crate) const NAME: &str = "name";
    /// The names of the players allowed.
    pub(crate) const NAMES: &str = "names";
    /// Whose Safe it is.
    pub(crate) const SAFE: &str = "safe";
    /// Where the Safe stands.
    pub(crate) const AT: &str = "at";
    /// A language code as given.
    pub(crate) const CODE: &str = "code";
}
