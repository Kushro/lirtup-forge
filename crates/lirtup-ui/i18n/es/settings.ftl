# settings (frente D).
settings-title = Ajustes
settings-tab-general = General
settings-tab-hotkeys = Atajos
settings-tab-style = Estilo

## General
settings-appearance = Apariencia
settings-language = Idioma
settings-language-help = Se aplica al instante, sin reiniciar
settings-language-system = Sistema
settings-theme = Tema
settings-theme-help = Claro, oscuro o el del escritorio
settings-theme-system = Sistema
settings-theme-light = Claro
settings-theme-dark = Oscuro
settings-seed = Color
settings-seed-help = Semilla del tema; las paletas de antes quedan como semillas
settings-tray = Bandeja del sistema
settings-show-tray-icon = Mostrar ícono en la bandeja
settings-show-tray-icon-help = Sin el ícono, cerrar la ventana sale de LirtUp Forge
settings-close-to-tray = Cerrar a la bandeja
settings-close-to-tray-help = El botón cerrar oculta la ventana en vez de salir
settings-minimize-to-tray = Minimizar a la bandeja
settings-minimize-to-tray-help = El botón minimizar oculta la ventana en la bandeja, no en la barra de tareas
settings-start-hidden = Empezar oculto
settings-start-hidden-help = Al abrir, queda solo el ícono de la bandeja
settings-auto = Escalado automático
settings-daemon = Modo daemon
settings-daemon-help = Un proceso en segundo plano escala las ventanas que coinciden con un perfil
settings-daemon-watching = { $count ->
    [0] Activo, sin perfiles con reglas que vigilar
    [one] Activo, vigilando 1 perfil
   *[other] Activo, vigilando { $count } perfiles
}
settings-keep-running = Seguir ejecutándose tras salir
settings-keep-running-help = El daemon sigue activo aunque cierres LirtUp Forge

## Bandeja: menú, avisos y salir (ronda 4, frente Y)
tray-idle = En reposo
tray-scaling = Escalando: { $title }
tray-starting = Preparando: { $title }
tray-failed = Error al escalar
tray-watching = { $count ->
    [0] Daemon activo, sin perfiles que vigilar
    [one] Daemon vigilando 1 perfil
   *[other] Daemon vigilando { $count } perfiles
}
tray-stop = Detener
tray-restart = Reiniciar
tray-profile = Perfil
tray-profile-of = Perfil: { $name }
tray-framegen = Generación de cuadros: { $value }
tray-framegen-note = Elegir una reinicia la sesión
tray-daemon = Modo daemon
tray-show = Mostrar LirtUp Forge
tray-quit = Salir
notify-scaling = Escalando: «{ $title }». Utilice «Detener» en el menú de la bandeja para volver.
notify-failed = No se pudo escalar: { $error }
notify-stop = Detener
quit-body = Tienes cambios sin guardar. Guárdalos antes de salir, descártalos, o cancela para seguir en LirtUp Forge.
quit-save = Guardar y salir
quit-discard = Descartar y salir

## Atajos
settings-hotkeys-help = Pulsa «Cambiar atajo» y luego la combinación; necesita al menos un modificador. Esc cancela, Retroceso lo deja sin asignar. Se guarda al instante.
settings-hotkeys-reset-all = Restablecer todos
settings-hotkeys-activation = Activación
settings-hotkeys-session = Sesión
settings-hotkeys-view = Vista
settings-hotkeys-zoom = Zoom y desplazamiento
settings-hotkey-default = Predeterminado: { $keys }
settings-hotkey-conflict = También lo usa: { $actions }
settings-hotkey-start_fullscreen = Escalar en pantalla completa
settings-hotkey-start_windowed = Escalar en ventana
settings-hotkey-toggle_scaling = Mostrar u ocultar el escalado
settings-hotkey-exit_app = Terminar la sesión
settings-hotkey-screenshot = Capturar pantalla
settings-hotkey-toggle_stats = Mostrar estadísticas
settings-hotkey-toggle_toolbar = Mostrar u ocultar la barra
settings-hotkey-cycle_model = Cambiar modelo
settings-hotkey-cycle_geometry = Cambiar geometría
settings-hotkey-restore_view = Restaurar vista
settings-hotkey-zoom_in = Acercar
settings-hotkey-zoom_out = Alejar
settings-hotkey-offset_up = Mover el contenido hacia arriba
settings-hotkey-offset_down = Mover el contenido hacia abajo
settings-hotkey-offset_left = Mover el contenido a la izquierda
settings-hotkey-offset_right = Mover el contenido a la derecha

## Estilo
settings-zoom = Zoom
settings-zoom-help = Tamaño de toda la interfaz
settings-font-scale = Escala de fuente
settings-font-scale-help = Solo el texto
settings-font = Fuente
settings-font-help = Vacía: Google Sans Flex, la incluida
settings-grid = Cuadrícula de ventanas
settings-aspect = Forma de los mosaicos
settings-aspect-help = Auto sigue la forma de cada ventana
settings-aspect-auto = Auto
settings-columns = Columnas
settings-columns-help = Auto llena el ancho disponible
settings-columns-auto = Auto
settings-intervals = Actualización y vista previa
settings-intervals-note = lista cada { $list } ms · vista previa cada { $preview } ms
settings-refresh = Lista de ventanas
settings-preview = Vistas previas
settings-preview-help = Más seguido gasta más CPU
settings-drawer-note = Los anchos de las barras laterales ya no se ajustan: el cajón mide 288 px y se contrae a un riel de 80 px por debajo de 1100 px de ancho.
