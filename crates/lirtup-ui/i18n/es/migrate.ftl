# migrate (ronda 4, frente M): «Pasar a modos de escalado», la migración de la 1.x.
migrate-title = Pasar a modos de escalado
migrate-body = { $modes ->
    [0] Tu configuración de la 1.x guarda el modelo, el muestreo y los efectos sueltos en cada perfil. Ahora viven en modos de escalado con nombre que los perfiles eligen. Se { $fields ->
        [one] mueve { $fields } ajuste
       *[other] mueven { $fields } ajustes
    } a modos que ya existen:
    [one] Tu configuración de la 1.x guarda el modelo, el muestreo y los efectos sueltos en cada perfil. Ahora viven en modos de escalado con nombre que los perfiles eligen. Se { $fields ->
        [one] mueve { $fields } ajuste
       *[other] mueven { $fields } ajustes
    } a { $modes } modo nuevo:
   *[other] Tu configuración de la 1.x guarda el modelo, el muestreo y los efectos sueltos en cada perfil. Ahora viven en modos de escalado con nombre que los perfiles eligen. Se { $fields ->
        [one] mueve { $fields } ajuste
       *[other] mueven { $fields } ajustes
    } a { $modes } modos nuevos:
}
migrate-new = nuevo
migrate-inherit = { $count ->
    [one] { $profiles } queda con el modo de Predeterminado.
   *[other] { $profiles } quedan con el modo de Predeterminado.
}
migrate-backup = Antes se copia la configuración actual a { $file }. El escalado queda igual que antes.
migrate-legacy-gui = También pasan a Ajustes la bandeja y la paleta de la 1.x.
migrate-later = Ahora no
migrate-confirm = Migrar
migrate-done = Configuración pasada a modos. Copia de la anterior en { $backup }
migrate-failed = No se pudo migrar: { $error }
