//! Helpers shared by the YAML files: atomic writes, tolerant deserialization
//! and number cleanup.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::{ConfigError, ConfigWarning, Mapping, Value};

/// Read and parse a YAML file into a top-level mapping. `Ok(None)` when the
/// file does not exist; an empty file is an empty mapping.
pub(crate) fn read_mapping(path: &Path) -> Result<Option<Mapping>, ConfigError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ConfigError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    parse_mapping(&text, path).map(Some)
}

/// Parse YAML text into a top-level mapping (anchors and `<<` merge keys
/// applied, like PyYAML's `safe_load`).
pub(crate) fn parse_mapping(text: &str, path: &Path) -> Result<Mapping, ConfigError> {
    let yaml_err = |message: String| ConfigError::Yaml {
        path: path.to_path_buf(),
        message,
    };
    let mut value: Value = serde_yaml_ng::from_str(text).map_err(|e| yaml_err(e.to_string()))?;
    value.apply_merge().map_err(|e| yaml_err(e.to_string()))?;
    match value {
        Value::Null => Ok(Mapping::new()),
        Value::Mapping(map) => Ok(map),
        other => Err(yaml_err(format!(
            "expected a mapping at the top, found {}",
            type_name(&other)
        ))),
    }
}

/// Write `contents` to `path` through a temporary file in the same folder and
/// a `rename`, so a crash never leaves a half-written file. Creates the
/// folder.
pub(crate) fn write_atomic(path: &Path, contents: &str) -> Result<(), ConfigError> {
    let io_err = |source: std::io::Error| ConfigError::Io {
        path: path.to_path_buf(),
        source,
    };
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir).map_err(io_err)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp: PathBuf = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let result = (|| {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(io_err)
}

/// Serialize a mapping as a YAML document.
pub(crate) fn to_yaml(map: &Mapping, path: &Path) -> Result<String, ConfigError> {
    serde_yaml_ng::to_string(map).map_err(|e| ConfigError::Yaml {
        path: path.to_path_buf(),
        message: e.to_string(),
    })
}

/// Deserialize `T` from `map`, dropping the keys whose value does not fit
/// (each one gives a [`ConfigWarning`] and keeps the default of `T`). A
/// `null` that does not fit is dropped silently: in 1.x `null` means "not
/// set".
///
/// `prefix` is put before the key in warnings (`"profiles.x.options."`).
pub(crate) fn tolerant<T: DeserializeOwned + Default>(
    map: &Mapping,
    prefix: &str,
) -> (T, Vec<ConfigWarning>) {
    if let Ok(value) = serde_yaml_ng::from_value::<T>(Value::Mapping(map.clone())) {
        return (value, Vec::new());
    }
    let mut warnings = Vec::new();
    let mut good = Mapping::new();
    for (key, value) in map {
        let mut one = Mapping::new();
        one.insert(key.clone(), value.clone());
        match serde_yaml_ng::from_value::<T>(Value::Mapping(one)) {
            Ok(_) => {
                good.insert(key.clone(), value.clone());
            }
            Err(_) if value.is_null() => {}
            Err(e) => warnings.push(ConfigWarning {
                key: format!("{prefix}{}", key_string(key)),
                message: format!("invalid value {}, using the default ({e})", display(value)),
            }),
        }
    }
    let value = match serde_yaml_ng::from_value::<T>(Value::Mapping(good)) {
        Ok(value) => value,
        Err(e) => {
            // Keys that load alone but not together: give up on the layer.
            warnings.push(ConfigWarning {
                key: prefix.trim_end_matches('.').to_owned(),
                message: format!("could not be read, using the defaults ({e})"),
            });
            T::default()
        }
    };
    (value, warnings)
}

/// A mapping key as text (`720:` is a number in YAML).
pub(crate) fn key_string(key: &Value) -> String {
    match key {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        other => display(other),
    }
}

/// A value on one line, for messages.
pub(crate) fn display(value: &Value) -> String {
    match value {
        Value::String(s) => format!("{s:?}"),
        other => serde_yaml_ng::to_string(other)
            .map(|s| s.trim_end().replace('\n', " "))
            .unwrap_or_else(|_| format!("{other:?}")),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Sequence(_) => "a list",
        Value::Mapping(_) => "a mapping",
        Value::Tagged(_) => "a tagged value",
    }
}

/// `serde_yaml_ng::to_value` widens `f32` to `f64` (`0.03` becomes
/// `0.029999999329447746`). Floats that are exactly an `f32` are rewritten
/// with the shortest `f32` spelling, recursively.
pub(crate) fn clean_floats(value: &mut Value) {
    match value {
        Value::Number(n) if n.is_f64() => {
            if let Some(x) = n.as_f64() {
                let narrow = x as f32;
                if x.is_finite()
                    && narrow as f64 == x
                    && let Ok(short) = narrow.to_string().parse::<f64>()
                {
                    *value = Value::Number(short.into());
                }
            }
        }
        Value::Sequence(seq) => seq.iter_mut().for_each(clean_floats),
        Value::Mapping(map) => map.iter_mut().for_each(|(_, v)| clean_floats(v)),
        Value::Tagged(tagged) => clean_floats(&mut tagged.value),
        _ => {}
    }
}

/// `serde_yaml_ng::to_value`, as a mapping, floats cleaned.
pub(crate) fn to_mapping<T: serde::Serialize>(value: &T) -> Mapping {
    match serde_yaml_ng::to_value(value) {
        Ok(mut v) => {
            clean_floats(&mut v);
            match v {
                Value::Mapping(map) => map,
                _ => Mapping::new(),
            }
        }
        Err(e) => {
            tracing::error!("cannot serialize the configuration: {e}");
            Mapping::new()
        }
    }
}
