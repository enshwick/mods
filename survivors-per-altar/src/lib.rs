//! A copy of each survivor at every Flame Altar: a cluster of N altars holds at
//! most N copies of a survivor, each copy stands inside a zone, works there and
//! sleeps in a bed of its own altar.
//!
//! A copy staff places a copy at an altar without one and moves the copy an
//! altar has; the survivor's own staff moves the original, as in vanilla, and
//! a copy where it arrives gives way.

mod copies;
mod cursor;
pub mod points;
mod policy;
mod renew;
mod staffs;
#[cfg(test)]
mod tests;

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::abi::Side;
use enshwick::config::ConfigError;
use enshwick::loot::Drop;
use enshwick::state::ModLocal;
use enshwick::{ItemId, Load, Mod, Off};

use enshwick::Relock;

/// The text refusing fewest charges above the full count, with `{min}` and
/// `{full}`.
const CHARGES_MIN_ABOVE_FULL: &str = "config.copy_staff_charges_min.above_full";

/// The settings in force, as the game's hooks read them: the server's on a
/// joined client.
#[derive(Debug)]
pub(crate) struct InForce {
    /// Whether copy staffs work and drop.
    pub(crate) copy_staffs: AtomicBool,
    /// Whether the survivor's own staff places and moves copies.
    pub(crate) vanilla_places: AtomicBool,
    /// Whether the world keeps a copy of each survivor per altar; off, every
    /// answer is vanilla.
    pub(crate) per_altar: AtomicBool,
    /// The copy staff drops: the ones the settings ask for while copy staffs
    /// are on, none while they are off.
    drops: Mutex<Vec<Drop>>,
}

impl Default for InForce {
    fn default() -> Self {
        Self {
            copy_staffs: AtomicBool::new(true),
            vanilla_places: AtomicBool::new(false),
            per_altar: AtomicBool::new(true),
            drops: Mutex::new(Vec::new()),
        }
    }
}

static IN_FORCE: ModLocal<InForce> = ModLocal::new();

/// The settings in force in this mod life.
pub(crate) fn in_force() -> &'static InForce {
    IN_FORCE.get_or_init(InForce::default)
}

const fn yes() -> bool {
    true
}

const fn boss_chance() -> f64 {
    0.10
}

const fn locked_chest_chance() -> f64 {
    0.02
}

const fn full_charges() -> u16 {
    3
}

/// Survivors per altar settings, in `mod-configs/enshwick.survivors-per-altar.toml`.
#[enshwick::config]
#[derive(Debug)]
#[config(validate = Settings::check)]
struct Settings {
    /// A copy of each survivor at every altar, at most one per altar zone.
    /// Off, survivors and staffs behave as in vanilla in that world. A
    /// joined player runs with the server's value.
    #[config(startup, sync)]
    per_altar: bool,
    /// Copy staffs: one per workshop survivor. Each new copy it places
    /// spends a charge and the last charge uses the staff up; moving the
    /// copy an altar has is free. Off, they neither drop nor work, and
    /// staffs already held stay. A joined player runs with the server's
    /// value.
    #[serde(default = "yes")]
    #[config(sync)]
    copy_staffs: bool,
    /// The chance a boss's corpse holds a copy staff, from 0 to 1.
    #[serde(default = "boss_chance")]
    #[config(runtime)]
    copy_staff_boss_chance: f64,
    /// The chance a chest that needs a key or a lockpick holds one.
    #[serde(default = "locked_chest_chance")]
    #[config(runtime)]
    copy_staff_locked_chest_chance: f64,
    /// The charges a copy staff holds when full.
    #[serde(default = "full_charges")]
    #[config(restart, range = 1..=100)]
    copy_staff_charges: u16,
    /// The fewest charges a dropped staff holds, at random up to
    /// the full count; 0 drops every staff full.
    #[serde(default)]
    #[config(runtime, range = 0..=100)]
    copy_staff_charges_min: u16,
    /// Whether the survivor's own staff places a copy at an altar without
    /// one and moves the copy an altar has, as before copy staffs; off, it
    /// moves the survivor itself, as in vanilla.
    #[serde(default)]
    #[config(runtime)]
    vanilla_staff_places_copies: bool,
}

/// How many charges a staff holds when full, and the fewest a dropped one
/// holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Charges {
    pub(crate) full: u16,
    pub(crate) min: u16,
}

impl Settings {
    /// Refuses a fewest count above the full one.
    fn check(&self) -> Result<(), ConfigError> {
        if self.copy_staff_charges_min > self.copy_staff_charges {
            return Err(ConfigError::text(
                CHARGES_MIN_ABOVE_FULL,
                &[
                    ("min", &self.copy_staff_charges_min.to_string()),
                    ("full", &self.copy_staff_charges.to_string()),
                ],
            ));
        }
        Ok(())
    }

    /// The staffs' charges.
    fn charges(&self) -> Charges {
        let full = self.copy_staff_charges.max(1);
        let min = match self.copy_staff_charges_min {
            0 => full,
            min => min.min(full),
        };
        Charges { full, min }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            per_altar: true,
            copy_staffs: yes(),
            copy_staff_boss_chance: boss_chance(),
            copy_staff_locked_chest_chance: locked_chest_chance(),
            copy_staff_charges: full_charges(),
            copy_staff_charges_min: 0,
            vanilla_staff_places_copies: false,
        }
    }
}

/// `chance` within 0 to 1, warning about a value outside, such as `10` typed
/// for 10 %: a setting out of range never turns the copies off.
fn chance(name: &str, chance: f64) -> f64 {
    if (0.0..=1.0).contains(&chance) {
        return chance;
    }
    let kept = if chance > 1.0 { 1.0 } else { 0.0 };
    enshwick::warn!("{name} must be from 0 to 1, not {chance}: {kept} is used");
    kept
}

/// The mod's state.
#[derive(Debug)]
pub struct SurvivorsPerAltar;

impl Mod for SurvivorsPerAltar {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        // The staffs stay in the game when they are off, so a value that
        // changes while it runs, or the server's on a joined client, finds
        // them.
        let loot = copy_staffs(load, &settings)?;
        apply(&settings, &loot);
        load.on_config_change(move |settings: Settings| apply(&settings, &loot))?;
        let staffs = policy::Staffs {
            charges: settings.charges(),
        };
        // Renewals act in the dedicated server and in the games a client
        // hosts, solo play included: the scheduler and the world's events
        // run only there.
        let server = load.host().build().side == Side::SERVER;
        if server {
            policy::link(load, staffs)?;
        }
        if let Err(off) = renew::link(load) {
            enshwick::warn!("copy staffs from before charges stay as they are: {off}");
        }
        if !server {
            // The policy acts in the games this client hosts, and for its own
            // cursor.
            if let Err(off) = policy::link(load, staffs) {
                enshwick::warn!("copies stay off in hosted games: {off}");
            }
            // The menu keeps one record per survivor, which every copy writes
            // its status into in turn: show the bits every copy has, the
            // worst status, so a copy without a bed shows "no bed".
            load.on_survivor_status(|_, statuses| {
                (in_force().per_altar.load(Ordering::Relaxed) && statuses.len() >= 2)
                    .then(|| statuses.iter().fold(u8::MAX, |all, s| all & s))
            })?;
        }
        Ok(Self)
    }
}

/// The copy staffs' items and full charges, set as the game starts.
#[derive(Clone, Debug)]
struct Loot {
    items: Vec<ItemId>,
    full: u16,
}

impl Loot {
    /// The drops `settings` ask for: one per source with a chance above 0.
    fn drops(&self, settings: &Settings) -> Vec<Drop> {
        use enshwick::loot::Source;

        let min = settings.charges().min.min(self.full);
        let boss = chance("copy_staff_boss_chance", settings.copy_staff_boss_chance);
        let locked = chance(
            "copy_staff_locked_chest_chance",
            settings.copy_staff_locked_chest_chance,
        );
        [(boss, Source::bosses()), (locked, Source::locked_chests())]
            .into_iter()
            .filter(|(chance, _)| *chance > 0.0)
            .map(|(chance, source)| {
                Drop::one_of(&self.items, chance, source).meter(min, self.full, self.full)
            })
            .collect()
    }
}

/// Puts the copy staff settings in force: whether they work, their drops
/// and whether the survivor's own staff places copies.
fn apply(settings: &Settings, loot: &Loot) {
    let per_altar = settings.per_altar;
    if in_force().per_altar.swap(per_altar, Ordering::Relaxed) != per_altar {
        enshwick::info!("a copy of each survivor per altar: {per_altar}");
    }
    let on = settings.copy_staffs;
    if in_force().copy_staffs.swap(on, Ordering::Relaxed) != on {
        enshwick::info!("copy staffs {}", if on { "on" } else { "off" });
    }
    let vanilla = settings.vanilla_staff_places_copies;
    if in_force().vanilla_places.swap(vanilla, Ordering::Relaxed) != vanilla {
        enshwick::info!("the survivor's own staff places copies: {vanilla}");
    }
    let drops = if on && per_altar {
        loot.drops(settings)
    } else {
        Vec::new()
    };
    let mut kept = in_force().drops.relock();
    if *kept == drops {
        return;
    }
    match enshwick::loot::replace_drops(drops.clone()) {
        Ok(()) => {
            let chances: Vec<f64> = drops.iter().map(|d| d.chance).collect();
            enshwick::info!("copy staff drops at {chances:?}");
            *kept = drops;
        }
        Err(off) => enshwick::warn!("copy staff drops stay: {off}"),
    }
}

/// Adds the copy staffs, files them with their survivors and links a drop
/// for each source, so a world whose settings turn one on finds it linked;
/// [`apply`] then puts the drops the settings ask for in force.
fn copy_staffs(load: &mut Load<'_>, settings: &Settings) -> Result<Loot, Off> {
    use enshwick::loot::Source;

    let texts = load.texts()?;
    let charges = settings.charges();
    staffs::add(load, &texts, charges.full)?;
    staffs::file(load)?;
    let loot = Loot {
        items: staffs::copy_ids(),
        full: charges.full,
    };
    let mut kept = in_force().drops.relock();
    for source in [Source::bosses(), Source::locked_chests()] {
        let drop =
            Drop::one_of(&loot.items, 0.0, source).meter(charges.full, charges.full, charges.full);
        if let Err(off) = load.add_loot(drop.clone()) {
            enshwick::warn!("copy staffs drop from no loot: {off}");
            break;
        }
        kept.push(drop);
    }
    drop(kept);
    enshwick::info!(
        "{} copy staffs of {} to {} charges",
        loot.items.len(),
        charges.min,
        charges.full,
    );
    Ok(loot)
}

enshwick::declare_mod!(SurvivorsPerAltar, points: [points::CopyLimit, points::ChargeCost]);
