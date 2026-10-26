//! Copy staffs: one item per workshop survivor, a clone of the survivor's
//! summoning staff with charges of its own.
//!
//! The platform files each copy staff as a staff of its survivor, so the game
//! treats it as the vanilla staff on the server and the client. Each staff
//! keeps its charges in its meter, which the game saves and draws as a bar.
//! The policy spends one per use that works and marks the use of the last
//! charge as consuming, which makes the game take the staff in the same
//! transaction as the spawn or move, and only when that works.

use enshwick::content::icons::{Icon, Pixels, Tint};
use enshwick::content::items::{FuelSetup, ImpactUiValue, NewItem, StatFormat};
use enshwick::content::{Guid, Id};
use enshwick::i18n::{Language, Texts};
use enshwick::migrations::Migrations;
use enshwick::state::ModLocal;
use enshwick::survivors::{WORKSHOP_SURVIVORS, WorkshopSurvivor};
use enshwick::{Load, Off};

/// The mod's texts, per language.
pub(crate) const TEXTS: &str = include_str!("texts.toml");

/// The template of every copy staff's own entity, which keeps its charges.
const STAFF_ENTITY: Id = Id::new("survivors-per-altar:copy-staff.entity");
/// The GUID the template had before content ids; saved staffs name it.
const OLD_STAFF_ENTITY: Guid = Guid::parse("79250fcb-6170-4767-bfbb-2745e36e905a");
/// The colour copy staffs' icons take, so they stand apart from the
/// survivors' own staffs.
const ICON_TINT: Tint = Tint {
    colour: [150, 110, 255],
    amount: 55,
};
/// The tag of the tooltip line with a staff's full charges.
const FULL_CHARGES: Id = Id::new("survivors-per-altar:copy-staff.full-charges");

/// The namespace and folder of every copy staff's ids.
const STAFF_IDS: &str = "survivors-per-altar:copy-staff/";

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
    /// The GUID the copy staff had before content ids, when it had one;
    /// saved stacks keep it.
    pub old: Option<Guid>,
}

impl Staff {
    /// The survivor's key in the platform's table.
    pub(crate) const fn key(&self) -> &'static str {
        self.survivor.key()
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
            old: OLD_STAFFS
                .iter()
                .find(|(key, _)| *key == survivor.key())
                .map(|&(_, old)| old),
        })
    }
}

/// The GUIDs the copy staffs had before content ids, by survivor key: saved
/// stacks of those days hold them. A survivor the game adds later has none.
const OLD_STAFFS: &[(&str, Guid)] = &[
    (
        "blacksmith",
        Guid::parse("dac11dad-687c-4397-b528-fdec7cdb6ba1"),
    ),
    (
        "blacksmith_assistant",
        Guid::parse("10b19c0f-b86a-4bb2-96c6-5dfef7e996d3"),
    ),
    (
        "alchemist",
        Guid::parse("448e4f2a-d620-41ca-bfc6-f311320779e5"),
    ),
    (
        "alchemist_assistant",
        Guid::parse("bbecddee-022a-4506-bde1-ffe07d1b6727"),
    ),
    (
        "carpenter",
        Guid::parse("177d03e4-ac34-439a-b342-fee6973629f6"),
    ),
    (
        "carpenter_assistant",
        Guid::parse("f54ca128-3ed7-4e30-bb93-6f82d8fc0d29"),
    ),
    (
        "hunter",
        Guid::parse("8057fdfe-c990-4338-8a8d-a850a12458ab"),
    ),
    (
        "hunter_assistant",
        Guid::parse("a3bca904-6d54-4c66-a11b-8fc0f52daba5"),
    ),
    (
        "farmer",
        Guid::parse("42f978ea-d349-4f44-8796-b61bebaa7123"),
    ),
    (
        "farmer_assistant",
        Guid::parse("d5bf7763-a011-4ad0-af9b-09b67e1f5a91"),
    ),
    ("bard", Guid::parse("afbaccbe-d61d-4997-84d8-9e2c69359ee8")),
    (
        "bard_assistant",
        Guid::parse("17dfc67d-1749-4318-88f7-6aa1c48064f2"),
    ),
    (
        "crypt_keeper",
        Guid::parse("4e56d680-2fe3-497b-ad1c-40a8306b2923"),
    ),
    (
        "crypt_keeper_assistant",
        Guid::parse("9d99fcc0-7431-41a3-a1e3-9f37cd5ba8a7"),
    ),
    (
        "barber",
        Guid::parse("de8e029e-5be3-4ad9-ad7d-817538046f9a"),
    ),
    (
        "researcher",
        Guid::parse("5eaf09fd-a724-4d8e-93f4-3bf9e3395e83"),
    ),
    (
        "trader",
        Guid::parse("6181d065-0d20-4361-ab4f-2622ceefa542"),
    ),
    (
        "fisher",
        Guid::parse("b19d25af-3283-416e-8173-9506ddd8f26e"),
    ),
];

static STAFFS: ModLocal<Vec<Staff>> = ModLocal::new();

/// A copy staff for every workshop survivor; the ids are the mod's own.
pub(crate) fn staffs() -> &'static [Staff] {
    STAFFS.get_or_init(|| WORKSHOP_SURVIVORS.iter().filter_map(Staff::of).collect())
}

/// The copy staff with the game id `item`, its own or the one it had
/// before.
pub(crate) fn copy_staff(item: u32) -> Option<&'static Staff> {
    let item = enshwick::migrations::current_id(item);
    staffs().iter().find(|s| s.copy.id() == item)
}

/// The game ids of every copy staff.
pub(crate) fn copy_ids() -> Vec<u32> {
    staffs().iter().map(|s| s.copy.id()).collect()
}

/// The game ids of every copy staff, with the ids saved stacks may hold.
pub(crate) fn known_ids() -> Vec<u32> {
    staffs()
        .iter()
        .flat_map(|s| std::iter::once(s.copy.id()).chain(s.old.map(Guid::id)))
        .collect()
}

/// Every migration of the copy staffs, oldest first: the GUIDs they had
/// before content ids.
pub(crate) fn migrations() -> Migrations {
    Migrations::new().add("0.5.0", |mut m| {
        m = m.rename_content(OLD_STAFF_ENTITY, STAFF_ENTITY.guid());
        for staff in staffs() {
            if let Some(old) = staff.old {
                m = m.rename_content(old, staff.copy.guid());
            }
        }
        m
    })
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
        load.add_summon_item(staff.copy.id(), survivor)?;
        if let Some(old) = staff.old {
            load.add_summon_item(old.id(), survivor)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_workshop_survivor_has_a_staff_with_its_own_ids() {
        // A mod life of its own, with no migration declared yet.
        let _loader = enshwick::test::FakeHost::new().install();
        let mut ids = std::collections::BTreeSet::new();
        for staff in staffs() {
            for id in [staff.copy, staff.name, staff.description, staff.icon] {
                assert!(ids.insert(id.id()), "{}", staff.key());
            }
            if let Some(old) = staff.old {
                assert!(ids.insert(old.id()), "{}", staff.key());
                assert!(known_ids().contains(&old.id()), "{}", staff.key());
                assert_eq!(
                    copy_staff(old.id()).map(Staff::key),
                    None,
                    "only once the migration is declared"
                );
            }
            let vanilla = staff.survivor.staff().id();
            assert_ne!(staff.copy.id(), vanilla);
            assert_eq!(
                copy_staff(staff.copy.id()).map(Staff::key),
                Some(staff.key())
            );
            assert_eq!(copy_staff(vanilla).map(Staff::key), None);
        }
        assert_eq!(staffs().len(), WORKSHOP_SURVIVORS.len());
    }

    #[test]
    fn every_old_staff_guid_belongs_to_a_workshop_survivor() {
        for (key, _) in OLD_STAFFS {
            assert!(WorkshopSurvivor::find(key).is_some(), "{key}");
        }
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
