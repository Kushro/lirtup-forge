//! «Pasar a modos de escalado» (round 4, front M): the dialog the GUI shows
//! after loading a 1.x configuration with loose mode fields. It says how
//! many modes it creates and which profiles name them, that a backup goes
//! to `config.yaml.1x.bak`, and offers «Migrar» (`Action::Migrate`) or
//! «Ahora no» (`Action::SkipMigration`).
//!
//! fx-unify: mode files that still use the retired native effects engine
//! ([`MigrationPlan::rewritten`]) add a paragraph (how many, which, and the
//! `<id>.yaml.bak` copies); a plan that only rewrites them is titled
//! «Actualizar los efectos» and leaves out the 1.x paragraphs ([`sections`]).

use dioxus::prelude::*;
use lirtup_config::migrate::MigrationPlan;
use lirtup_config::{ModeStore, Value};

use crate::app::{use_dispatch, use_store};
use crate::components::{Button, ButtonVariant, Dialog as DialogBox};
use crate::state::Action;

/// The dialog (`Dialog::Migrate`).
#[component]
pub fn MigrateDialog(plan: MigrationPlan) -> Element {
    let store = use_store();
    let dispatch = use_dispatch();
    let open = use_signal(|| true);
    let sections = sections(&plan);
    let rows = rows(&plan, &store.read().modes.store, &t!("nav-default-profile"));
    let rewritten = rewritten_names(&plan, &store.read().modes.store);
    let title = if sections.loose {
        t!("migrate-title")
    } else {
        t!("migrate-native-title")
    };
    let mode_backup = format!("<id>.yaml{}", lirtup_config::migrate::MODE_BACKUP_SUFFIX);
    let new_label = t!("migrate-new");
    let inherit: Vec<String> = plan
        .profiles
        .iter()
        .filter(|(_, mode)| mode.is_none())
        .map(|(name, _)| format!("«{name}»"))
        .collect();
    rsx! {
        DialogBox {
            open,
            icon: "tune",
            title,
            on_close: move |_| dispatch(Action::CloseDialog),
            actions: rsx! {
                Button { label: t!("migrate-later"), variant: ButtonVariant::Text, onclick: move |_| dispatch(Action::SkipMigration) }
                Button { label: t!("migrate-confirm"), icon: "check", onclick: move |_| dispatch(Action::Migrate) }
            },
            if sections.loose {
                p { class: "t-bm m-0", {t!("migrate-body", fields = plan.moved_fields, modes = plan.modes.len())} }
                ul { class: "list-none m-0 p-0 flex flex-col gap-1",
                    for row in rows {
                        li { class: "t-bm",
                            span { class: "t-ls", "{row.name}" }
                            if row.new {
                                span { class: "t-bs text-on-surface-variant", " · {new_label}" }
                            }
                            span { class: "t-bs text-on-surface-variant", " ← {row.users.join(\", \")}" }
                        }
                    }
                }
                if !inherit.is_empty() {
                    p { class: "t-bs m-0 text-on-surface-variant",
                        {t!("migrate-inherit", profiles = inherit.join(", "), count = inherit.len())}
                    }
                }
            }
            if let Some(count) = sections.native {
                p { class: "t-bm m-0", {t!("migrate-native-body", count = count)} }
                ul { class: "list-none m-0 p-0 flex flex-col gap-1",
                    for name in rewritten {
                        li { class: "t-bm", span { class: "t-ls", "{name}" } }
                    }
                }
                p { class: "t-bs m-0 text-on-surface-variant", {t!("migrate-native-backup", file = mode_backup)} }
            }
            if sections.loose {
                p { class: "t-bs m-0 text-on-surface-variant", {t!("migrate-backup", file = "config.yaml.1x.bak")} }
                p { class: "t-bs m-0 text-on-surface-variant", {t!("migrate-legacy-gui")} }
            }
        }
    }
}

/// Which parts of the dialog a plan shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sections {
    /// The 1.x paragraphs (loose fields into modes): title `migrate-title`,
    /// the modes and who names them, the `config.yaml` backup and the 1.x
    /// GUI settings. Without them the title is `migrate-native-title`.
    loose: bool,
    /// The native-engine paragraph: how many mode files are rewritten.
    native: Option<usize>,
}

/// The parts of the dialog for `plan`.
fn sections(plan: &MigrationPlan) -> Sections {
    Sections {
        loose: plan.moved_fields > 0
            || !plan.modes.is_empty()
            || plan.top_mode.is_some()
            || !plan.profiles.is_empty(),
        native: (!plan.rewritten.is_empty()).then_some(plan.rewritten.len()),
    }
}

/// The display names of the mode files the plan rewrites, in plan order
/// (the id when the mode is not in `modes`).
fn rewritten_names(plan: &MigrationPlan, modes: &ModeStore) -> Vec<String> {
    plan.rewritten
        .iter()
        .map(|(id, _)| modes.get(id).map_or_else(|| id.clone(), |m| m.name.clone()))
        .collect()
}

/// One mode of the plan and who names it.
#[derive(Debug, Clone, PartialEq)]
struct Row {
    /// Display name.
    name: String,
    /// Created by the migration (not an existing mode reused).
    new: bool,
    /// «Predeterminado» and the profiles, quoted.
    users: Vec<String>,
}

/// The modes the plan names, in order: the top level's first (its user
/// shown as `default_profile`), then the profiles' in file order.
fn rows(plan: &MigrationPlan, modes: &ModeStore, default_profile: &str) -> Vec<Row> {
    let mut rows: Vec<(String, Row)> = Vec::new();
    let named = plan
        .top_mode
        .iter()
        .map(|id| (id, default_profile.to_owned()))
        .chain(
            plan.profiles
                .iter()
                .filter_map(|(name, id)| id.as_ref().map(|id| (id, format!("«{name}»")))),
        );
    for (id, user) in named {
        if let Some((_, row)) = rows.iter_mut().find(|(i, _)| i == id) {
            row.users.push(user);
            continue;
        }
        let new = plan.modes.iter().find(|(i, _)| i == id);
        let name = match new {
            Some((_, raw)) => raw.get("name").and_then(Value::as_str).unwrap_or(id).to_owned(),
            None => modes.get(id).map_or_else(|| id.clone(), |m| m.name.clone()),
        };
        rows.push((
            id.clone(),
            Row {
                name,
                new: new.is_some(),
                users: vec![user],
            },
        ));
    }
    rows.into_iter().map(|(_, row)| row).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lirtup_config::{Mapping, Mode};

    #[test]
    fn rows_group_users_by_mode() {
        let raw = |name: &str| {
            let mut m = Mapping::new();
            m.insert("name".into(), name.into());
            m
        };
        let plan = MigrationPlan {
            modes: vec![
                ("predeterminado".into(), raw("Predeterminado")),
                ("modo-de-a".into(), raw("Modo de A")),
            ],
            top_mode: Some("predeterminado".into()),
            profiles: vec![
                ("A".into(), Some("modo-de-a".into())),
                ("B".into(), Some("modo-de-a".into())),
                ("C".into(), None),
                ("D".into(), Some("anime".into())),
            ],
            moved_fields: 5,
            rewritten: Vec::new(),
        };
        let mut modes = ModeStore::default();
        let mut anime = Mode::from_mapping("anime", &Mapping::new()).0;
        anime.name = "Anime".into();
        modes.modes.insert("anime".into(), anime);
        let rows = rows(&plan, &modes, "Predeterminado");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].name, "Predeterminado");
        assert!(rows[0].new);
        assert_eq!(rows[1].users, ["«A»", "«B»"]);
        assert_eq!(
            rows[2],
            Row {
                name: "Anime".into(),
                new: false,
                users: vec!["«D»".into()]
            }
        );
    }

    #[test]
    fn sections_follow_the_plan() {
        let mut modes = ModeStore::default();
        let mut crt = Mode::from_mapping("crt", &Mapping::new()).0;
        crt.name = "CRT".into();
        modes.modes.insert("crt".into(), crt);
        // Only native-engine modes: the short dialog.
        let native = MigrationPlan {
            rewritten: vec![("crt".into(), Mapping::new()), ("other".into(), Mapping::new())],
            ..MigrationPlan::default()
        };
        assert_eq!(
            sections(&native),
            Sections {
                loose: false,
                native: Some(2)
            }
        );
        assert_eq!(rewritten_names(&native, &modes), ["CRT", "other"]);
        // 1.x fields only: as before.
        let loose = MigrationPlan {
            top_mode: Some("predeterminado".into()),
            moved_fields: 3,
            ..MigrationPlan::default()
        };
        assert_eq!(
            sections(&loose),
            Sections {
                loose: true,
                native: None
            }
        );
        // Both: the 1.x dialog plus the native paragraph.
        let both = MigrationPlan {
            rewritten: native.rewritten.clone(),
            ..loose.clone()
        };
        assert_eq!(
            sections(&both),
            Sections {
                loose: true,
                native: Some(2)
            }
        );
    }

    /// The seeds of the 1.x palettes in `lirtup_config::migrate` are the
    /// seeds Ajustes offers.
    #[test]
    fn palette_seeds_are_offered() {
        for (name, seed) in lirtup_config::migrate::PALETTE_SEEDS {
            assert!(
                crate::theme::PALETTES
                    .iter()
                    .any(|(_, s)| s.eq_ignore_ascii_case(seed)),
                "{name}: {seed}"
            );
        }
    }
}
