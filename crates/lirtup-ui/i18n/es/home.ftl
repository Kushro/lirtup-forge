# home (frente B): Inicio.
home-title = Inicio

## Tarjeta de sesión
home-session-starting = Iniciando { $title }…
home-session-starting-aria = Iniciando la sesión
home-session-running = Escalando { $title }
home-session-restart = Reiniciar
home-session-stop = Detener
home-session-stop-hint = Detener el escalado
home-session-gpu-warning = La GPU no llega: { $gpu } ms por cuadro de { $budget } ms disponibles
home-session-failed = No se pudo escalar: { $stage }
home-session-retry = Reintentar
home-stage-config = error al leer la configuración
home-stage-window = no se encontró la ventana
home-stage-device = error de la GPU o de Vulkan
home-stage-capture = error de captura
home-stage-effects = error al preparar los efectos
home-stage-overlay = error de la ventana de salida
home-stage-swapchain = error de presentación
home-stage-framegen = error de la generación de cuadros
home-stage-runtime = error durante el escalado

## Activación
home-activation-title = Activación
home-activation-help = Pulsa un atajo para escalar o dejar de escalar la ventana con foco, o elige una ventana abajo.
home-activation-fullscreen = Escalar en pantalla completa
home-activation-fullscreen-subtitle = La ventana con foco, con el modo de su perfil
home-activation-windowed = Escalar en ventana
home-activation-windowed-subtitle = Siempre encima, se puede mover desde la barra de herramientas
home-activation-edit-hotkey = Cambiar atajo
home-activation-no-hotkey = Sin atajo
home-delay-title = Escalado diferido
home-delay-subtitle = Escala la ventana con foco después de { $seconds } s: tiempo para volver al juego
home-delay-start = Empezar
home-delay-start-fullscreen = Empezar en pantalla completa
home-delay-start-windowed = Empezar en ventana
home-delay-choose = Elegir demora
home-delay-option = { $seconds } s
home-daemon-title = Modo daemon
home-daemon-subtitle = Escala solo las ventanas que coinciden con un perfil, aunque cierres esta ventana
home-daemon-watching = { $count ->
    [one] Vigilando { $count } perfil
   *[other] Vigilando { $count } perfiles
}

## Ventanas abiertas
home-windows-title = Ventanas abiertas
home-filter-label = Filtro rápido
home-filter-all = Todas
home-filter-with-profile = Con perfil
home-filter-without-profile = Sin perfil
home-search-label = Filtrar ventanas
home-search-placeholder = Filtrar ventanas por título o clase…
home-windows-empty = No hay ventanas abiertas para escalar.
home-windows-no-match = Ninguna ventana coincide con el filtro.
home-windows-hint = Clic: escalar con el modo del perfil que coincide. Clic derecho o ⋮: escalar en ventana, crear perfil desde esta ventana, copiar título. Las vistas previas se refrescan cada { $ms } ms y se pausan con la app en segundo plano.
home-tile-menu-windowed = Escalar en ventana
home-tile-menu-profile = Crear perfil desde esta ventana
home-tile-menu-copy = Copiar título
