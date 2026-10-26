//! Flame Altar zones that touch form one cluster: every member grows by the
//! same amount until the cluster has the area of its members' zones without
//! losing any to overlap, no zone ever shrinks, and pieces can be built across
//! the seams between members.
//!
//! The zones are solved on the server, which the game trusts for them, and by
//! a client's built-in server while it hosts a game; the seam check runs on
//! both sides, since the client predicts placement. The platform follows the
//! altars and writes the sizes; this mod only decides them.

mod seam;
mod solver;

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::altars::ZoneControl;
use enshwick::events::{AltarsChanged, ShutdownStarted};
use enshwick::state::ModLocal;
use enshwick::{Load, Off};

static SEAMS: ModLocal<AtomicBool> = ModLocal::new();

/// Whether pieces may be placed across seams, from the settings in force:
/// the server's on a joined client.
fn seams() -> &'static AtomicBool {
    SEAMS.get_or_init(|| AtomicBool::new(Zones::default().building_across_seams))
}

/// How the zones of touching altars grow and join.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick::Config)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Zones {
    /// Altars whose zones touch grow together and share their area. Each
    /// world's own value applies when it loads.
    #[config(startup)]
    pub(crate) clusters: bool,
    /// Pieces may be placed across the seams between the zones of one cluster.
    /// A joined player runs with the server's value.
    #[config(sync)]
    pub(crate) building_across_seams: bool,
}

impl Default for Zones {
    fn default() -> Self {
        Self {
            clusters: true,
            building_across_seams: true,
        }
    }
}

/// The altars' zones under the mod's control.
#[derive(Debug)]
pub(crate) struct AltarZones {
    control: ZoneControl,
}

impl AltarZones {
    /// Links the seam check and takes the altars' zones, with `zones` the
    /// settings in force.
    pub(crate) fn start(load: &mut Load<'_>, zones: &Zones) -> Result<Self, Off> {
        seams().store(zones.building_across_seams, Ordering::Relaxed);
        // Always linked: the server's value may turn seams on for a joined
        // client, and a change applies while the game runs.
        load.on_zone_check(|check| seams().load(Ordering::Relaxed) && seam::accepts(check))?;
        // Taken whatever the settings say: each world's own value turns
        // clusters on or off as it loads.
        let control = load.control_altar_zones()?;
        solver::switch(control, zones.clusters);
        load.on::<AltarsChanged>(move |()| solver::request(control))?;
        // The loader drops the sizes with a hosted game; the next game,
        // the same world again included, is solved afresh.
        load.on::<ShutdownStarted>(|()| solver::forget())?;
        Ok(Self { control })
    }

    /// Takes `zones`, the settings now in force.
    pub(crate) fn apply(&self, zones: &Zones) {
        let on = zones.building_across_seams;
        if seams().swap(on, Ordering::Relaxed) != on {
            enshwick::info!("building across seams {on}");
        }
        solver::switch(self.control, zones.clusters);
    }

    /// Logs the seam setting in force once the game runs.
    pub(crate) fn ready() {
        enshwick::info!("building across seams {}", seams().load(Ordering::Relaxed));
    }

    /// Turns building across seams on or off, as the settings would.
    #[cfg(test)]
    pub(crate) fn set_seams(on: bool) {
        seams().store(on, Ordering::Relaxed);
    }
}
