//! Profiles and their `match:` rules (`config/profiles.py`).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::{ConfigError, Mapping};

/// One entry of `profiles:` in `config.yaml`.
///
/// `options` stays a raw mapping: profiles store only what differs, and the
/// GUI needs to know which keys a profile sets (inherited vs. overridden). A
/// profile names its scaling mode with `options.mode`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Which windows this profile applies to; `None` = only by name (`-p`).
    #[serde(rename = "match", default, skip_serializing_if = "Option::is_none")]
    pub matcher: Option<Match>,
    /// Option overrides, as written.
    #[serde(default)]
    pub options: Mapping,
    /// Other keys (`icon`, ...), kept for saving.
    #[serde(flatten)]
    pub extra: Mapping,
}

impl Profile {
    /// The mode this profile names (`options.mode`), if any.
    pub fn mode(&self) -> Option<&str> {
        todo!("front C")
    }

    /// Whether the daemon skips it (`options.daemon_exclude`).
    pub fn daemon_excluded(&self) -> bool {
        todo!("front C")
    }
}

/// Window match rules. Every rule that is set must match (AND), as in
/// `find_matching_profile(require_all=True)`. Titles compare
/// case-insensitively.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Match {
    /// Exact title.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Title contains.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    /// Title regex (searched, not anchored).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_regex: Option<String>,
    /// Title starts with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_startswith: Option<String>,
    /// Title ends with.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_endswith: Option<String>,
    /// Window width interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<Interval>,
    /// Window height interval.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<Interval>,
    /// Unknown rules: kept, and never match (as 1.x).
    #[serde(flatten)]
    pub extra: Mapping,
}

impl Match {
    /// No rule set.
    pub fn is_empty(&self) -> bool {
        todo!("front C")
    }

    /// Whether a window with this title and size matches every rule.
    pub fn matches(&self, title: &str, width: u32, height: u32) -> bool {
        todo!("front C: port _criterion_matches")
    }
}

/// A size interval: `720`, `"<480"`, `">=1280"`, `"480<"`, `"720-1080"`,
/// `"720..1080"`, `"720,1080"` (`parse_interval` in `config/parsers.py`).
/// Kept as written for saving.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Interval {
    /// A bare number: equality.
    Number(f64),
    /// An interval string.
    Text(String),
}

/// Parsed [`Interval`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IntervalOp {
    /// `n == v`.
    Eq(f64),
    /// `n < v`.
    Lt(f64),
    /// `n <= v`.
    Le(f64),
    /// `n > v`.
    Gt(f64),
    /// `n >= v`.
    Ge(f64),
    /// `lo <= n <= hi`.
    Range(f64, f64),
}

impl Interval {
    /// Parse; malformed strings are an error (the match then fails).
    pub fn parse(&self) -> Result<IntervalOp, ConfigError> {
        todo!("front C")
    }

    /// Whether `n` is inside. Malformed intervals never match.
    pub fn contains(&self, n: u32) -> bool {
        todo!("front C")
    }
}

/// The first profile, in file order, whose non-empty `match` fits the window.
pub fn find_matching_profile<'a>(
    profiles: &'a IndexMap<String, Profile>,
    title: &str,
    width: u32,
    height: u32,
) -> Option<(&'a str, &'a Profile)> {
    todo!("front C")
}
