# settings (front D).
settings-title = Settings
settings-tab-general = General
settings-tab-hotkeys = Hotkeys
settings-tab-style = Style

## General
settings-appearance = Appearance
settings-language = Language
settings-language-help = Applies at once, without restarting
settings-language-system = System
settings-theme = Theme
settings-theme-help = Light, dark or the desktop's
settings-theme-system = System
settings-theme-light = Light
settings-theme-dark = Dark
settings-seed = Color
settings-seed-help = Seed of the theme; the old palettes remain as seeds
settings-tray = System tray
settings-show-tray-icon = Show the tray icon
settings-show-tray-icon-help = Without the icon, closing the window quits LirtUp Forge
settings-close-to-tray = Close to the tray
settings-close-to-tray-help = The close button hides the window instead of quitting
settings-minimize-to-tray = Minimize to the tray
settings-minimize-to-tray-help = The minimize button hides the window in the tray, not the taskbar
settings-start-hidden = Start hidden
settings-start-hidden-help = On launch, only the tray icon shows
settings-auto = Automatic scaling
settings-daemon = Daemon mode
settings-daemon-help = A background process scales the windows that match a profile
settings-daemon-watching = { $count ->
    [0] On, no profiles with rules to watch
    [one] On, watching 1 profile
   *[other] On, watching { $count } profiles
}
settings-keep-running = Keep running after quitting
settings-keep-running-help = The daemon stays on even when you close LirtUp Forge

## Tray: menu, notifications and quitting (round 4, front Y)
tray-idle = Idle
tray-scaling = Scaling: { $title }
tray-starting = Starting: { $title }
tray-failed = Scaling failed
tray-watching = { $count ->
    [0] Daemon on, no profiles to watch
    [one] Daemon watching 1 profile
   *[other] Daemon watching { $count } profiles
}
tray-stop = Stop
tray-restart = Restart
tray-profile = Profile
tray-profile-of = Profile: { $name }
tray-framegen = Frame generation: { $value }
tray-framegen-note = Choosing one restarts the session
tray-daemon = Daemon mode
tray-show = Show LirtUp Forge
tray-quit = Quit
notify-scaling = Scaling «{ $title }». Use «Stop» in the tray menu to go back.
notify-failed = Scaling failed: { $error }
notify-stop = Stop
quit-body = You have unsaved changes. Save them before quitting, discard them, or cancel to stay in LirtUp Forge.
quit-save = Save and quit
quit-discard = Discard and quit

## Hotkeys
settings-hotkeys-help = Press «Change hotkey», then the combination; it needs at least one modifier. Esc cancels, Backspace leaves it unassigned. Saved at once.
settings-hotkeys-reset-all = Reset all
settings-hotkeys-activation = Activation
settings-hotkeys-session = Session
settings-hotkeys-view = View
settings-hotkeys-zoom = Zoom and offset
settings-hotkey-default = Default: { $keys }
settings-hotkey-conflict = Also used by: { $actions }
settings-hotkey-start_fullscreen = Scale fullscreen
settings-hotkey-start_windowed = Scale in a window
settings-hotkey-toggle_scaling = Show or hide the scaling
settings-hotkey-exit_app = End the session
settings-hotkey-screenshot = Take a screenshot
settings-hotkey-toggle_stats = Show statistics
settings-hotkey-toggle_toolbar = Show or hide the bar
settings-hotkey-cycle_model = Cycle model
settings-hotkey-cycle_geometry = Cycle geometry
settings-hotkey-restore_view = Restore view
settings-hotkey-zoom_in = Zoom in
settings-hotkey-zoom_out = Zoom out
settings-hotkey-offset_up = Move the content up
settings-hotkey-offset_down = Move the content down
settings-hotkey-offset_left = Move the content left
settings-hotkey-offset_right = Move the content right

## Style
settings-zoom = Zoom
settings-zoom-help = Size of the whole interface
settings-font-scale = Font scale
settings-font-scale-help = Text only
settings-font = Font
settings-font-help = Empty: Google Sans Flex, the bundled one
settings-grid = Window grid
settings-aspect = Tile shape
settings-aspect-help = Auto follows the shape of each window
settings-aspect-auto = Auto
settings-columns = Columns
settings-columns-help = Auto fills the available width
settings-columns-auto = Auto
settings-intervals = Refresh and preview
settings-intervals-note = list every { $list } ms · preview every { $preview } ms
settings-refresh = Window list
settings-preview = Previews
settings-preview-help = More often uses more CPU
settings-drawer-note = Side bar widths are no longer adjustable: the drawer is 288 px wide and folds into an 80 px rail below 1100 px.
