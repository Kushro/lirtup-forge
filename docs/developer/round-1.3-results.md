# Resultados ronda 1.3

| ID | Resultado | Falla / observación |
| --- | --- | --- |
| P0.1-P0.4 | OK | 14 GB libres; 1.3.0; .qm, 17 .dds, lsfg; lock coincide |
| T1.1 | FALLA | 234/235. `test_upload_shader_writes_the_texture_format[R16G16B16A16_FLOAT]`: el test parsea mal `RWTexture2D<float4>` (KeyError). Bug del test |
| T1.2 | OK | corrieron los tests nuevos de A, B y C |
| T1.3 | OK | 1290 textos de la GUI (9 pestañas, diálogo de medición, bandeja), ninguno en inglés |
| T2.1 | OK | los 31 nuevos corren (0,05 a 0,63 ms a 1280x720) |
| T2.2 | OK | RAVU_R3, RAVU_Zoom_R2_RGB, NIS, SMAA_High: colores y bordes iguales a sin efecto |
| T2.3 | OK (obs.) | FP16 activo en la cadena UL; imagen igual (diff máx 2/255). Pero solo ~3 % más rápido en la app (23,9 vs 24,5 ms; Upscale_Denoise_UL un poco más lento en FP16). En el banco 6-12 % |
| T2.4 | OK | UPSCALER_MAGPIEFX_FP16=0 compila y corre en FP32 sin error |
| T2.5 | OK | error claro con el nombre del .dds; RAVU_R2 desactivado, Lanczos sigue |
| T2.6 | OK | 86/86 de antes corren |
| T3.1 | OK | mailbox x2/x3/x4: 60,0/90,0/120,0 fps, desvío 0,14/0,24/0,10 ms |
| T3.2 | OK | fifo x2/x3/x4: 60,0/90,0/120,0 fps, desvío 0,08-0,10 ms; sin FG capta los 30 fps |
| T3.3 | OK | video 60 x2: 118,6 fps mailbox, 119,6 fifo |
| T3.4 | OK | Auto: 24->x5, 30->x4, 60->x2 (120 fps en los tres), sin saltos |
| T3.5 | OK (obs.) | 30->60 fps: un solo cambio x4->x2 ~2 s después; sin huecos; después 60 reales + 60 generados por s. Obs.: MangoHud deja de registrar tras el reinicio de lsfg (se engancha también al dispositivo de lsfg); el CSV de latencia confirma que la app sigue |
| T3.6 | OK | latencia hasta presentar: sin FG 3,3 ms; capa x2 3,4 ms (+ lo de la capa); integrado x2 reales 32,4 ms y generados 15,7 ms |
| T3.7 | SIN DATO | no se logró una cadena estable entre 33,3 y 41,7 ms (los tiempos de GPU varían entre corridas); repetir tras corregir T3.8 |
| T3.8 | FALLA | video 60 + cadena UL (34,1 ms), 19 de 60 fps captados: NO avisa (en 1.2 sí). También con 67,7 ms y 8 de 20 fps. Causa: `frames_lost` mira sondeos vacíos por cuadro nuevo, pero fuera de "solo cuadros nuevos" el bucle re-presenta y sondea ~150/s entre renders, así que nunca detecta cuadros perdidos y el presupuesto queda en el intervalo medido |
| T3.9 | OK | log: `Vulkan robustImageAccess: on` (RADV) |
| T3.10 | OK | iGPU Raphael: integrado x2 con FP16 funciona; 18 reales + 18 generados por s (la iGPU no da para 30) |
| T3.11 | SIN DATO | lavapipe no está instalado (falta el ICD lvp); sin driver la app termina con error claro |
| T3.12 | OK | capa x2: carga el perfil "LirtUp Forge" y lirtup presenta ~30/s (de T3.6) |
| T4.1 | OK | diálogo real: 132 casos (117 MagpieFX, 3 nativos, 12 lsfg) en 11 s, 0 fallidos, guardado |
| T4.2 | FALLA (parcial) | vs scripts/magpiefx-bench.py: Restore_Soft_UL -1 %, RAVU_R3 -3 % OK; Upscale_Denoise_UL -16 %, NIS +21 %, SMAA_High +32 % (SMAA: el diálogo cuenta el pase SwapRB de entrada). Investigar Upscale_Denoise_UL y NIS |
| T4.3 | OK (obs.) | 12 casos de lsfg sin fallas (12 init/finalize seguidos). x2 1,0 Calidad 1,59 ms; flujo 0,5 ~1/3; Rendimiento más barato. Obs.: x4 (2,37) < x3 (2,53), ruido de medir con CPU |
| T4.4 | OK | bench.json guardado; en un proceso nuevo la tabla de Efectos muestra lo medido y Presentación 'Measured: 1.6 ms per real frame' |
| T4.5 | OK | cancelado en el caso 47 de 132: tarda 0,1 s, guarda lo medido, no deja procesos |
| T4.6 | OK | con sesión: Iniciar deshabilitado con aviso; al terminar la sesión se habilita en ~1 s |
| T5.1 | OK | GUI real en español (incluye Guardar/Restablecer y secciones antes en inglés); sin errores |
| T5.2 | OK | flujo 0,5 + Rendimiento se guardan; un proceso nuevo arranca con flow 0.5 y performance |
| T5.3 | OK | bandeja Auto: guarda 'auto', se relanza (mismo PID), retoma la ventana; elige x4 con video 30 |
| T5.4 | OK (obs.) | reiniciar, cambiar ventana y perfil como en 1.2; Auto deshabilitado con la capa y su nota. Obs.: al cerrar el script de prueba, traza en gui/grid ('WindowTileItem already deleted') durante el desmontaje |
| T5.5 | FALLA (menor) | sin glslang: Bilateral se desactiva y la cadena sigue, pero el error solo muestra los errores de dxc ('variable length arrays...') sin decir que hace falta glslangValidator |
| T6.1 | OK | perfil nuevo: recorta T21 L2 R2 B2 (1280x720), lo guarda con la detección apagada; la 2.a sesión arranca recortada |
| T6.2 | OK (base) | integrado x2, flujo 1,0, Calidad, FP32: 14,8 reales + 14,7 generados = 29,5 mostrados/s; ritmo mediana 29,9 ms, desvío 3,7 |
| T6.3 | FALLA (esperado no cumplido) | FP16: 29,2 mostrados/s, igual que FP32 (no más fps). Coincide con T2.3: FP16 no alivia la cadena UL en esta GPU |
| T6.4 | OK | FP16 + flujo 0,5: 18,9 reales -> 37,7 mostrados/s (+28 %); ritmo muy parejo (desvío 0,72 ms) |
| T6.5 | OK | + Rendimiento: 20,0 reales -> 40,1 mostrados/s (+36 % sobre la base); desvío 1,0 ms |
| T6.6 | SIN DATO (obs.) | el juego estuvo quieto (2 cuadros nuevos/s). Obs.: Auto eligió x5 con 2 fps (el arranque no aplica el mínimo de 5 fps) |
| T6.7 | OK (obs.) | latencia hasta presentar, integrado: base 118 ms (reales) y 89 ms (generados); flujo 0,5 + Rendimiento 99 / 74 ms. Sin FG con la cadena UL ~31 ms (corridas inválidas); capa 28,7 ms (+ lo de la capa). Obs.: el integrado suma mucha latencia con la cadena UL |
| T6.8 | OK | capa x4 + cadena UL: lirtup presenta 22,4/s (la capa multiplica después); latencia 28,7 ms |
| Obs. | — | Con la cadena UL, el motor integrado baja los cuadros reales de ~22,5/s (sin FG) a 14,8/s con flujo 1,0: lsfg compite por la GPU con los efectos. En reposo la medición de lsfg da solo 1,6 ms (T4.3) |

## Pasada de correcciones

| ID | Causa | Corrección | Repetida |
| --- | --- | --- | --- |
| T1.1 | El test separaba mal el tipo de `RWTexture2D<float4>` | Lee el tipo entre `<>` (`b6a62fb7`) | OK: 231/231 |
| T3.8 | Con efectos que saturan la GPU el juego también se frena, y deja consultas vacías: parecía contenido lento que alcanza. Tampoco sirvió mirar la primera consulta después de cada render | Vuelve el piso de 30 fps de la 1.2 (`92d56ade`) | OK: video 60 + UL avisa "34.1 ms > 33.3 ms/frame" |
| T3.7 | Depende de T3.8: con el piso, contenido de 24 fps con ~35 ms avisa por diseño | Guion actualizado; cubierto por test unitario | OK (por diseño) |
| T4.2 | No era un error: el diálogo usa su imagen de prueba (con detalle) y el script recibió barras lisas; SMAA y NIS cuestan más con más bordes. Upscale_Denoise_UL -16 % fue una corrida con otro reloj de GPU | Ninguna | OK: con la misma imagen difieren < 1,5 % (NIS 0,191/0,190, SMAA 0,091/0,092, RAVU 0,148/0,146, UL 1,603/1,598) |
| T5.5 | El error solo mostraba los diagnósticos de dxc | Avisa primero que hace falta glslangValidator (`d6f0c576`) | OK |
| T6.6 | La primera elección de Auto no respetaba el mínimo de 5 fps | Con imagen quieta arranca en x2 (`abee4a55`) | OK con el juego en movimiento: elige x5 (15-18 reales/s a 144 Hz) y no cambia en 60 s, aunque el juego alterna texto quieto y animación; ~85 mostrados/s (17 reales + 68 generados) |
| Obs. | El integrado le quita GPU a efectos pesados | Nota en la ayuda del motor, README y análisis | — |
