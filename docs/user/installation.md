# Installation

LirtUp Forge 2.0 has two programs:

- `lirtup-gui`, the app;
- `lirtup`, the command line and the scaler itself. The GUI starts `lirtup serve`
  in the background the first time it needs it.

It runs on X11, or on Wayland through XWayland, with any Vulkan GPU.

!!! note "Coming from 1.x"

    Version 2.0 replaces the Python 1.x and uses the same package name
    (`linux-rt-upscaler`) and the same configuration folder
    (`~/.config/linux-rt-upscaler`). Uninstall the 1.x first, then read
    [Migrating from 1.x](migration.md).

    ```sh
    pipx uninstall linux-rt-upscaler
    ```

## Arch Linux and derivatives (AUR)

The PKGBUILD is in [`packaging/aur`](https://github.com/Kushro/lirtup-forge/tree/main/packaging/aur).
Build and install it with `makepkg`:

```sh
git clone https://github.com/Kushro/lirtup-forge.git
cd lirtup-forge/packaging/aur
makepkg -si
```

Once the package is on the AUR, an AUR helper does the same:

```sh
yay -S linux-rt-upscaler
```

The build needs `cargo`, `dioxus-cli` (the same version as the `dioxus` crate,
0.7.10), `npm` and `vulkan-headers`; `makepkg -s` installs them. It installs:

| Path | What |
| --- | --- |
| `/usr/bin/lirtup`, `/usr/bin/lirtup-gui` | Launchers |
| `/usr/lib/lirtup/` | The two binaries and the GUI's assets |
| `/usr/share/lirtup/effects/` | The 117 Magpie effects |
| `/usr/share/applications/`, `/usr/share/icons/hicolor/` | The menu entry and its icon |

Optional packages:

- `directx-shader-compiler` (`dxc`): compiles the MagpieFX effects. Recommended.
- `glslang`: compiles them without `dxc`, and the three `Anime4K_Denoise_Bilateral_*`
  effects, which `dxc` rejects.
- `lsfg-vk`: frame generation through its Vulkan layer (see
  [Frame generation](modes.md#frame-generation)).

## AppImage (any distribution)

Download `LirtUp_Forge-<version>-x86_64.AppImage`, make it executable and run it:

```sh
chmod +x LirtUp_Forge-*-x86_64.AppImage
./LirtUp_Forge-*-x86_64.AppImage
```

The AppImage starts the GUI. For the command line, pass `cli` first, or make a
symlink named `lirtup`:

```sh
./LirtUp_Forge-*-x86_64.AppImage cli list windows
ln -s "$PWD/LirtUp_Forge-2.0.0-x86_64.AppImage" ~/.local/bin/lirtup
lirtup run -s
```

The AppImage does not include WebKitGTK, GTK or the Vulkan driver: they come
from your system. Install them if they are missing:

=== "Debian / Ubuntu / Linux Mint"

    ```sh
    sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1 \
        libvulkan1 mesa-vulkan-drivers libfuse2
    ```

=== "Fedora"

    ```sh
    sudo dnf install webkit2gtk4.1 gtk3 libayatana-appindicator-gtk3 \
        vulkan-loader mesa-vulkan-drivers fuse-libs
    ```

=== "Arch Linux / CachyOS"

    ```sh
    sudo pacman -S webkit2gtk-4.1 gtk3 libayatana-appindicator vulkan-icd-loader fuse2
    ```

Install `dxc` or `glslang` as well if you use the MagpieFX effects.

!!! note "Building on an older system"

    The AppImage is built on Arch Linux, so it needs a glibc as new as Arch's.
    On an older distribution, build it yourself with
    `scripts/build-release-linux.sh` (see below).

## From source

You need:

- Rust 1.99 or newer (`rustup` picks the version in `rust-toolchain.toml`);
- `dx`, the Dioxus CLI, at the version of the `dioxus` crate in `Cargo.lock`
  (0.7.10): `cargo install dioxus-cli --version 0.7.10 --locked`;
- `npm`, for the Tailwind of `crates/lirtup-ui/package.json`;
- a C++20 compiler and the Vulkan headers, for the lsfg core;
- the development files of WebKitGTK 4.1, GTK 3 and libxdo.

=== "Debian / Ubuntu / Linux Mint"

    ```sh
    sudo apt install build-essential npm libvulkan-dev libwebkit2gtk-4.1-dev \
        libgtk-3-dev libxdo-dev libayatana-appindicator3-dev
    ```

=== "Fedora"

    ```sh
    sudo dnf install gcc-c++ npm vulkan-headers webkit2gtk4.1-devel gtk3-devel \
        libxdo-devel libayatana-appindicator-gtk3-devel
    ```

=== "Arch Linux / CachyOS"

    ```sh
    sudo pacman -S base-devel rustup npm vulkan-headers webkit2gtk-4.1 gtk3 \
        xdotool libayatana-appindicator dioxus-cli
    ```

Then build the release, which leaves a tarball with the installed tree and the
AppImage in `target/release-linux/`:

```sh
git clone https://github.com/Kushro/lirtup-forge.git
cd lirtup-forge
scripts/build-release-linux.sh --fetch-tools
```

`--no-appimage` skips the AppImage (it needs `appimagetool` and `patchelf`).
The tarball unpacks into `/usr` or `/usr/local` as is:

```sh
sudo tar -C /usr/local --strip-components=1 -xzf target/release-linux/lirtup-forge-*-linux-x86_64.tar.gz
```

## What to read next

- [Graphical interface](gui.md): the app.
- [Scaling modes](modes.md): models, effects and frame generation.
- [Controls](controls.md): hotkeys and the toolbar over the game.
- [Tray and daemon](tray-daemon.md).
- [Command line](cli.md) and [configuration](configuration.md).
- [Troubleshooting](troubleshooting.md), including the known limitations.
