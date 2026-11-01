# Tray and daemon

## How the app runs

The GUI and the scaler are two processes. `lirtup-gui` is the window;
`lirtup serve` captures, scales and presents, and the GUI talks to it over a
socket in `$XDG_RUNTIME_DIR/lirtup/`. The GUI starts it the first time it
needs it.

By default the scaler ends when you quit the GUI. With *Keep running after
exit* (*Settings* ▸ *General*) it stays: the daemon keeps scaling the windows
that match a profile, and opening the GUI again connects to it.

Only one GUI runs at a time: starting it again shows the open window.

## Tray icon

*Settings* ▸ *General* turns on the tray icon, and decides whether closing and
minimizing the window send it to the tray, and whether the app starts hidden
in it. The icon has four states: idle, daemon watching, scaling, and error.

Its menu:

- **Scaling: _window_**, with the frame rate and the mode, while a window is
  scaled;
- **Stop** and **Restart** (scale the same window again, for example after
  changing a setting);
- **Profile** ▸ the profile for the session; picking one while scaling
  restarts the session with it;
- **Frame generation** ▸ *Off*, ×2, ×3, ×4 or *Auto*, with the reason when an
  engine is missing. Changing it restarts the session;
- **Daemon mode**;
- **Show** the window, and **Quit**.

On desktops without a tray (GNOME without the AppIndicator extension), the
icon does not appear and closing the window quits the app.

## Daemon mode

With *Daemon mode* on (*Home*, the tray, `lirtup serve --daemon` or
`lirtup run -d`), the scaler watches the open windows. When one matches a
profile, it:

1. focuses it;
2. scales it with the profile's mode and settings;
3. when the window closes, goes back to watching.

The windows of profiles with *Exclude from the daemon* (`daemon_exclude`) are
skipped. `daemon_poll_interval` (seconds, default 2) sets how often it looks.
With *Follow focus*, the daemon hands over to the focused window.

## Notifications

The app sends desktop notifications when the daemon starts scaling a window
and when scaling fails. The notification of a running session has a *Stop*
button.
