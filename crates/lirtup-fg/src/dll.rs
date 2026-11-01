//! Finding `lsfg-vk.dll` (layer) and `Lossless.dll` (native) in Steam
//! (`find_dll`, `_steam_libraries`: `libraryfolders.vdf`, also Flatpak Steam).
//!
//! The DLLs are looked up where Steam installed Lossless Scaling and never
//! copied or redistributed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Folder of Lossless Scaling under `steamapps/common`.
pub const STEAM_APP_DIR: &str = "Lossless Scaling";

/// Which DLL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DllKind {
    /// `lsfg-vk.dll`, for the layer.
    LsfgVk,
    /// `Lossless.dll` (Lossless Scaling), for the native engine.
    Lossless,
}

impl DllKind {
    /// File name.
    pub fn file_name(self) -> &'static str {
        match self {
            DllKind::LsfgVk => "lsfg-vk.dll",
            DllKind::Lossless => "Lossless.dll",
        }
    }
}

/// `$XDG_DATA_HOME` or `~/.local/share`.
pub(crate) fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(dirs::data_dir)
        .unwrap_or_else(|| PathBuf::from(".local/share"))
}

/// Where Steam keeps its root folder (native and Flatpak).
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = vec![data_home().join("Steam")];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".steam/root"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }
    roots
}

/// Steam library folders (native and Flatpak Steam), existing ones only.
pub fn steam_libraries() -> Vec<PathBuf> {
    libraries_of(&steam_roots())
}

/// The libraries of these Steam roots: each root, then the `path` entries
/// of its `steamapps/libraryfolders.vdf`; existing folders only, without
/// duplicates (symlinked roots are the same library).
pub(crate) fn libraries_of(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let mut libraries = vec![root.clone()];
        if let Ok(text) = std::fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
            libraries.extend(vdf_paths(&text).into_iter().map(PathBuf::from));
        }
        for lib in libraries {
            let Ok(key) = lib.canonicalize() else { continue };
            if key.is_dir() && seen.insert(key) {
                out.push(lib);
            }
        }
    }
    out
}

/// The values of the `"path"` keys of a `libraryfolders.vdf`.
pub(crate) fn vdf_paths(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut quoted = quoted_strings(line);
        if quoted.next().as_deref() == Some("path") {
            if let Some(path) = quoted.next() {
                out.push(path);
            }
        }
    }
    out
}

/// The `"..."` strings of a VDF line, with `\\` and `\"` unescaped.
fn quoted_strings(line: &str) -> impl Iterator<Item = String> + '_ {
    let mut chars = line.chars();
    std::iter::from_fn(move || {
        chars.by_ref().find(|&c| c == '"')?;
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '"' => return Some(s),
                '\\' => s.extend(chars.next()),
                c => s.push(c),
            }
        }
        None
    })
}

/// Find the DLL. `hint` is `frame_gen_dll`: a folder, or a file inside it;
/// when it does not hold the DLL, Steam is searched.
///
/// lsfg-vk always loads `lsfg-vk.dll` from the folder, so for the layer only
/// the folder of the hint matters; the native engine takes a file hint as it
/// is, whatever its name.
pub fn find_dll(kind: DllKind, hint: Option<&Path>) -> Option<PathBuf> {
    if let Some(found) = hint.and_then(|h| from_hint(kind, h)) {
        return Some(found);
    }
    if let Some(hint) = hint {
        tracing::info!(
            "{} is not in {}, searching Steam",
            kind.file_name(),
            hint.display()
        );
    }
    find_in_libraries(kind, &steam_libraries())
}

fn from_hint(kind: DllKind, hint: &Path) -> Option<PathBuf> {
    let path = lirtup_config::paths::expand_tilde(hint);
    if kind == DllKind::Lossless && path.is_file() {
        return Some(path);
    }
    let folder = if path.is_dir() {
        path.as_path()
    } else {
        path.parent()?
    };
    let dll = folder.join(kind.file_name());
    dll.is_file().then_some(dll)
}

pub(crate) fn find_in_libraries(kind: DllKind, libraries: &[PathBuf]) -> Option<PathBuf> {
    libraries
        .iter()
        .map(|lib| {
            lib.join("steamapps/common")
                .join(STEAM_APP_DIR)
                .join(kind.file_name())
        })
        .find(|dll| dll.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    const VDF: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/user/.local/share/Steam"
		"label"		""
		"apps"
		{
			"993090"		"1234"
		}
	}
	"1"
	{
		"path"		"/mnt/games/Steam Library"
	}
}
"#;

    #[test]
    fn parses_libraryfolders() {
        assert_eq!(
            vdf_paths(VDF),
            ["/home/user/.local/share/Steam", "/mnt/games/Steam Library"]
        );
        assert_eq!(vdf_paths(r#""path" "C:\\Games\\Steam""#), [r"C:\Games\Steam"]);
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"MZ").unwrap();
    }

    #[test]
    fn finds_the_dll_in_any_library() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Steam");
        let extra = tmp.path().join("Library 2");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        std::fs::write(
            root.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"path\" \"{}\"\n\"path\" \"{}\"\n\"path\" \"/nonexistent\"\n",
                root.display(),
                extra.display()
            ),
        )
        .unwrap();
        let dll = extra.join("steamapps/common/Lossless Scaling/Lossless.dll");
        touch(&dll);

        let libs = libraries_of(&[root.clone(), root.clone()]);
        assert_eq!(libs, [root.clone(), extra.clone()]);
        assert_eq!(find_in_libraries(DllKind::Lossless, &libs), Some(dll));
        assert_eq!(find_in_libraries(DllKind::LsfgVk, &libs), None);
    }

    #[test]
    fn hint_folder_or_file() {
        let tmp = tempfile::tempdir().unwrap();
        let layer = tmp.path().join("lsfg-vk.dll");
        touch(&layer);
        let renamed = tmp.path().join("LS-3.1.dll");
        touch(&renamed);

        assert_eq!(from_hint(DllKind::LsfgVk, tmp.path()), Some(layer.clone()));
        // The layer only looks at the folder.
        assert_eq!(from_hint(DllKind::LsfgVk, &renamed), Some(layer));
        // The native engine takes the file as it is.
        assert_eq!(from_hint(DllKind::Lossless, &renamed), Some(renamed));
        assert_eq!(from_hint(DllKind::Lossless, tmp.path()), None);
    }
}
