# Ronda 1.3: guion de pruebas final

Se corre entero, en orden, sobre `round13/integration` ya instalada con pipx
([plan](round-1.3-plan.md)). No se corrige nada durante la pasada: cada falla se anota en la
tabla de resultados del final y se corrige todo junto después. Después de corregir se repiten
las pruebas que fallaron y las que tocan lo mismo que la corrección.

Convenciones:

- **Video 30 / 60 / 24:** `testsrc2` de 1280×720 a esos fps en mpv
  (`ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=N -t 20`), ventana X11 en `+50+50`.
- **MangoHud:** `MANGOHUD=1 MANGOHUD_CONFIG=autostart_log=1,log_duration=8,log_interval=0`;
  se informan fps medios, mediana y desvío del tiempo entre cuadros.
- **Juego:** G-senjou no Maou, con la cadena de la persona (Anime4K_Upscale_Denoise_UL +
  Anime4K_Restore_Soft_UL + Lanczos), salida 2560×1440, `mailbox` salvo que se diga otra cosa.
- **GPU:** RX 9070 XT salvo que se diga otra cosa. Para elegir otra: `--gpu`.

## 0. Preparación

| ID | Paso | Esperado |
| --- | --- | --- |
| P0.1 | `df -h /tmp` | Más de 2 GB libres |
| P0.2 | Compilar las extensiones e instalar con pipx desde `round13/integration` | Sin errores; versión 1.3.0 |
| P0.3 | `unzip -l` de la rueda (o listar el venv de pipx) | Están los `.qm`, los 31 efectos nuevos con sus `.dds` y la extensión `lsfg` |
| P0.4 | `uv lock --check` | El lock coincide con `pyproject.toml` |

## 1. Automáticas

| ID | Paso | Esperado |
| --- | --- | --- |
| T1.1 | `pytest` | Todo pasa, sin saltos inesperados |
| T1.2 | Los tests nuevos de A, B y C aparecen en la salida | Se corrieron los de `.dds`, FP16, multiplicador automático, latencia, aviso de GPU y medición |
| T1.3 | Arrancar la GUI sin ventana (`QT_QPA_PLATFORM=offscreen`) en español | Sin cadenas en inglés en Presentación, Efectos, bandeja ni diálogo de medición |

## 2. MagpieFX (frente A), con video 30

| ID | Paso | Esperado |
| --- | --- | --- |
| T2.1 | Cargar uno por uno los 31 efectos nuevos sobre un cuadro de 1280×720 | Compilan y corren todos; tiempo de cada uno anotado |
| T2.2 | RAVU (un par de variantes), NIS y SMAA sobre el video | Imagen correcta, sin colores cambiados ni bordes rotos (captura) |
| T2.3 | Tu cadena UL en FP32 y en FP16 (panel de stats, GPU ms por efecto) | FP16 más rápido; sin diferencias visibles (captura de los dos) |
| T2.4 | Forzar FP32 en una GPU sin FP16 (o simulándolo) | Vuelve a 32 bits sin error |
| T2.5 | Un efecto con su `.dds` borrado | Error claro que nombra el archivo; el escalado sigue sin ese efecto |
| T2.6 | Regresión: los 86 efectos de antes siguen compilando (script de banco) | Mismos resultados que en 1.2 |

## 3. Vulkan y generación de cuadros (frente B), con video

| ID | Paso | Esperado |
| --- | --- | --- |
| T3.1 | Motor integrado ×2, ×3, ×4 con video 30, `mailbox` (MangoHud) | Igual que 1.2: 60 / 90 / 120 fps, desvío < 0,3 ms |
| T3.2 | Lo mismo en `fifo` | Mismas cadencias; el pipeline no pierde cuadros (panel: fps nuevos = 30) |
| T3.3 | ×2 con video 60, `mailbox` y `fifo` | ~120 fps en los dos |
| T3.4 | *Auto* con video 24, 30 y 60 (monitor de 144 Hz) | Elige el multiplicador esperado (p. ej. ×5 o ×4, ×4, ×2) y no salta entre valores; el cambio no corta la imagen |
| T3.5 | *Auto* con video que cambia de fps a mitad (concatenar 30 y 60) | Cambia una vez, con la histéresis prevista |
| T3.6 | Latencia (`--latency-log`): sin FG, capa ×2, integrado ×2, con video 30 | CSV completo; latencia del integrado ≈ un intervalo del juego más la de sin FG; anotar las tres |
| T3.7 | Aviso de GPU con video 24 y efectos de ~35 ms que alcanzan | No avisa |
| T3.8 | Aviso de GPU con video 60 y la cadena UL | Avisa en rojo, como en 1.2 |
| T3.9 | `robustImageAccess`: log de arranque | Activado en RADV; arranca igual donde no está |
| T3.10 | Motor integrado en la GPU integrada del Ryzen (`--gpu`), ×2 con video 30 | Funciona o falla con un mensaje claro y sigue sin FG |
| T3.11 | Motor integrado en lavapipe (`VK_ICD_FILENAMES` de lvp) | Igual que T3.10; anotar el resultado |
| T3.12 | Regresión: capa lsfg-vk ×2 con video 30 | 33 fps presentados por lirtup y la capa activa, como en 1.2 |

## 4. Medir en mi GPU (frente C)

| ID | Paso | Esperado |
| --- | --- | --- |
| T4.1 | Abrir *Medir en mi GPU* desde la pestaña de efectos y medir todo | Progreso visible; termina; resultados por efecto |
| T4.2 | Comparar con T2.1 y T2.3 | Diferencias menores al 10 % |
| T4.3 | Medición de generación de cuadros desde la sección de Presentación | Tiempos de lsfg ×2 a ×4, flujo 1,0 / 0,5, Calidad / Rendimiento |
| T4.4 | Cerrar y reabrir la app | Los resultados guardados se muestran en lugar de los de referencia |
| T4.5 | Cancelar a mitad de una medición | Se cancela sin colgar la GUI |
| T4.6 | Medir con una sesión de escalado activa | Lo impide o avisa, sin romper la sesión |

## 5. GUI en la app real

| ID | Paso | Esperado |
| --- | --- | --- |
| T5.1 | Abrir la GUI | En español; selector de motor de efectos y de generación de cuadros funcionando |
| T5.2 | Presentación: slider de flujo y selector de modelo | El texto cambia por tramo; guardar y reiniciar aplica los valores |
| T5.3 | Bandeja: *Generación de cuadros* con *Auto* y ×2 a ×5 | Reinicia y retoma la ventana |
| T5.4 | Regresión de la bandeja: reiniciar, cambiar a otra ventana, cambiar perfil | Como en 1.2 |
| T5.5 | Sin `glslangValidator` en el PATH, cargar un Anime4K_Denoise_Bilateral | Error claro; el resto de la cadena sigue |

## 6. Con el juego (necesita que juegues)

Grabaciones de 90 s jugando (texto, transiciones, animaciones), con el panel de stats visible.

| ID | Paso | Esperado |
| --- | --- | --- |
| T6.1 | Barra de menú en un perfil nuevo | Recorta `T21 L2 R2 B2`, 1280×720; queda guardado en el perfil |
| T6.2 | Integrado ×2, flujo 1,0, Calidad, UL en FP32 | Línea base (1.2: ~33 fps mostrados) |
| T6.3 | Lo mismo con UL en FP16 | Más fps que T6.2 |
| T6.4 | FP16 + flujo 0,5 | Más fps que T6.3; anotar si se ven defectos |
| T6.5 | FP16 + flujo 0,5 + Rendimiento | Más fps que T6.4; anotar defectos |
| T6.6 | La mejor de T6.3 a T6.5 con *Auto* | Elige un multiplicador coherente con los fps reales |
| T6.7 | Latencia de T6.2 y de la mejor configuración | Anotada |
| T6.8 | Regresión: capa lsfg-vk ×4 con la cadena UL | Como la prueba de la persona en 1.2 |
| T6.9 | Impresión a ojo de la mejor configuración frente a la capa | Anotada por la persona |

## Resultados

Una fila por prueba que falle o deje dudas. Las que pasan solo se marcan en las tablas de
arriba.

| ID | Resultado | Falla / observación | Causa | Corrección | Repetida |
| --- | --- | --- | --- | --- | --- |
| | | | | | |

## Pasada de correcciones

1. Agrupar las fallas por frente y por archivo.
2. Corregir todo en `round13/integration`, un commit por causa.
3. Repetir las pruebas que fallaron y las de la misma sección que la corrección pueda afectar.
4. Anotar en la columna *Repetida* el resultado nuevo.
5. Cuando la tabla no tenga fallas abiertas: PR a `main`.
