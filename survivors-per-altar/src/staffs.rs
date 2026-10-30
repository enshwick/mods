//! Copy staffs: one item per workshop survivor, a clone of the survivor's
//! summoning staff with charges of its own.
//!
//! The platform files each copy staff as a staff of its survivor, so the game
//! treats it as the vanilla staff on the server and the client. Each staff
//! keeps its charges in its meter, which the game saves and draws as a bar.
//! The policy spends one per use that works and marks the use of the last
//! charge as consuming, which makes the game take the staff in the same
//! transaction as the spawn or move, and only when that works.

use enshwick::content::Id;
use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::items::{FuelSetup, ImpactUiValue, NewItem, StatFormat};
use enshwick::i18n::{Language, Texts};
use enshwick::state::ModLocal;
use enshwick::survivors::{WORKSHOP_SURVIVORS, WorkshopSurvivor};
use enshwick::{ItemId, Load, Off};

/// The mod's texts, per language.
pub(crate) const TEXTS: &str = include_str!("texts.toml");

/// The template of every copy staff's own entity, which keeps its charges.
const STAFF_ENTITY: Id = Id::new("enshwick.survivors-per-altar:copy-staff.entity");
/// The colour copy staffs' icons take, so they stand apart from the
/// survivors' own staffs.
const ICON_TINT: Tint = Tint {
    colour: [150, 110, 255],
    amount: 55,
};
/// The tag of the tooltip line with a staff's full charges.
const FULL_CHARGES: Id = Id::new("enshwick.survivors-per-altar:copy-staff.full-charges");

/// The namespace and folder of every copy staff's ids.
const STAFF_IDS: &str = "enshwick.survivors-per-altar:copy-staff/";

/// A workshop survivor and its copy staff.
#[derive(Debug)]
pub(crate) struct Staff {
    /// The survivor the staff copies.
    pub survivor: &'static WorkshopSurvivor,
    /// The copy staff.
    pub copy: Id,
    /// The tag of the copy staff's name.
    pub name: Id,
    /// The tag of its description.
    pub description: Id,
    /// Its icon: the vanilla staff's, recoloured.
    pub icon: Id,
}

impl Staff {
    /// The survivor's key in the platform's table.
    pub(crate) const fn key(&self) -> &'static str {
        self.survivor.key()
    }

    /// The copy staff's game id.
    pub(crate) const fn item(&self) -> ItemId {
        ItemId::from_guid(self.copy.guid())
    }

    fn of(survivor: &'static WorkshopSurvivor) -> Option<Self> {
        let id = |suffix: &str| {
            let name = format!("{STAFF_IDS}{}{suffix}", survivor.key());
            Id::try_new(Box::leak(name.into_boxed_str()))
        };
        Some(Self {
            survivor,
            copy: id("")?,
            name: id(".name")?,
            description: id(".description")?,
            icon: id(".icon")?,
        })
    }
}

static STAFFS: ModLocal<Vec<Staff>> = ModLocal::new();

/// A copy staff for every workshop survivor; the ids are the mod's own.
pub(crate) fn staffs() -> &'static [Staff] {
    STAFFS.get_or_init(|| WORKSHOP_SURVIVORS.iter().filter_map(Staff::of).collect())
}

/// The copy staff with the game id `item`.
pub(crate) fn copy_staff(item: ItemId) -> Option<&'static Staff> {
    staffs().iter().find(|s| s.item() == item)
}

/// The game ids of every copy staff.
pub(crate) fn copy_ids() -> Vec<ItemId> {
    staffs().iter().map(Staff::item).collect()
}

/// `template` with `{survivor}` filled in.
fn fill(template: &str, survivor: &str) -> String {
    template.replace("{survivor}", survivor)
}

/// The survivor's name in `language` as the game shows it; without the
/// game's texts (on a server), its role from its key, such as "Blacksmith
/// assistant".
fn survivor_name(staff: &Staff, language: Language) -> String {
    if let Some(name) = staff.survivor.name(language) {
        return name.to_owned();
    }
    let role = staff.key().replace('_', " ");
    let mut chars = role.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// Adds every copy staff and its texts; a full staff holds `charges`.
pub(crate) fn add(load: &mut Load<'_>, texts: &Texts, charges: u16) -> Result<(), Off> {
    load.add_texts(texts, &[("copy.full_charges", FULL_CHARGES)])?;
    let meter = FuelSetup {
        template: STAFF_ENTITY,
        max_fuel_charges: charges,
        start: charges,
    };
    let full = ImpactUiValue {
        loca_id: FULL_CHARGES.guid(),
        value: i32::from(charges),
        value_format: StatFormat::Number,
    };
    for staff in staffs() {
        let vanilla = staff.survivor.staff();
        for (template_key, tag) in [
            ("copy.name", staff.name),
            ("copy.description", staff.description),
        ] {
            let tag = load.declare(tag)?;
            let translations: Vec<_> = texts.translations(template_key).collect();
            if translations.is_empty() {
                return Err(Off::new(format!("texts: no text for {template_key:?}")));
            }
            for (language, template) in translations {
                let survivor = survivor_name(staff, language);
                load.add_text(tag, language, &fill(template, &survivor))?;
            }
        }
        load.add_item(NewItem {
            name: Some(staff.name.guid()),
            description: Some(staff.description.guid()),
            meter: Some(meter),
            stats: vec![full],
            icon: Some(Icon {
                id: staff.icon,
                like: vanilla,
                pixels: Pixels::Tint(ICON_TINT),
            }),
            debug_name: format!("SurvivorsPerAltar_CopyStaff_{}", staff.key()),
            ..NewItem::new(staff.copy, vanilla)
        })?;
    }
    Ok(())
}

/// Files every copy staff as a staff of its survivor, beside its vanilla
/// staff, so the cursor, the unlock check and the placement treat both alike.
pub(crate) fn file(load: &mut Load<'_>) -> Result<(), Off> {
    for staff in staffs() {
        let survivor = staff.survivor.id();
        load.add_summon_item(staff.item(), survivor)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workshop_survivor_has_a_staff_with_its_own_ids() {
        let _loader = enshwick::test::FakeHost::new().install();
        let mut ids = std::collections::BTreeSet::new();
        for staff in staffs() {
            for id in [staff.copy, staff.name, staff.description, staff.icon] {
                assert!(ids.insert(ItemId::from_guid(id.guid())), "{}", staff.key());
            }
            let vanilla = ItemId::from_guid(staff.survivor.staff());
            assert_ne!(staff.item(), vanilla);
            assert_eq!(copy_staff(staff.item()).map(Staff::key), Some(staff.key()));
            assert_eq!(copy_staff(vanilla).map(Staff::key), None);
        }
        assert_eq!(staffs().len(), WORKSHOP_SURVIVORS.len());
    }

    #[test]
    fn without_the_game_s_texts_a_survivor_is_named_by_its_role() -> Result<(), Off> {
        let smith = staffs()
            .iter()
            .find(|s| s.key() == "blacksmith_assistant")
            .ok_or_else(|| Off::new("no blacksmith assistant"))?;
        assert_eq!(
            survivor_name(smith, Language::UK_UA),
            "Blacksmith assistant"
        );
        Ok(())
    }

    #[test]
    fn names_fill_in_the_survivor_in_all_languages() -> Result<(), Off> {
        let texts = Texts::parse(TEXTS)?;
        assert_eq!(texts.untranslated(), []);
        for language in texts.languages() {
            for key in ["copy.name", "copy.description"] {
                let template = texts
                    .get(key, language)
                    .ok_or_else(|| Off::new(format!("{key} has no {}", language.code())))?;
                assert!(template.contains("{survivor}"), "{key} {}", language.code());
                assert!(!fill(template, "Balthazar").contains('{'));
            }
        }
        Ok(())
    }
}
