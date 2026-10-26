//! Development-only Enshwick mods, never shipped.

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::{Load, Mod, Off};

/// Whether the mod does its work now; a settings change updates it.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// The settings, in `mod-configs/devtools.toml`.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Whether the mod does its work.
    #[config(runtime)]
    enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct Devtools;

impl Mod for Devtools {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(
            id: "devtools",
            side: SERVER,
            name: "Developer mods",
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        ENABLED.store(settings.enabled, Ordering::Relaxed);
        load.on_config_change(|settings: Settings| {
            ENABLED.store(settings.enabled, Ordering::Relaxed);
        })?;
        enshwick::info!("enabled: {}", settings.enabled);
        Ok(Self)
    }
}

enshwick::declare_mod!(Devtools);

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;

    use super::*;

    #[test]
    fn starts_with_the_settings_it_reads() {
        let loader = enshwick::test::FakeHost::new()
            .settings("enabled = false\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert!(!ENABLED.load(Ordering::Relaxed));
    }
}
