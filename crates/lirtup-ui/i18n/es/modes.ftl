# modes (frente D): Modos de escalado, «Agregar efecto», «Eliminar modo» y «Medir en mi GPU».

## Lista
modes-title = Modos de escalado
modes-intro = Un modo junta modelo, muestreo y efectos. Cada perfil elige uno.
modes-list-label = Modos
modes-none = Todavía no hay modos.
modes-new = Nuevo modo
modes-new-name = Modo nuevo
modes-pick-one = Elige un modo para editarlo.
modes-no-effects = sin efectos
modes-effects-more = { $first } y { $count } más
modes-profiles = { $count ->
    [0] sin uso
    [one] 1 perfil
   *[other] { $count } perfiles
}

## Cabecera
modes-name = Nombre del modo
modes-name-empty = El nombre no puede quedar vacío
modes-rename = Renombrar
modes-rename-done = Aplicar el nombre
modes-rename-cancel = Cancelar el cambio de nombre
modes-duplicate = Duplicar
modes-delete = Eliminar
modes-unused = Sin uso
modes-used-by-one = Usado por { $name }
modes-used-by-many = Usado por { $names } y { $last }
modes-missing = Este modo ya no existe.
modes-back-to-list = Volver a la lista
modes-shared = { $count ->
    [one] Este modo lo usa 1 perfil.
   *[other] Este modo lo usan { $count } perfiles: los cambios se aplican a todos.
}
modes-shared-duplicate = Duplicar antes de editar
modes-shared-edit-anyway = Editar igual

## Modelo
modes-section-model = Modelo
modes-model = Modelo SRCNN
modes-model-help = 2×, de más rápido a más fino
modes-double-upscale = Escalado doble (4×)
modes-double-upscale-help = Dos pasadas 2×, por ejemplo de 720p a 2880p

## Muestreo
modes-section-sampling = Muestreo
modes-upsampler = Sobremuestreador
modes-upsampler-help = Tras SRCNN, para llegar al tamaño de salida
modes-upsampler-lanczos = Lanczos-2
modes-upsampler-fsr = AMD FSR 1.0
modes-upsampler-nis = NVIDIA NIS
modes-downsampler = Submuestreador
modes-downsampler-catmull = Catmull-Rom
modes-downsampler-lanczos = Lanczos adaptativo
modes-blur = Desenfoque
modes-lanczos-options = Opciones de Lanczos
modes-antiring = Anti-ringing
modes-tight-antiring = Anti-ringing estricto
modes-kernel-radius = Radio del núcleo
modes-kernel-radius-help = Automático salvo que lo fuerces
modes-kernel-radius-force = Forzar el radio

## Efectos
modes-section-effects = Efectos de Magpie
modes-engine = Motor de efectos
modes-engine-native = Nativo
modes-engine-magpiefx = MagpieFX
modes-engine-magpiefx-help = ejecuta los .hlsl de Magpie sin modificarlos · { $count } efectos en el catálogo ({ $verified } verificados; los demás llevan «Sin probar») y los tuyos en ~/.config/linux-rt-upscaler/effects · compila la primera vez (dxc o glslangValidator).
modes-engine-magpiefx-help-unloaded = ejecuta los .hlsl de Magpie sin modificarlos, los del catálogo y los tuyos en ~/.config/linux-rt-upscaler/effects · compila la primera vez (dxc o glslangValidator).
modes-engine-native-help = precompilado, carga al instante, controles afinados · solo 3 efectos por ahora.
modes-chain-label = Cadena de efectos
modes-chain-empty = La cadena está vacía. Agrega un efecto del catálogo.
modes-add-effect = Agregar efecto…
modes-untested = Sin probar
modes-effect-user = Tuyo
modes-effect-passes = { $count ->
    [one] 1 pasada
   *[other] { $count } pasadas
}
modes-effect-params = { $count ->
    [one] 1 parámetro
   *[other] { $count } parámetros
}
modes-effect-overrides = { $count ->
    [one] 1 cambiado
   *[other] { $count } cambiados
}
modes-effect-move = Mover { $name } (flechas arriba y abajo)
modes-effect-up = Subir
modes-effect-down = Bajar
modes-effect-remove = Quitar { $name }
modes-effect-params-toggle = Parámetros de { $name }
modes-effect-param-default = predeterminado: { $value }
modes-effect-reset-params = Restablecer los parámetros
modes-effect-compiling = Compilando…
modes-effect-disabled-badge = Desactivado
modes-effect-disabled = La sesión lo desactivó: { $error }
modes-effect-compile-error = No compila: { $error }
modes-effect-parse-error = No se pudo leer: { $error }
modes-effect-missing = No está en el catálogo: la sesión lo va a saltar.
modes-total = Total ≈ { $ms }/cuadro
modes-total-unknown = Sin medir en esta GPU
modes-measure = Medir en mi GPU

## Nativo
modes-anime4k = Anime4K_Upscale_S
modes-anime4k-help = Escala 2× · anime y arte lineal
modes-luma-sharpen = LumaSharpen
modes-luma-sharpen-help = Máscara de enfoque sobre la luminancia
modes-luma-sharpen-strength = Magnitud
modes-luma-sharpen-clamp = Límite por píxel
modes-luma-sharpen-pattern = Patrón de muestras
modes-luma-sharpen-offset-bias = Radio del patrón
modes-crt = CRT-Easymode
modes-crt-help = Máscara y líneas de un monitor CRT; reemplaza el escalado final
modes-crt-sharpness-h = Nitidez horizontal
modes-crt-sharpness-v = Nitidez vertical
modes-crt-mask-strength = Intensidad de la máscara
modes-crt-mask-dot-width = Ancho del punto
modes-crt-mask-dot-height = Alto del punto
modes-crt-mask-stagger = Desfase de la máscara
modes-crt-mask-size = Tamaño de la máscara
modes-crt-scanline-strength = Intensidad de las líneas
modes-crt-beam-min = Haz mínimo
modes-crt-beam-max = Haz máximo
modes-crt-bright-min = Brillo mínimo
modes-crt-bright-max = Brillo máximo
modes-crt-cutoff = Corte de líneas
modes-crt-gamma-input = Gamma de entrada
modes-crt-gamma-output = Gamma de salida
modes-crt-bright-boost = Refuerzo de brillo
modes-crt-dilation = Dilatación

## Retoque final
modes-section-final = Retoque final
modes-more-options = Opciones de { $name }
modes-strength = Intensidad
modes-cas = Contraste CAS
modes-cas-help = Mejora el contraste del texto y del arte lineal
modes-deband = Supresión de bandas
modes-deband-help = Antes del escalado, suaviza los degradados con escalones
modes-bloom = Resplandor
modes-bloom-threshold = Umbral de brillo
modes-bloom-radius = Radio
modes-vignette = Viñetado
modes-vignette-radius = Radio
modes-vignette-falloff = Suavidad
modes-grain = Grano
modes-grain-size = Tamaño del grano
modes-lut = LUT de color
modes-lut-help = Gradación de color con una tabla 3D incluida
modes-lut-intensity = Mezcla
modes-lut-preset = Tabla
modes-lut-identity = Ninguna
modes-lut-warm = Cálida
modes-lut-cool = Fría
modes-lut-split = Tonos divididos
modes-lut-vivid = Vívida
modes-lut-pastel = Pastel
modes-lut-lofi = Lo-fi
modes-lut-bleach = Blanqueo
modes-lut-film = Película
modes-lut-noir = Noir
modes-lut-sepia = Sepia
modes-lut-cyano = Cianotipia

## Pasadas en la medición
modes-pass-deband = Supresión de bandas
modes-pass-anime4k = Anime4K
modes-pass-luma_sharpen = LumaSharpen
modes-pass-crt_easymode = CRT-Easymode
modes-pass-cas = CAS
modes-pass-bloom = Resplandor
modes-pass-vignette = Viñetado
modes-pass-grain = Grano
modes-pass-lut = LUT

## Agregar efecto
modes-picker-title = Agregar efecto
modes-picker-search = Buscar efecto
modes-picker-filter = Filtro de efectos
modes-picker-all = Todos ({ $count })
modes-picker-verified = Verificados
modes-picker-mine = Míos
modes-picker-help = «Sin probar»: el efecto no se comparó con Magpie; puede verse distinto o no compilar.
modes-picker-loading = Cargando el catálogo…
modes-picker-empty = Ningún efecto coincide.
modes-picker-in-chain = En la cadena
modes-picker-add = Agregar
modes-close = Cerrar

## Eliminar modo
modes-delete-title = Eliminar modo
modes-delete-body = Se borra «{ $name }» de la carpeta de modos. No se puede deshacer.
modes-delete-body-used = Se borra «{ $name }». { $count ->
    [one] El perfil que lo usa pasa a usar el modo de Predeterminado.
   *[other] Los { $count } perfiles que lo usan pasan a usar el modo de Predeterminado.
}
modes-delete-confirm = Eliminar
modes-cancel = Cancelar

## Medir en mi GPU
bench-title = Medir en mi GPU
bench-what-mode = Mide cada pasada del modo «{ $name }» con una imagen de prueba.
bench-what-native = Mide las pasadas nativas con una imagen de prueba.
bench-sizes = Tamaños
bench-framegen = Incluir la generación de cuadros
bench-framegen-help = ×2 a ×4, con los dos modelos; tarda bastante más
bench-unsaved = El modo tiene cambios sin guardar: se mide la versión guardada.
bench-start = Medir
bench-cancel = Cancelar
bench-again = Medir otra vez
bench-done = Listo
bench-preparing = Preparando…
bench-progress = Midiendo { $done } de { $total }…
bench-keep-open = Evita usar la GPU mientras tanto.
bench-cancelled = Se canceló: estos son los resultados hasta ese momento.
bench-nothing = No se midió nada.
bench-col-pass = Pasada
bench-col-median = Mediana (ms)
bench-col-p95 = p95 (ms)
bench-total = Total del modo ≈ { $ms }/cuadro
bench-framegen-case = Generación de cuadros { $case }
