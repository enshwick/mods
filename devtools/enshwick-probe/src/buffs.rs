//! Buffs: the types of the running game, and on an entity that holds buffs
//! a buff given, read back and removed.

use std::time::Duration;

use enshwick::buffs::{Apply, BuffSlotType, Buffs};
use enshwick::schedule::Scheduler;
use enshwick::world::World;
use enshwick::{Load, Off};

/// How long the probe's buff lasts.
const LASTING: Duration = Duration::from_secs(30);
/// How long after a removal the probe looks again.
const SETTLE: Duration = Duration::from_secs(1);

/// The buff checks.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuffChecks {
    buffs: Buffs,
    world: World,
    scheduler: Scheduler,
}

impl BuffChecks {
    pub(crate) fn new(load: &mut Load<'_>, scheduler: Scheduler) -> Result<Self, Off> {
        let world = load.world()?;
        Ok(Self {
            buffs: load.buffs(),
            world,
            scheduler,
        })
    }

    pub(crate) fn run(self) {
        let types = self.buffs.types();
        if types.is_empty() {
            enshwick::warn!("FAIL buff types: none");
            return;
        }
        let saved = types.iter().filter(|ty| ty.is_saved).count();
        enshwick::info!("PASS buff types: {} ({saved} saved)", types.len());
        let Some(holder) = crate::living(self.world)
            .into_iter()
            .find(|&entity| self.buffs.of(entity).is_ok())
        else {
            enshwick::info!("SKIP buffs: no entity in the world holds buffs");
            return;
        };
        let Some(ty) = types
            .iter()
            .find(|ty| ty.slot == BuffSlotType::Buff && !ty.hide_in_ui)
        else {
            enshwick::warn!("FAIL buffs: no shown buff type");
            return;
        };
        match self.buffs.of(holder) {
            Ok(on) => enshwick::info!("PASS buffs of {holder:?}: {on:?}"),
            Err(why) => enshwick::warn!("FAIL buffs of {holder:?}: {why}"),
        }
        if let Err(why) = self
            .buffs
            .apply(holder, ty.buff_type_id, Apply::new().lasting(LASTING))
        {
            enshwick::warn!("FAIL buff {:?} on {holder:?}: {why}", ty.buff_type_id);
            return;
        }
        match self.buffs.of(holder) {
            Ok(on) if on.iter().any(|b| b.type_id == ty.buff_type_id) => {
                enshwick::info!(
                    "PASS buff {:?} given to {holder:?}: {on:?}",
                    ty.buff_type_id
                );
            }
            other => enshwick::warn!("FAIL buff {:?} given reads {other:?}", ty.buff_type_id),
        }
        match self.buffs.remove(holder, ty.buff_type_id) {
            Ok(true) => {}
            other => {
                enshwick::warn!("FAIL buff {:?} removal: {other:?}", ty.buff_type_id);
                return;
            }
        }
        let id = ty.buff_type_id;
        self.scheduler.after(SETTLE, move || {
            if self.buffs.has(holder, id) {
                enshwick::warn!("FAIL buff {id:?} still on {holder:?} after removal");
            } else {
                enshwick::info!("PASS buff {id:?} removed from {holder:?}");
            }
        });
    }
}
