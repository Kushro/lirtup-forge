# modes (front D): Scaling modes, «Add effect», «Delete mode» and «Benchmark on my GPU».

## List
modes-title = Scaling modes
modes-intro = A mode brings together model, sampling and effects. Each profile picks one.
modes-list-label = Modes
modes-none = There are no modes yet.
modes-new = New mode
modes-new-name = New mode
modes-pick-one = Pick a mode to edit it.
modes-no-effects = no effects
modes-effects-more = { $first } and { $count } more
modes-profiles = { $count ->
    [0] unused
    [one] 1 profile
   *[other] { $count } profiles
}

## Header
modes-name = Mode name
modes-name-empty = The name cannot be empty
modes-rename = Rename
modes-rename-done = Apply the name
modes-rename-cancel = Cancel renaming
modes-duplicate = Duplicate
modes-delete = Delete
modes-unused = Unused
modes-used-by-one = Used by { $name }
modes-used-by-many = Used by { $names } and { $last }
modes-missing = This mode no longer exists.
modes-back-to-list = Back to the list
modes-shared = { $count ->
    [one] 1 profile uses this mode.
   *[other] { $count } profiles use this mode: changes apply to all of them.
}
modes-shared-duplicate = Duplicate before editing
modes-shared-edit-anyway = Edit anyway

## Model
modes-section-model = Model
modes-model = SRCNN model
modes-model-help = 2×, from fastest to finest
modes-double-upscale = Double upscale (4×)
modes-double-upscale-help = Two 2× passes, for example from 720p to 2880p

## Sampling
modes-section-sampling = Sampling
modes-upsampler = Upsampler
modes-upsampler-help = After SRCNN, to reach the output size
modes-upsampler-lanczos = Lanczos-2
modes-upsampler-fsr = AMD FSR 1.0
modes-upsampler-nis = NVIDIA NIS
modes-downsampler = Downsampler
modes-downsampler-catmull = Catmull-Rom
modes-downsampler-lanczos = Adaptive Lanczos
modes-blur = Blur
modes-lanczos-options = Lanczos options
modes-antiring = Anti-ringing
modes-tight-antiring = Tight anti-ringing
modes-kernel-radius = Kernel radius
modes-kernel-radius-help = Automatic unless you force it
modes-kernel-radius-force = Force the radius

## Effects
modes-section-effects = Magpie effects
modes-effects-help = runs Magpie's .hlsl unchanged · { $count } effects in the catalog ({ $verified } verified; the rest are marked «Untested») and yours in ~/.config/linux-rt-upscaler/effects · the bundled ones come precompiled; the ones you add or edit compile the first time (dxc or glslangValidator).
modes-effects-help-unloaded = runs Magpie's .hlsl unchanged, the catalog's and yours in ~/.config/linux-rt-upscaler/effects · the bundled ones come precompiled; the ones you add or edit compile the first time (dxc or glslangValidator).
modes-legacy-banner = This mode has settings of the retired native effects engine. Scaling applies them as MagpieFX effects (shown below, read-only); migrate the configuration to edit them here.
modes-legacy-migrate = Migrate…
modes-chain-label = Effect chain
modes-chain-before = Before the model
modes-chain-before-help = On the capture, at the game's resolution; the model gets the result.
modes-chain-before-empty = Nothing before the model.
modes-chain-after = After the model
modes-chain-after-help = On the model's output, before the final scaling.
modes-chain-empty = Nothing after the model.
modes-chain-model = Model { $model } · { $scale }×
modes-chain-no-model = No model: both lists run back to back, at the game's resolution.
modes-add-effect = Add effect…
modes-untested = Untested
modes-effect-user = Yours
modes-effect-passes = { $count ->
    [one] 1 pass
   *[other] { $count } passes
}
modes-effect-params = { $count ->
    [one] 1 parameter
   *[other] { $count } parameters
}
modes-effect-overrides = { $count ->
    [one] 1 changed
   *[other] { $count } changed
}
modes-effect-move = Move { $name } (up and down arrows)
modes-effect-up = Move up
modes-effect-down = Move down
modes-effect-to-before = Move before the model
modes-effect-to-after = Move after the model
modes-effect-remove = Remove { $name }
modes-effect-params-toggle = Parameters of { $name }
modes-effect-param-default = default: { $value }
modes-effect-reset-params = Reset the parameters
modes-effect-compiling = Compiling…
modes-effect-disabled-badge = Disabled
modes-effect-disabled = The session disabled it: { $error }
modes-effect-compile-error = Does not compile: { $error }
modes-effect-parse-error = Could not be read: { $error }
modes-effect-missing = Not in the catalog: the session will skip it.
modes-total = Total ≈ { $ms }/frame
modes-total-unknown = Not measured on this GPU
modes-measure = Benchmark on my GPU

## Final touches
modes-section-final = Final touches
modes-more-options = { $name } options
modes-strength = Strength
modes-cas = CAS contrast
modes-cas-help = Improves the contrast of text and line art
modes-deband = Debanding
modes-deband-help = Before scaling, smooths stepped gradients
modes-bloom = Bloom
modes-bloom-threshold = Brightness threshold
modes-bloom-radius = Radius
modes-vignette = Vignette
modes-vignette-radius = Radius
modes-vignette-falloff = Softness
modes-grain = Grain
modes-grain-size = Grain size
modes-lut = Color LUT
modes-lut-help = Color grading with a bundled 3D table
modes-lut-intensity = Blend
modes-lut-preset = Table
modes-lut-identity = None
modes-lut-warm = Warm
modes-lut-cool = Cool
modes-lut-split = Split toning
modes-lut-vivid = Vivid
modes-lut-pastel = Pastel
modes-lut-lofi = Lo-fi
modes-lut-bleach = Bleach bypass
modes-lut-film = Film
modes-lut-noir = Noir
modes-lut-sepia = Sepia
modes-lut-cyano = Cyanotype

## Passes in the benchmark
modes-pass-deband = Debanding
modes-pass-cas = CAS
modes-pass-bloom = Bloom
modes-pass-vignette = Vignette
modes-pass-grain = Grain
modes-pass-lut = LUT

## Add effect
modes-picker-title = Add effect
modes-picker-search = Search effects
modes-picker-filter = Effect filter
modes-picker-all = All ({ $count })
modes-picker-verified = Verified
modes-picker-mine = Mine
modes-picker-help = «Untested»: the effect was not compared with Magpie; it may look different or not compile.
modes-picker-loading = Loading the catalog…
modes-picker-empty = No effect matches.
modes-picker-in-chain = In the chain
modes-picker-add = Add
modes-close = Close

## Delete mode
modes-delete-title = Delete mode
modes-delete-body = «{ $name }» is removed from the modes folder. This cannot be undone.
modes-delete-body-used = «{ $name }» is removed. { $count ->
    [one] The profile that uses it switches to Default's mode.
   *[other] The { $count } profiles that use it switch to Default's mode.
}
modes-delete-confirm = Delete
modes-cancel = Cancel

## Benchmark on my GPU
bench-title = Benchmark on my GPU
bench-what-mode = Measures each pass of the mode «{ $name }» with a test image.
bench-what-native = Measures LumaSharpen, Anime4K_Upscale_S and CRT_Easymode (MagpieFX) with a test image.
bench-sizes = Sizes
bench-framegen = Include frame generation
bench-framegen-help = ×2 to ×4, with both models; takes much longer
bench-unsaved = The mode has unsaved changes: the saved version is measured.
bench-start = Measure
bench-cancel = Cancel
bench-again = Measure again
bench-done = Done
bench-preparing = Preparing…
bench-progress = Measuring { $done } of { $total }…
bench-keep-open = Avoid using the GPU meanwhile.
bench-cancelled = Cancelled: these are the results up to that point.
bench-nothing = Nothing was measured.
bench-col-pass = Pass
bench-col-median = Median (ms)
bench-col-p95 = p95 (ms)
bench-total = Mode total ≈ { $ms }/frame
bench-framegen-case = Frame generation { $case }
