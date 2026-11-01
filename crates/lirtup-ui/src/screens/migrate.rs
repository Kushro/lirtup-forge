//! «Pasar a modos de escalado» (round 4, front M): the dialog the GUI shows
//! after loading a 1.x configuration with loose mode fields. It says how
//! many modes it creates and which profiles name them, that a backup goes
//! to `config.yaml.1x.bak`, and offers «Migrar» (`Action::Migrate`) or
//! «Ahora no» (`Action::SkipMigration`).

use dioxus::prelude::*;
use lirtup_config::migrate::MigrationPlan;

/// The dialog (`Dialog::Migrate`).
#[component]
pub fn MigrateDialog(plan: MigrationPlan) -> Element {
    todo!()
}
