# home (front B): Home.
home-title = Home

## Session card
home-session-starting = Starting { $title }…
home-session-starting-aria = Starting the session
home-session-running = Scaling { $title }
home-session-restart = Restart
home-session-stop = Stop
home-session-stop-hint = Stop scaling
home-session-gpu-warning = The GPU cannot keep up: { $gpu } ms per frame of { $budget } ms available
home-session-failed = Could not scale: { $stage }
home-session-retry = Retry
home-stage-config = could not read the configuration
home-stage-window = the window was not found
home-stage-device = GPU or Vulkan error
home-stage-capture = capture error
home-stage-effects = could not prepare the effects
home-stage-overlay = output window error
home-stage-swapchain = presentation error
home-stage-framegen = frame generation error
home-stage-runtime = error while scaling

## Activation
home-activation-title = Activation
home-activation-help = Press a hotkey to start or stop scaling the focused window, or pick a window below.
home-activation-fullscreen = Scale fullscreen
home-activation-fullscreen-subtitle = The focused window, with its profile's mode
home-activation-windowed = Scale in a window
home-activation-windowed-subtitle = Always on top, movable from the toolbar
home-activation-edit-hotkey = Change hotkey
home-activation-no-hotkey = No hotkey
home-delay-title = Deferred scaling
home-delay-subtitle = Scales the focused window after { $seconds } s: time to go back to the game
home-delay-start = Start
home-delay-start-fullscreen = Start fullscreen
home-delay-start-windowed = Start in a window
home-delay-choose = Choose the delay
home-delay-option = { $seconds } s
home-daemon-title = Daemon mode
home-daemon-subtitle = Scales only the windows that match a profile, even if you close this window
home-daemon-watching = { $count ->
    [one] Watching { $count } profile
   *[other] Watching { $count } profiles
}

## Open windows
home-windows-title = Open windows
home-filter-label = Quick filter
home-filter-all = All
home-filter-with-profile = With profile
home-filter-without-profile = Without profile
home-search-label = Filter windows
home-search-placeholder = Filter windows by title or class…
home-windows-empty = There are no open windows to scale.
home-windows-no-match = No window matches the filter.
home-windows-hint = Click: scale with the mode of the matching profile. Right click or ⋮: scale in a window, create a profile from this window, copy the title. Previews refresh every { $ms } ms and pause while the app is in the background.
home-tile-menu-windowed = Scale in a window
home-tile-menu-profile = Create a profile from this window
home-tile-menu-copy = Copy title
