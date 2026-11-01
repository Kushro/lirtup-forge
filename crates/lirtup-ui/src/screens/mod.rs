//! The screens (6.1 of the analysis). `app.rs` picks one by
//! [`crate::route::Route`]; each front owns its folder (5.5).
//!
//! | Module | Front | Route or dialog |
//! | --- | --- | --- |
//! | [`home`] | B | `Route::Home` |
//! | [`about`] | B | `Route::About` |
//! | [`profile`] | C | `Route::Profile`, `Dialog::Unsaved`, `Dialog::DeleteProfile` |
//! | [`new_profile`] | C | `Dialog::NewProfile` |
//! | [`modes`] | D | `Route::Modes`, `Dialog::EffectPicker`, `Dialog::DeleteMode` |
//! | [`settings`] | D | `Route::Settings` |
//! | [`bench`] | D | `Dialog::Bench` |

pub mod about;
pub mod bench;
pub mod home;
pub mod modes;
pub mod new_profile;
pub mod profile;
pub mod settings;
