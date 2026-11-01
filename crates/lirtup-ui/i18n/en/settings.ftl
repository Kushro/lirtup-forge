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
settings-close-to-tray = Close to the tray
settings-close-to-tray-help = The close button hides the window instead of quitting
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
