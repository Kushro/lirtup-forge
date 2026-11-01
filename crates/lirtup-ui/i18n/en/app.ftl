# Common frame (reviewer): title bar, drawer, save bar, dialogs.
app-title = LirtUp Forge
titlebar-minimize = Minimize
titlebar-maximize = Maximize
titlebar-close = Close
titlebar-close-to-tray = Close to tray
nav-home = Home
nav-modes = Scaling modes
nav-profiles = Profiles
nav-default-profile = Default
nav-new-profile = New profile
nav-settings = Settings
nav-about = About
nav-running = Scaling
nav-unsaved = Unsaved
savebar-changes = { $count ->
    [one] { $count } unsaved change
   *[other] { $count } unsaved changes
}
savebar-save = Save
savebar-reset = Reset
savebar-reset-more = More reset options
savebar-reset-tab = Undo the changes of this tab
savebar-remove-overrides = Remove the profile's settings in this tab
savebar-reset-all = Reset everything to the system defaults
toast-saved = Profile “{ $name }” saved.
toast-undo = Undo
toast-undone = The previous version was restored.
toast-write-failed = Could not save: { $error }
unsaved-title = Unsaved changes
unsaved-body = You have unsaved changes in “{ $name }”. Save them before switching, discard them, or cancel to stay.
unsaved-save = Save and switch
unsaved-discard = Discard and switch
unsaved-cancel = Cancel
link-connecting = Connecting to the scaler…
link-down = The scaler does not answer: { $reason }
link-version = The scaler speaks another protocol version ({ $server }, the GUI { $gui }).
link-restart = Restart the scaler
