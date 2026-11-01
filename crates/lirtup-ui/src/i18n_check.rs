//! The test that both languages are complete (round 4, front M): every key
//! the code names exists in `es` and `en`, both define the same keys, and
//! none is defined twice (what `scripts/check-ftl-keys.py` checks today, as
//! a `cargo test`). Uses `i18n::has_key`.
//!
//! The files are read from disk, so a new `.ftl` or `.rs` needs no list
//! here. On top of the script, it checks that the compiled-in loader has
//! every message (a Fluent syntax error drops messages without failing the
//! build) and that both languages use the same variables in each message
//! (an English text that forgets `{ $name }` shows less than the Spanish).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::i18n::{Lang, has_key};

const LANGS: [(&str, Lang); 2] = [("es", Lang::Es), ("en", Lang::En)];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// A message: the file it is in and its text (continuation lines included).
struct Message {
    file: String,
    text: String,
}

/// `<root>/i18n/<lang>/*.ftl`: key → message, and the keys defined twice.
fn defined(lang: &str) -> (BTreeMap<String, Message>, Vec<String>) {
    let dir = root().join("i18n").join(lang);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ftl"))
        .collect();
    files.sort();
    let mut keys = BTreeMap::new();
    let mut twice = Vec::new();
    for path in files {
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let mut current: Option<String> = None;
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            if let Some(key) = message_key(line) {
                let text = line.split_once('=').map_or("", |(_, t)| t).to_owned();
                if let Some(old) = keys.insert(
                    key.clone(),
                    Message {
                        file: file.clone(),
                        text,
                    },
                ) {
                    twice.push(format!("{lang}/{file}: {key} (also in {})", old.file));
                }
                current = Some(key);
            } else if line.starts_with([' ', '\t']) || line.trim_start().starts_with(['[', '*', '}']) {
                if let Some(m) = current.as_ref().and_then(|k| keys.get_mut(k)) {
                    m.text.push('\n');
                    m.text.push_str(line);
                }
            } else {
                current = None;
            }
        }
    }
    (keys, twice)
}

/// `key` of a `key = ...` line (lowercase, digits and `-`, starting with a
/// letter), as `MSG` of the script.
fn message_key(line: &str) -> Option<String> {
    let (key, _) = line.split_once('=')?;
    let key = key.trim_end();
    looks_like_key(key, false).then(|| key.to_owned())
}

/// `[a-z][a-z0-9-]*`; with `dashed`, at least one `-` between non-empty
/// parts (`LITERAL` of the script: what a key in the code looks like).
fn looks_like_key(s: &str, dashed: bool) -> bool {
    let mut chars = s.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_lowercase());
    let rest_ok = s
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !(first_ok && rest_ok) {
        return false;
    }
    !dashed || (s.contains('-') && s.split('-').all(|part| !part.is_empty()))
}

/// The `$variables` of a message.
fn variables(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = text;
    while let Some(i) = rest.find('$') {
        rest = &rest[i + 1..];
        let name: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect();
        if !name.is_empty() {
            out.insert(name);
        }
    }
    out
}

/// String literals in `src/**/*.rs` that look like keys and start with the
/// first part of some defined key: key → the first file that names it.
fn named_in_code(prefixes: &BTreeSet<String>) -> BTreeMap<String, String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap().filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|x| x == "rs") {
                out.push(path);
            }
        }
    }
    let src = root().join("src");
    let mut files = Vec::new();
    walk(&src, &mut files);
    files.sort();
    let mut used = BTreeMap::new();
    for path in files {
        let text = std::fs::read_to_string(&path).unwrap();
        let rel = path.strip_prefix(&src).unwrap().display().to_string();
        // Between any two consecutive quotes, as the script's regex: a
        // `'"'` or `\"` elsewhere does not shift what is inside.
        let quotes: Vec<usize> = text.match_indices('"').map(|(i, _)| i).collect();
        for pair in quotes.windows(2) {
            let literal = &text[pair[0] + 1..pair[1]];
            let prefix = literal.split('-').next().unwrap_or_default();
            if looks_like_key(literal, true) && prefixes.contains(prefix) {
                used.entry(literal.to_owned()).or_insert_with(|| rel.clone());
            }
        }
    }
    used
}

#[test]
fn both_languages_are_complete() {
    let mut problems = Vec::new();
    let mut langs = Vec::new();
    for (name, lang) in LANGS {
        let (keys, twice) = defined(name);
        problems.extend(twice);
        langs.push((name, lang, keys));
    }
    let prefixes: BTreeSet<String> = langs
        .iter()
        .flat_map(|(_, _, keys)| keys.keys())
        .filter_map(|k| k.split('-').next())
        .map(str::to_owned)
        .collect();
    let used = named_in_code(&prefixes);

    for (name, lang, keys) in &langs {
        for (key, file) in &used {
            if !keys.contains_key(key) {
                problems.push(format!("{name}: missing {key} (used in {file})"));
            }
        }
        for (other, _, other_keys) in &langs {
            for (key, m) in other_keys.iter().filter(|(k, _)| !keys.contains_key(*k)) {
                problems.push(format!("{name}: missing {key} (defined in {other}/{})", m.file));
            }
        }
        for (key, m) in keys {
            if !has_key(*lang, key) {
                problems.push(format!(
                    "{name}/{}: {key} is not in the loader (a Fluent syntax error?)",
                    m.file
                ));
            }
        }
    }

    let (_, _, es) = &langs[0];
    let (_, _, en) = &langs[1];
    for (key, m) in es {
        if let Some(e) = en.get(key) {
            let (a, b) = (variables(&m.text), variables(&e.text));
            if a != b {
                problems.push(format!("{key}: variables {a:?} in es, {b:?} in en"));
            }
        }
    }

    assert!(
        problems.is_empty(),
        "{} keys in es, {} in en, {} named in the code:\n{}",
        es.len(),
        en.len(),
        used.len(),
        problems.join("\n")
    );
    assert!(
        used.len() > 500,
        "the scan of the code found only {} keys",
        used.len()
    );
}

#[test]
fn parsing_helpers() {
    assert_eq!(message_key("home-title = Inicio").as_deref(), Some("home-title"));
    assert_eq!(message_key("    [one] x = y"), None);
    assert_eq!(message_key("# a = b"), None);
    assert!(looks_like_key("migrate-title", true));
    assert!(!looks_like_key("migrate", true));
    assert!(!looks_like_key("a--b", true));
    assert!(!looks_like_key("Text-x", true));
    assert_eq!(
        variables("{ $count ->\n [one] { $name } x\n *[other] y }"),
        BTreeSet::from(["count".to_owned(), "name".to_owned()])
    );
}

/// The migration messages format in both languages with their arguments
/// (they nest selectors).
#[test]
fn migrate_messages_format() {
    use crate::i18n::{arg, tr_in};
    use std::borrow::Cow;
    use std::collections::HashMap;
    for (_, lang) in LANGS {
        for (fields, modes) in [(1, 0), (2, 1), (5, 3)] {
            let args: HashMap<Cow<'static, str>, _> = [
                (Cow::Borrowed("fields"), arg(fields)),
                (Cow::Borrowed("modes"), arg(modes)),
            ]
            .into_iter()
            .collect();
            let text = tr_in(lang, "migrate-body", Some(&args));
            assert!(
                text.contains(&fields.to_string()) && !text.contains('$'),
                "{text}"
            );
        }
    }
    let es = |fields: i64, modes: i64| {
        let args: HashMap<Cow<'static, str>, _> = [
            (Cow::Borrowed("fields"), arg(fields)),
            (Cow::Borrowed("modes"), arg(modes)),
        ]
        .into_iter()
        .collect();
        tr_in(Lang::Es, "migrate-body", Some(&args))
    };
    assert!(
        es(1, 1).ends_with("Se mueve 1 ajuste a 1 modo nuevo:"),
        "{}",
        es(1, 1)
    );
    assert!(
        es(4, 2).ends_with("Se mueven 4 ajustes a 2 modos nuevos:"),
        "{}",
        es(4, 2)
    );
}
