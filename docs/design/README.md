# Rediseño de la GUI: sistema de diseño M3 Expressive

Propuesta para la GUI 2.0, pensada para la reescritura en Rust con Dioxus y Tailwind.

**Artefacto (lienzo de diseño):** https://claude.ai/artifact/19aBN9Lzfh8aibMbazSXRM

Tiene cuatro páginas: sistema de diseño (portada, color, tipografía, forma y movimiento,
componentes), pantallas (Inicio, Modos de escalado, Perfil, Nuevo perfil, Ajustes, superposición
durante el escalado, bandeja y Acerca de), flujos de UX y notas de implementación en Dioxus. Los
tokens y las clases de referencia están en [`lf.css`](lf.css), copia del archivo del lienzo.

## Decisiones principales

1. **Navegación al estilo de Magpie.** Un cajón lateral de 288 px con Inicio, Modos de escalado,
   Perfiles (Predeterminado, que reemplaza a «Global», un perfil por juego y «Nuevo perfil»),
   Acerca de y Ajustes. Desaparecen la columna de perfiles y el panel de nueve íconos.
2. **Inicio** pone primero los atajos de activación (pantalla completa `Alt+Shift+A`, ventana
   `Alt+Shift+Q`, escalado diferido de 3 s) y el modo daemon, y debajo la cuadrícula de ventanas
   con vistas previas en vivo, filtro de texto y filtro rápido (todas, con perfil, sin perfil).
   Una sesión activa se ve como tarjeta ámbar con Reiniciar y Detener.
3. **Modos de escalado como entidad propia.** Modelo SRCNN, escalado doble, muestreo y cadena de
   efectos (Nativo o MagpieFX, con «Medir en mi GPU») viven en un modo con nombre; cada perfil
   elige uno. Se guardarían en `modes/*.yaml`.
4. **Herencia visible.** Cada fila de un perfil dice si hereda de Predeterminado, si el perfil la
   cambia (franja primaria, botón para volver a heredar) o si tiene cambios sin guardar (franja
   ámbar). Guardar y Restablecer pasan a una barra flotante que sólo aparece con cambios; el menú
   de Restablecer conserva sus tres opciones.
5. **Pestañas del perfil:** Escalado (modo, enfoque, excluir del daemon), Presentación (con la
   generación de cuadros, marcada como ajuste global), Pantalla, Extras y Avanzado. Atajos, estilo,
   idioma, tema y bandeja van a Ajustes.
6. **Color:** semilla azul `#2F5F9E` y terciario ámbar para lo «vivo» (escalando, sin guardar,
   generación de cuadros). Oscuro por defecto y claro completo. Las 20 paletas actuales pasan a ser
   semillas del tema. La superposición usa un cristal siempre oscuro.
7. **Tipografía y forma:** Google Sans Flex (respaldo Roboto Flex), JetBrains Mono para fps y ms,
   Material Symbols Rounded. Escala de formas M3 con botones que se cuadran al pulsar y listas
   segmentadas; los resortes de `MotionScheme.expressive()`, con reducir movimiento respetado.
8. **Superposición durante el escalado:** barra flotante arriba (fijar, stats, recorte, captura,
   multiplicador, fps, ocultar, a ventana, detener), panel de stats y OSD. Se propone dibujarla en
   Vulkan, como el panel de stats actual, con constantes generadas de los mismos tokens, en lugar
   de una webview encima del juego.
9. **Implementación:** `tokens.json` → `assets/tokens.css` → `@theme inline` de Tailwind v4, como en
   Miko Kagura y Enigmata Temple. Bandeja con `tray-icon`, atajos con `global-hotkey` (portal en
   Wayland), traducciones con Fluent y vistas previas servidas por `use_asset_handler`.

## Confirmado

- **Contrastes (WCAG 2.1).** [`contrast.py`](contrast.py) mide los roles de `lf.css`: en cada tema
  pasan los 54 pares (texto e indicadores a 4,5:1, contornos a 3:1). El texto sobre el cristal de la
  superposición (al 86 %) también pasa, sobre fondo negro y blanco: 13,3 y 8,2 para el texto
  principal, 10,1 y 6,2 para el primario. Para repetirlo: `python3 docs/design/contrast.py docs/design/lf.css`.
- **Google Sans Flex.** Está en google/fonts bajo `ofl/googlesansflex` con la licencia OFL 1.1, sin
  nombre reservado: se puede incluir en el paquete. «Google Sans» y «Google Sans Flex» son marcas de
  Google (`TRADEMARKS.md`): se incluye el `.ttf` sin modificar y, si se recorta, la copia lleva otro
  nombre de familia. Roboto Flex queda como respaldo.
- **Material Symbols en la barra de Vulkan.** google/material-design-icons publica
  `variablefont/MaterialSymbolsRounded[FILL,GRAD,opsz,wght].ttf` y su `.codepoints` (Apache 2.0).
  `ab_glyph` no lee woff2 y tampoco aplica ligaduras, así que la barra busca cada ícono por su
  punto de código (desde `.codepoints`), no por su nombre. Conviene sacar dos instancias estáticas
  (FILL 0 y 1) con `fonttools varLib.instancer` y recortarlas a los íconos de la barra.
- **Cantidad de efectos.** El lienzo ya no fija un número: la GUI muestra la cantidad real del
  catálogo (hoy 117 incluidos, de los que 86 se verificaron), con «Sin probar» en los demás.

## Qué falta

- Generar `tokens.json` desde la semilla con `material-colors` y comparar con los valores del
  lienzo.
- Probar la barra en Vulkan con la tipografía real antes de fijar sus medidas.
