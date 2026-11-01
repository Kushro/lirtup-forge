//! Profiles and their `match:` rules (`config/profiles.py`).

use indexmap::IndexMap;
use serde::{Deserialize, Serialize, Serializer};

use crate::{ConfigError, Mapping, Value};

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
        self.options
            .get("mode")
            .and_then(Value::as_str)
            .filter(|m| !m.is_empty())
    }

    /// Whether the daemon skips it (`options.daemon_exclude`).
    pub fn daemon_excluded(&self) -> bool {
        self.options
            .get("daemon_exclude")
            .and_then(Value::as_bool)
            .unwrap_or(false)
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
        self.title.is_none()
            && self.title_contains.is_none()
            && self.title_regex.is_none()
            && self.title_startswith.is_none()
            && self.title_endswith.is_none()
            && self.width.is_none()
            && self.height.is_none()
            && self.extra.is_empty()
    }

    /// Whether a window with this title and size matches every rule.
    pub fn matches(&self, title: &str, width: u32, height: u32) -> bool {
        if self.is_empty() {
            return false;
        }
        if !self.extra.is_empty() {
            tracing::debug!("unknown match keys never match: {:?}", self.extra);
            return false;
        }
        let title = title.to_lowercase();
        let text = |rule: &Option<String>, test: &dyn Fn(&str) -> bool| {
            rule.as_ref().is_none_or(|v| test(&v.to_lowercase()))
        };
        text(&self.title, &|v| title == v)
            && text(&self.title_contains, &|v| title.contains(v))
            && text(&self.title_startswith, &|v| title.starts_with(v))
            && text(&self.title_endswith, &|v| title.ends_with(v))
            && self.title_regex.as_ref().is_none_or(|re| {
                match regex::RegexBuilder::new(re).case_insensitive(true).build() {
                    Ok(re) => re.is_match(&title),
                    Err(e) => {
                        tracing::warn!("invalid regex in match: {re:?}: {e}");
                        false
                    }
                }
            })
            && self.width.as_ref().is_none_or(|i| i.contains(width))
            && self.height.as_ref().is_none_or(|i| i.contains(height))
    }

    /// Problems that make a rule never match (bad regex or interval, unknown
    /// keys), as warnings for the load.
    pub fn check(&self, prefix: &str) -> Vec<crate::ConfigWarning> {
        let mut warnings = Vec::new();
        let mut warn = |key: &str, message: String| {
            warnings.push(crate::ConfigWarning {
                key: format!("{prefix}{key}"),
                message,
            })
        };
        if let Some(re) = &self.title_regex
            && let Err(e) = regex::Regex::new(re)
        {
            warn(
                "title_regex",
                format!("invalid regex, the profile never matches ({e})"),
            );
        }
        for (key, interval) in [("width", &self.width), ("height", &self.height)] {
            if let Some(Err(e)) = interval.as_ref().map(Interval::parse) {
                warn(key, format!("{e}, the profile never matches"));
            }
        }
        for key in self.extra.keys() {
            warn(
                &crate::util::key_string(key),
                "unknown match rule, the profile never matches".into(),
            );
        }
        warnings
    }
}

/// A size interval: `720`, `"<480"`, `">=1280"`, `"480<"`, `"720-1080"`,
/// `"720..1080"`, `"720,1080"` (`parse_interval` in `config/parsers.py`).
/// Kept as written for saving.
#[derive(Debug, Clone, PartialEq, Deserialize)]
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
        let text = match self {
            Interval::Number(n) => return Ok(IntervalOp::Eq(*n)),
            Interval::Text(t) => t,
        };
        let invalid = || ConfigError::Invalid {
            key: "match".into(),
            message: format!("unrecognised interval format: {text:?}"),
        };
        let s: String = text.chars().filter(|c| !c.is_whitespace()).collect();
        if s.is_empty() {
            return Err(invalid());
        }
        let num = |t: &str| -> Option<f64> {
            let ok = !t.is_empty()
                && t.trim_start_matches(['+', '-'])
                    .chars()
                    .all(|c| c.is_ascii_digit() || c == '.')
                && t.chars().any(|c| c.is_ascii_digit());
            ok.then(|| t.parse::<f64>().ok()).flatten()
        };
        if let Some(n) = num(&s) {
            return Ok(IntervalOp::Eq(n));
        }
        // Ranges: `720..1080`, `720-1080`, `720,1080` (the separator is
        // searched after the first character so a leading sign stays).
        let range = s.find("..").map(|i| (i, 2)).or_else(|| {
            s.char_indices()
                .skip(1)
                .find(|(_, c)| matches!(c, '-' | ','))
                .map(|(i, _)| (i, 1))
        });
        if let Some((i, len)) = range
            && let (Some(a), Some(b)) = (num(&s[..i]), num(&s[i + len..]))
        {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            return Ok(IntervalOp::Range(lo, hi));
        }
        let ops = ["<=", ">=", "<", ">"];
        for op in ops {
            if let Some(n) = s.strip_prefix(op).and_then(num) {
                return Ok(match op {
                    "<" => IntervalOp::Lt(n),
                    "<=" => IntervalOp::Le(n),
                    ">" => IntervalOp::Gt(n),
                    _ => IntervalOp::Ge(n),
                });
            }
        }
        // Reversed: `480<` means 480 < n.
        for op in ops {
            if let Some(n) = s.strip_suffix(op).and_then(num) {
                return Ok(match op {
                    "<" => IntervalOp::Gt(n),
                    "<=" => IntervalOp::Ge(n),
                    ">" => IntervalOp::Lt(n),
                    _ => IntervalOp::Le(n),
                });
            }
        }
        Err(invalid())
    }

    /// Whether `n` is inside. Malformed intervals never match.
    pub fn contains(&self, n: u32) -> bool {
        let n = f64::from(n);
        match self.parse() {
            Ok(IntervalOp::Eq(v)) => n == v,
            Ok(IntervalOp::Lt(v)) => n < v,
            Ok(IntervalOp::Le(v)) => n <= v,
            Ok(IntervalOp::Gt(v)) => n > v,
            Ok(IntervalOp::Ge(v)) => n >= v,
            Ok(IntervalOp::Range(lo, hi)) => lo <= n && n <= hi,
            Err(e) => {
                tracing::warn!("{e}");
                false
            }
        }
    }
}

impl Serialize for Interval {
    /// Numbers that are whole are written as integers (`720`, not `720.0`),
    /// so a file reads back unchanged.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Interval::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => {
                serializer.serialize_i64(*n as i64)
            }
            Interval::Number(n) => serializer.serialize_f64(*n),
            Interval::Text(t) => serializer.serialize_str(t),
        }
    }
}

/// The first profile, in file order, whose non-empty `match` fits the window.
pub fn find_matching_profile<'a>(
    profiles: &'a IndexMap<String, Profile>,
    title: &str,
    width: u32,
    height: u32,
) -> Option<(&'a str, &'a Profile)> {
    profiles
        .iter()
        .find(|(_, p)| {
            p.matcher
                .as_ref()
                .is_some_and(|m| m.matches(title, width, height))
        })
        .map(|(name, p)| (name.as_str(), p))
}

impl Profile {
    /// Read one profile from its YAML value, tolerating a bad `match` (it is
    /// kept as written, never matches, and gives a warning) and a missing or
    /// malformed `options`.
    pub fn from_value(name: &str, value: &Value) -> (Profile, Vec<crate::ConfigWarning>) {
        let prefix = format!("profiles.{name}.");
        let mut warnings = Vec::new();
        let map = match value {
            Value::Mapping(map) => map.clone(),
            Value::Null => Mapping::new(),
            other => {
                warnings.push(crate::ConfigWarning {
                    key: format!("profiles.{name}"),
                    message: format!("not a mapping ({}), ignored", crate::util::display(other)),
                });
                return (Profile::default(), warnings);
            }
        };
        let mut profile = Profile::default();
        for (key, value) in map {
            match key.as_str() {
                Some("match") if !value.is_null() => {
                    match serde_yaml_ng::from_value::<Match>(value.clone()) {
                        Ok(m) => {
                            warnings.extend(m.check(&format!("{prefix}match.")));
                            profile.matcher = Some(m);
                        }
                        Err(e) => {
                            warnings.push(crate::ConfigWarning {
                                key: format!("{prefix}match"),
                                message: format!("invalid, the profile never matches ({e})"),
                            });
                            profile.extra.insert(key, value);
                        }
                    }
                }
                Some("match") => {}
                Some("options") => match value {
                    Value::Mapping(options) => profile.options = options,
                    Value::Null => {}
                    other => {
                        warnings.push(crate::ConfigWarning {
                            key: format!("{prefix}options"),
                            message: "not a mapping, kept but ignored".into(),
                        });
                        profile.extra.insert(key, other);
                    }
                },
                _ => {
                    profile.extra.insert(key, value);
                }
            }
        }
        (profile, warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn interval(s: &str) -> Interval {
        Interval::Text(s.into())
    }

    #[test]
    fn interval_forms() {
        assert_eq!(interval("720").parse().unwrap(), IntervalOp::Eq(720.0));
        assert_eq!(Interval::Number(720.0).parse().unwrap(), IntervalOp::Eq(720.0));
        assert_eq!(interval("<480").parse().unwrap(), IntervalOp::Lt(480.0));
        assert_eq!(interval(" >= 1280 ").parse().unwrap(), IntervalOp::Ge(1280.0));
        assert_eq!(interval("480<").parse().unwrap(), IntervalOp::Gt(480.0));
        assert_eq!(interval("480>=").parse().unwrap(), IntervalOp::Le(480.0));
        assert_eq!(
            interval("720-1080").parse().unwrap(),
            IntervalOp::Range(720.0, 1080.0)
        );
        assert_eq!(
            interval("720..1080").parse().unwrap(),
            IntervalOp::Range(720.0, 1080.0)
        );
        assert_eq!(
            interval("1080,720").parse().unwrap(),
            IntervalOp::Range(720.0, 1080.0)
        );
        // 1.x strips the spaces first, so its "malformed" docstring example
        // `480 < =` is really `480<=` and parses (same here).
        assert_eq!(interval("480 < =").parse().unwrap(), IntervalOp::Ge(480.0));
        assert!(interval("<=>480").parse().is_err());
        assert!(interval("abc").parse().is_err());
        assert!(interval("").parse().is_err());
    }

    #[test]
    fn interval_contains() {
        assert!(interval(">1280").contains(1920));
        assert!(!interval(">1280").contains(1280));
        assert!(interval("720-1080").contains(720));
        assert!(!interval("nope").contains(720));
    }

    #[test]
    fn interval_saves_integers() {
        let yaml = serde_yaml_ng::to_string(&Interval::Number(720.0)).unwrap();
        assert_eq!(yaml.trim(), "720");
    }

    #[test]
    fn match_rules_are_anded() {
        let m = Match {
            title_contains: Some("Firefox".into()),
            width: Some(interval(">1280")),
            ..Match::default()
        };
        assert!(m.matches("Mozilla FIREFOX", 1920, 1080));
        assert!(!m.matches("Mozilla Firefox", 1280, 1080));
        assert!(!m.matches("Chromium", 1920, 1080));
        assert!(!Match::default().matches("x", 1, 1));
    }

    #[test]
    fn match_title_forms() {
        let exact = Match {
            title: Some("VLC media player".into()),
            ..Match::default()
        };
        assert!(exact.matches("vlc media player", 1, 1));
        assert!(!exact.matches("vlc media player 2", 1, 1));
        let re = Match {
            title_regex: Some("(PCSX2|Dolphin).*".into()),
            ..Match::default()
        };
        assert!(re.matches("dolphin 5.0", 1, 1));
        let bad = Match {
            title_regex: Some("(".into()),
            ..Match::default()
        };
        assert!(!bad.matches("(", 1, 1));
    }

    #[test]
    fn unknown_rule_never_matches() {
        let m: Match = serde_yaml_ng::from_str(
            "title_contains: a
wm_class: b
",
        )
        .unwrap();
        assert!(!m.matches("a", 1, 1));
        assert_eq!(m.check("").len(), 1);
    }

    #[test]
    fn first_matching_profile_in_file_order() {
        let yaml = "
a:
  options: {model: 4x24}
b:
  match: {title_contains: game}
  options: {model: fast}
c:
  match: {title_contains: game}
";
        let raw: IndexMap<String, Value> = serde_yaml_ng::from_str(yaml).unwrap();
        let profiles: IndexMap<String, Profile> = raw
            .iter()
            .map(|(k, v)| (k.clone(), Profile::from_value(k, v).0))
            .collect();
        assert_eq!(
            find_matching_profile(&profiles, "My Game", 800, 600).unwrap().0,
            "b"
        );
        assert!(find_matching_profile(&profiles, "other", 800, 600).is_none());
    }

    #[test]
    fn profile_mode_and_daemon_exclude() {
        let v: Value = serde_yaml_ng::from_str("options: {mode: anime, daemon_exclude: true}").unwrap();
        let (p, w) = Profile::from_value("x", &v);
        assert!(w.is_empty());
        assert_eq!(p.mode(), Some("anime"));
        assert!(p.daemon_excluded());
    }

    #[test]
    fn bad_match_is_kept() {
        let v: Value = serde_yaml_ng::from_str(
            "match: [1, 2]
options: {model: fast}",
        )
        .unwrap();
        let (p, w) = Profile::from_value("x", &v);
        assert_eq!(w.len(), 1);
        assert!(p.matcher.is_none());
        assert!(p.extra.contains_key("match"));
    }
}
