# profile (front C): profile page, New profile, dialogs and state toasts.
profile-tab-scaling = Scaling
profile-tab-presentation = Presentation
profile-tab-display = Display
profile-tab-extras = Extras
profile-tab-advanced = Advanced

## Header
profile-default-title = Default
profile-default-subtitle = What every profile uses until it changes a setting.
profile-no-rules = No rules: applies only when picked by name
profile-edit = Edit profile
profile-edit-done = Done
profile-more = More: duplicate, delete
profile-duplicate = Duplicate
profile-delete = Delete
profile-name = Name
profile-rule-title = Title is “{ $value }”
profile-rule-title-contains = Title contains “{ $value }”
profile-rule-title-regex = Title matches /{ $value }/
profile-rule-title-startswith = Title starts with “{ $value }”
profile-rule-title-endswith = Title ends with “{ $value }”
profile-rule-width = Width { $value }
profile-rule-height = Height { $value }
profile-rule-unknown = Unknown rule: { $value }

## Origin of each row
profile-inherits = Inherits from Default
profile-inherits-system = System value
profile-changed = Changed in this profile
profile-changed-default = Changed in Default
profile-unsaved-before = Unsaved · before: { $before }
profile-global = Global
profile-global-note = profiles cannot change it
profile-experimental = Experimental
profile-on = On
profile-off = Off

## Scaling
profile-group-mode = Scaling mode
profile-mode = Scaling mode
profile-mode-inherit = Use Default's
profile-mode-none = No mode (loose settings)
profile-mode-edit = Edit mode
profile-mode-missing = Mode “{ $id }” does not exist
profile-group-focus = Focus tracking
profile-follow-focus = Follow focus
profile-follow-focus-sub = Scale whichever window has the focus
profile-pause-focus = Pause on focus loss
profile-pause-focus-sub = Hide the overlay while the game is not focused
profile-group-auto = Automatic scaling
profile-daemon-exclude = Exclude from daemon mode
profile-daemon-exclude-sub = The daemon will not scale this window on its own
profile-loose-mode-fields = This profile has { $count ->
    [one] { $count } loose mode setting
   *[other] { $count } loose mode settings
} from 1.x; they win over the mode until the migration.

## Presentation
profile-group-window = Scaling window
profile-overlay-mode = Window mode
profile-overlay-always-on-top = Always on top
profile-overlay-top-transparent = On top, mouse passes through
profile-overlay-fullscreen = Fullscreen
profile-overlay-windowed = Windowed
profile-geometry = Output geometry
profile-geometry-fit = Fit
profile-geometry-stretch = Stretch
profile-geometry-cover = Cover
profile-geometry-custom = Custom
profile-geometry-custom-label = Geometry (50%, 1920x, x1080, 1920x1080!)
profile-geometry-invalid = Invalid geometry
profile-menu-bar = Detect menu bar
profile-menu-bar-sub = Crops the Wine menu bar in the first seconds
profile-hide-cursor = Hide cursor
profile-hide-cursor-sub = after inactivity, in seconds
profile-group-crop = Crop and offset
profile-crop-sub = in pixels
profile-crop-top = Top
profile-crop-bottom = Bottom
profile-crop-left = Left
profile-crop-right = Right
profile-offset-x = X offset
profile-offset-y = Y offset
profile-background = Background color
profile-background-sub = Bars behind the image; transparency allowed

## Frame generation
fg-title = Frame generation
fg-active = Active at ×{ $n }
fg-active-auto = Active with the automatic multiplier
fg-off = Off
fg-configured = Set to { $value }
fg-not-running = No session: applies when scaling
fg-sub = Smoother motion: up to N times the captured fps · adds about one frame of latency
fg-ready = Engine ready
fg-not-ready = Engine unavailable
fg-engine = Engine
fg-engine-layer = lsfg-vk layer
fg-engine-native = Built in (experimental)
fg-multiplier = Multiplier
fg-multiplier-off = Off
fg-multiplier-auto = Auto
fg-auto-needs-native = The automatic multiplier only works with the built-in engine.
fg-flow-scale = Flow scale
fg-flow-scale-sub = Motion estimated at this fraction of the output resolution; lower is lighter.
fg-model = Model
fg-model-quality = Quality (LSFG 3.1)
fg-model-performance = Performance (3.1P)
fg-dll = DLL folder
fg-dll-sub = Empty: search Steam
fg-measure = Measure on my GPU
fg-restart-note = Engine, flow scale and model apply when the session restarts.
fg-save-restart = Save and restart
fg-env-locked = Set by LSFGVK_* variables: the GUI settings are not used.
fg-reason-layer-missing = The lsfg-vk layer is not installed.
fg-reason-lossless-dll-missing = Lossless.dll was not found (Lossless Scaling, from Steam).
fg-reason-lsfg-vk-dll-missing = The DLL lsfg-vk needs was not found.
fg-reason-env-locked = LSFGVK_* variables configure it from outside.
fg-reason-no-external-fd = The driver cannot share memory by fd (VK_KHR_external_memory_fd): the built-in engine cannot run.
fg-reason-engine-failed = lsfg failed during the session and frame generation was turned off.

## Display
profile-group-devices = Devices
profile-monitor = Monitor
profile-monitor-primary = Primary
profile-monitor-all = All
profile-gpu = GPU
profile-gpu-auto = Automatic (best)
profile-group-vsync = V-Sync
profile-present-mode = Present mode
profile-present-mode-sub = fifo: no tearing, lower power · mailbox: lower latency · immediate
profile-present-fifo = fifo (V-Sync)
profile-present-mailbox = mailbox
profile-present-immediate = immediate (no V-Sync)
profile-max-fps = Limit FPS
profile-present-on-change = Present only new frames
profile-present-on-change-sub = Saves GPU while the game is not drawing
profile-group-scale = Scale factor
profile-scale-factor = Manual factor
profile-scale-factor-sub = Off: from the monitor and window resolution

## Extras
profile-group-screenshots = Screenshots
profile-screenshot-dir = Folder
profile-screenshot-filename = Template
profile-screenshot-filename-sub = { "{" }timestamp:%Y%m%d_%H%M%S{ "}" } becomes the date
profile-group-osd = On-screen messages
profile-show-osd = Show OSD
profile-osd-duration = OSD duration
profile-osd-duration-sub = in seconds
profile-show-stats = Stats panel
profile-show-stats-sub = Start with the fps, GPU times and chain panel
profile-latency-log = Latency log
profile-latency-log-sub = CSV with every frame's latency; empty to not log

## Advanced
profile-group-tiles = Tile processing
profile-tiles = Enable tile processing
profile-tiles-sub = Processes only the parts that changed
profile-damage = Damage tracking
profile-tile-size = Tile size
profile-tile-margin = Tile margin
profile-tile-layers = Tiles per frame limit
profile-area-threshold = Area threshold
profile-area-threshold-sub = Changed fraction that forces a whole frame
profile-group-timing = Timing
profile-frame-timeout = Frame wait
profile-frame-timeout-sub = in milliseconds
profile-focus-poll = Focus polling
profile-pipeline-poll = Idle polling
profile-daemon-poll = Daemon polling
profile-seconds = in seconds
profile-group-recovery = Error recovery
profile-capture-failures = Capture failure limit
profile-capture-delay = Delay after a failure
profile-swapchain-debounce = Swapchain recreation wait

## Reset (save bar, named profile)
profile-reset-all-named = Remove every setting of the profile

## Dialogs
profile-delete-title = Delete profile
profile-delete-body = “{ $name }” is removed from config.yaml. The backup (config.yaml.bak1) keeps the previous version.
profile-delete-confirm = Delete
profile-cancel = Cancel

## New profile
new-profile-title = New profile
new-profile-intro = A profile keeps the settings of a game or program. It applies to the windows that meet every rule.
new-profile-close = Close
new-profile-icon-capture = Capture the icon from a window
new-profile-icon-file = Load the icon from a file
new-profile-icon-remove = Remove the icon
new-profile-name-empty = Type a name
new-profile-name-taken = There is already a profile with that name
new-profile-mode = Scaling mode
new-profile-copy-from = Copy settings from
new-profile-rules = Match rules
new-profile-add-rule = Add rule
new-profile-remove-rule = Remove rule
new-profile-rule-kind = Rule type
new-profile-rule-value = Value
new-profile-rules-help = Title: exact, contains or regex (case-insensitive) · Size: 1920, >1280, <800
new-profile-rule-placeholder-size = e.g., >800
new-profile-kind-title = Exact title
new-profile-kind-title-contains = Title contains
new-profile-kind-title-regex = Title (regex)
new-profile-kind-title-startswith = Title starts with
new-profile-kind-title-endswith = Title ends with
new-profile-kind-width = Width
new-profile-kind-height = Height
new-profile-matches = { $count ->
    [one] Matches 1 open window: { $first }
   *[other] Matches { $count } open windows: { $first }…
}
new-profile-matches-none = Matches no open window.
new-profile-no-rules = Without rules the profile applies only when picked by name.
new-profile-create = Create profile
new-profile-capture = Capture window
new-profile-capture-help = Pick a window to fill in the name, icon and rules. You can also press the button and click the window.
new-profile-point = Point at window
new-profile-pointing = Click a window…
new-profile-no-windows = No open windows.
new-profile-from-tile = From Home, “Create profile” in a tile's menu opens this dialog with the window already captured.

## Toasts
profile-toast-saved = Changes saved.
profile-toast-created = Profile “{ $name }” created.
profile-toast-duplicated = Profile duplicated as “{ $name }”.
profile-toast-deleted = Profile “{ $name }” deleted.
profile-toast-renamed = Profile renamed to “{ $name }”.
profile-toast-setting = The change from the bar was saved in “{ $name }”.
profile-toast-setting-default = The change from the bar was saved in Default.
profile-toast-moved = “{ $name }” moved: the first profile that matches wins.
profile-icon = Icon
profile-toast-menubar = The menu bar crop was saved in “{ $name }”.
profile-toast-mode-created = Mode “{ $name }” created.
profile-toast-mode-deleted = Mode “{ $name }” deleted.
