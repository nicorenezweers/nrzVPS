#![allow(ambiguous_glob_reexports)]
//! Lucide-Icons für Dioxus (aus nrzUI `crates/icons`, Leptos-Teil entfernt).

pub mod common;

#[cfg(feature = "dioxus")]
pub mod dioxus;
#[cfg(feature = "dioxus")]
pub use dioxus::*;

#[cfg(feature = "dioxus_animated")]
pub mod dioxus_animated;
#[cfg(feature = "dioxus_animated")]
pub use dioxus_animated::*;
