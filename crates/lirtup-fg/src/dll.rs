//! Finding `lsfg-vk.dll` (layer) and `Lossless.dll` (native) in Steam
//! (`find_dll`, `_steam_libraries`: `libraryfolders.vdf`, also Flatpak Steam).

use std::path::{Path, PathBuf};

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

/// Steam library folders (native and Flatpak Steam), existing ones only.
pub fn steam_libraries() -> Vec<PathBuf> {
    todo!("front C")
}

/// Find the DLL. `hint` is `frame_gen_dll`: a folder, or a file inside it;
/// when it does not hold the DLL, Steam is searched.
pub fn find_dll(kind: DllKind, hint: Option<&Path>) -> Option<PathBuf> {
    todo!("front C")
}
