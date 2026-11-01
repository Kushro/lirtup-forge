# Troubleshooting

## Known limitations

### Vulkan games are captured black under KWin with XWayland

On a Plasma Wayland session, windows that present with Vulkan through
XWayland come out black: games running on DXVK or VKD3D-Proton (most Windows
games under Proton and Wine), and native Vulkan games that use X11. The 1.x
behaves the same way: with `vkcube --wsi xcb`, both versions capture
0 non-black pixels out of 250 000. KWin does not give XWayland clients the
contents of those windows, so it is not something the scaler can work around.

Windows that present with OpenGL or with plain X11 drawing (visual novels,
emulators, video players, most older games) are not affected. Things to try,
in order:

- a Plasma **X11** session;
- making the game present with OpenGL: WineD3D instead of DXVK
  (`PROTON_USE_WINED3D=1 %command%` in Steam, or a Wine prefix without DXVK),
  at a performance cost;
- another compositor; this was only checked with KWin.

### Native Wayland windows

LirtUp Forge captures X11 windows. Native Wayland windows (most GTK 4 and Qt 6
apps on a Wayland session) are not listed and cannot be scaled; run them under
XWayland if they allow it (for example `QT_QPA_PLATFORM=xcb` or
`GDK_BACKEND=x11`).

## The window is blank or white

WebKitGTK, which draws the interface, shows a blank window with its DMA-BUF
renderer on some drivers (NVIDIA, some Wayland compositors). The launcher
turns that renderer off (`WEBKIT_DISABLE_DMABUF_RENDERER=1`). If you start
`/usr/lib/lirtup/lirtup-gui` directly, set it yourself. To try the renderer,
start `WEBKIT_DISABLE_DMABUF_RENDERER=0 lirtup-gui`.

## "The scaler is not responding"

The GUI could not reach `lirtup serve`. It starts it the first time it needs
it (when you start scaling or turn the daemon on). To see why it fails, run it
in a terminal:

```sh
lirtup -v serve
```

If the socket path is too long (a very long `XDG_RUNTIME_DIR`), pass a shorter
one with `--socket`.

## A window does not appear in the grid

The grid lists X11 windows that are mapped (not minimized) and bigger than a
few pixels. Restore a minimized window and it appears at the next refresh.

## The MagpieFX effects fail to compile

The MagpieFX engine needs `dxc` (`directx-shader-compiler`) or
`glslangValidator` (`glslang`). The three `Anime4K_Denoise_Bilateral_*` effects
need glslang, and `NIS`, `NVSharpen`, `FineSharp` and the four `SMAA` effects
need dxc. *About* ▸ *Diagnostics* says which compilers were found. See
[MagpieFX effects](magpiefx.md#compilers).

## Frame generation is not available

The *Presentation* tab and the tray say why:

- with the layer: lsfg-vk 2.x is not installed, or `lsfg-vk.dll` was not found
  (it comes with the `lsfg-vk` branch of Lossless Scaling on Steam);
- with the built-in engine: `Lossless.dll` was not found;
- `LSFGVK_*` or `DISABLE_LSFGVK` are set in the environment: the app then
  leaves the layer alone.

Set `frame_gen_dll` to the folder of the DLL if your Steam library is in an
unusual place.

## Reporting a problem

Copy *About* ▸ *Diagnostics* and the output of `lirtup -vv run …` (or
`lirtup -vv serve`) into the report. `LIRTUP_VULKAN_DEBUG=1` turns on the
Vulkan validation layer, if it is installed.
