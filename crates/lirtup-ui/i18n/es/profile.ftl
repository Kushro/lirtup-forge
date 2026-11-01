# profile (frente C): página de perfil, Nuevo perfil, diálogos y avisos del estado.
profile-tab-scaling = Escalado
profile-tab-presentation = Presentación
profile-tab-display = Pantalla
profile-tab-extras = Extras
profile-tab-advanced = Avanzado

## Cabecera
profile-default-title = Predeterminado
profile-default-subtitle = Lo que usan todos los perfiles mientras no cambien un ajuste.
profile-no-rules = Sin reglas: solo se aplica al elegirlo por nombre
profile-edit = Editar perfil
profile-edit-done = Listo
profile-more = Más: duplicar, eliminar
profile-duplicate = Duplicar
profile-delete = Eliminar
profile-name = Nombre
profile-rule-title = Título es «{ $value }»
profile-rule-title-contains = Título contiene «{ $value }»
profile-rule-title-regex = Título cumple /{ $value }/
profile-rule-title-startswith = Título empieza con «{ $value }»
profile-rule-title-endswith = Título termina con «{ $value }»
profile-rule-width = Ancho { $value }
profile-rule-height = Alto { $value }
profile-rule-unknown = Regla desconocida: { $value }

## Origen de cada fila
profile-inherits = Hereda de Predeterminado
profile-inherits-system = Valor del sistema
profile-changed = Cambiado en este perfil
profile-changed-default = Cambiado en Predeterminado
profile-unsaved-before = Sin guardar · antes: { $before }
profile-global = Global
profile-global-note = los perfiles no lo cambian
profile-experimental = Experimental
profile-on = Activado
profile-off = Desactivado

## Escalado
profile-group-mode = Modo de escalado
profile-mode = Modo de escalado
profile-mode-inherit = Usar el de Predeterminado
profile-mode-none = Sin modo (ajustes sueltos)
profile-mode-edit = Editar modo
profile-mode-missing = El modo «{ $id }» no existe
profile-group-focus = Seguimiento del enfoque
profile-follow-focus = Seguir enfoque
profile-follow-focus-sub = Escala la ventana que tenga el foco
profile-pause-focus = Pausa al perder foco
profile-pause-focus-sub = Oculta la superposición mientras el juego no tiene el foco
profile-group-auto = Escalado automático
profile-daemon-exclude = Excluir del modo daemon
profile-daemon-exclude-sub = El daemon no escalará solo esta ventana
profile-loose-mode-fields = Este perfil tiene { $count ->
    [one] { $count } ajuste de modo suelto
   *[other] { $count } ajustes de modo sueltos
} de la 1.x; ganan sobre el modo hasta la migración.

## Presentación
profile-group-window = Ventana de escalado
profile-overlay-mode = Modo de ventana
profile-overlay-always-on-top = Siempre encima
profile-overlay-top-transparent = Encima y transparente al ratón
profile-overlay-fullscreen = Pantalla completa
profile-overlay-windowed = En ventana
profile-geometry = Geometría de salida
profile-geometry-fit = Ajustar
profile-geometry-stretch = Estirar
profile-geometry-cover = Cubrir
profile-geometry-custom = Personalizada
profile-geometry-custom-label = Geometría (50%, 1920x, x1080, 1920x1080!)
profile-geometry-invalid = Geometría no válida
profile-menu-bar = Detectar barra de menú
profile-menu-bar-sub = Recorta la barra de menú de Wine en los primeros segundos
profile-hide-cursor = Esconder cursor
profile-hide-cursor-sub = tras inactividad, en segundos
profile-group-crop = Recorte y desplazamiento
profile-crop-sub = en píxeles
profile-crop-top = Superior
profile-crop-bottom = Inferior
profile-crop-left = Izquierdo
profile-crop-right = Derecho
profile-offset-x = Desplazamiento en X
profile-offset-y = Desplazamiento en Y
profile-background = Color de fondo
profile-background-sub = Franjas detrás de la imagen; admite transparencia

## Generación de cuadros
fg-title = Generación de cuadros
fg-active = Activa con ×{ $n }
fg-active-auto = Activa con el multiplicador automático
fg-off = Apagada
fg-configured = Configurada en { $value }
fg-not-running = Sin sesión: se aplica al escalar
fg-sub = Movimiento más fluido: hasta N veces los fps capturados · suma cerca de un cuadro de latencia
fg-ready = Motor listo
fg-not-ready = Motor no disponible
fg-engine = Motor
fg-engine-layer = Capa lsfg-vk
fg-engine-native = Integrado (experimental)
fg-multiplier = Multiplicador
fg-multiplier-off = Apagado
fg-multiplier-auto = Auto
fg-auto-needs-native = El multiplicador automático solo funciona con el motor integrado.
fg-flow-scale = Escala de flujo
fg-flow-scale-sub = Movimiento estimado a esta fracción de la resolución de salida; menos es más liviano.
fg-model = Modelo
fg-model-quality = Calidad (LSFG 3.1)
fg-model-performance = Rendimiento (3.1P)
fg-dll = Carpeta de la DLL
fg-dll-sub = Vacía: se busca en Steam
fg-measure = Medir en mi GPU
fg-restart-note = El motor, la escala de flujo y el modelo se aplican al reiniciar la sesión.
fg-save-restart = Guardar y reiniciar
fg-env-locked = Definida por variables LSFGVK_*: la configuración de la GUI no se usa.
fg-reason-layer-missing = La capa lsfg-vk no está instalada.
fg-reason-lossless-dll-missing = No se encontró Lossless.dll (Lossless Scaling, de Steam).
fg-reason-lsfg-vk-dll-missing = No se encontró la DLL que necesita lsfg-vk.
fg-reason-env-locked = Las variables LSFGVK_* la configuran desde fuera.
fg-reason-no-external-fd = El controlador no comparte memoria por fd (VK_KHR_external_memory_fd): el motor integrado no puede correr.
fg-reason-engine-failed = lsfg falló durante la sesión y se apagó la generación de cuadros.

## Pantalla
profile-group-devices = Dispositivos
profile-monitor = Monitor
profile-monitor-primary = Principal
profile-monitor-all = Todos
profile-gpu = GPU
profile-gpu-auto = Automática (la mejor)
profile-group-vsync = V-Sync
profile-present-mode = Modo de presentación
profile-present-mode-sub = fifo: sin tearing, menor consumo · mailbox: menor latencia · immediate
profile-present-fifo = fifo (V-Sync)
profile-present-mailbox = mailbox
profile-present-immediate = immediate (sin V-Sync)
profile-max-fps = Limitar FPS
profile-present-on-change = Presentar solo con cuadros nuevos
profile-present-on-change-sub = Ahorra GPU cuando el juego no dibuja
profile-group-scale = Factor de escala
profile-scale-factor = Factor manual
profile-scale-factor-sub = Apagado: según la resolución del monitor y de la ventana

## Extras
profile-group-screenshots = Capturas de pantalla
profile-screenshot-dir = Directorio
profile-screenshot-filename = Plantilla
profile-screenshot-filename-sub = { "{" }timestamp:%Y%m%d_%H%M%S{ "}" } se reemplaza por la fecha
profile-group-osd = Mensajes en pantalla
profile-show-osd = Mostrar OSD
profile-osd-duration = Duración del OSD
profile-osd-duration-sub = en segundos
profile-show-stats = Panel de estadísticas
profile-show-stats-sub = Empieza con el panel de fps, tiempos de GPU y cadena
profile-latency-log = Registro de latencia
profile-latency-log-sub = CSV con la latencia de cada cuadro; vacío para no registrar

## Avanzado
profile-group-tiles = Procesamiento por bloques
profile-tiles = Habilitar procesamiento por bloques
profile-tiles-sub = Procesa solo las partes que cambiaron
profile-damage = Seguimiento de daño
profile-tile-size = Tamaño del bloque
profile-tile-margin = Margen del bloque
profile-tile-layers = Límite de bloques por cuadro
profile-area-threshold = Umbral de área
profile-area-threshold-sub = Fracción cambiada que fuerza un cuadro entero
profile-group-timing = Tiempo
profile-frame-timeout = Espera de fotograma
profile-frame-timeout-sub = en milisegundos
profile-focus-poll = Sondeo del enfoque
profile-pipeline-poll = Sondeo en inactividad
profile-daemon-poll = Sondeo del daemon
profile-seconds = en segundos
profile-group-recovery = Recuperación de errores
profile-capture-failures = Límite de fallos de captura
profile-capture-delay = Retardo tras un fallo
profile-swapchain-debounce = Espera de recreación del swapchain

## Restablecer (en la barra de cambios, para un perfil con nombre)
profile-reset-all-named = Quitar todos los ajustes del perfil

## Diálogos
profile-delete-title = Eliminar perfil
profile-delete-body = Se quita «{ $name }» de config.yaml. La copia de respaldo (config.yaml.bak1) guarda la versión anterior.
profile-delete-confirm = Eliminar
profile-cancel = Cancelar

## Nuevo perfil
new-profile-title = Nuevo perfil
new-profile-intro = Un perfil guarda los ajustes de un juego o programa. Se aplica a las ventanas que cumplen todas las reglas.
new-profile-close = Cerrar
new-profile-icon-capture = Capturar ícono desde una ventana
new-profile-icon-file = Cargar ícono desde un archivo
new-profile-icon-remove = Quitar ícono
new-profile-name-empty = Escribe un nombre
new-profile-name-taken = Ya hay un perfil con ese nombre
new-profile-mode = Modo de escalado
new-profile-copy-from = Copiar ajustes de
new-profile-rules = Reglas de coincidencia
new-profile-add-rule = Agregar regla
new-profile-remove-rule = Quitar regla
new-profile-rule-kind = Tipo de regla
new-profile-rule-value = Valor
new-profile-rules-help = Título: exacto, contiene o regex (sin distinguir mayúsculas) · Tamaño: 1920, >1280, <800
new-profile-rule-placeholder-size = ej., >800
new-profile-kind-title = Título exacto
new-profile-kind-title-contains = Título contiene
new-profile-kind-title-regex = Título (regex)
new-profile-kind-title-startswith = Título empieza con
new-profile-kind-title-endswith = Título termina con
new-profile-kind-width = Ancho
new-profile-kind-height = Alto
new-profile-matches = { $count ->
    [one] Coincide con 1 ventana abierta: { $first }
   *[other] Coincide con { $count } ventanas abiertas: { $first }…
}
new-profile-matches-none = No coincide con ninguna ventana abierta.
new-profile-no-rules = Sin reglas el perfil solo se aplica al elegirlo por nombre.
new-profile-create = Crear perfil
new-profile-capture = Capturar ventana
new-profile-capture-help = Elige una ventana para completar nombre, ícono y reglas. También puedes pulsar el botón y hacer clic en la ventana.
new-profile-point = Señalar ventana
new-profile-pointing = Haz clic en una ventana…
new-profile-no-windows = No hay ventanas abiertas.
new-profile-from-tile = Desde Inicio, «Crear perfil» en el menú de un mosaico abre este diálogo con la ventana ya capturada.

## Avisos (snackbar)
profile-toast-saved = Cambios guardados.
profile-toast-created = Perfil «{ $name }» creado.
profile-toast-duplicated = Perfil duplicado como «{ $name }».
profile-toast-deleted = Perfil «{ $name }» eliminado.
profile-toast-renamed = Perfil renombrado a «{ $name }».
profile-toast-setting = Se guardó el cambio de la barra en «{ $name }».
profile-toast-setting-default = Se guardó el cambio de la barra en Predeterminado.
profile-toast-menubar = Se guardó el recorte de la barra de menú en «{ $name }».
profile-toast-mode-created = Modo «{ $name }» creado.
profile-toast-mode-deleted = Modo «{ $name }» eliminado.
