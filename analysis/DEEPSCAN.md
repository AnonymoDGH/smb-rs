# deepscan — escáner profundo de rutas

> **Español** (este archivo) | [English](DEEPSCAN.en.md)

`deepscan` recorre **Super Mario Bros. 1-1** con el emulador real, cartografía el nivel a partir de la Block Buffer (`$0500-$069F`) y busca una ruta rápida con rollouts deterministas. No usa datos de nivel copiados ni un clon del juego: todo sale de la ROM ejecutándose instrucción por instrucción.

---

## Pipeline

| Fase | Módulo | Qué hace |
|---|---|---|
| 1. Boot | `route::beam::boot_to_level` | Power-on → título → 1-1. Devuelve un `ConsoleSnapshot` y la cinta de START. |
| 2. Mapa | `route::map` | Fusiona la ventana visible de 16 columnas × 13 filas en un mapa absoluto. Registra enemigos (spawn por slot) y la columna de la bandera. |
| 3. Física | `route::physics` | Modelo de salto/gravedad con las constantes de la ROM (`JumpMForceData`, `FallMForceData`, `PlayerYSpdData`). |
| 4. Plan | `route::planner` | Descompone el mapa en obstáculos y elige despegue/hold para cada uno. |
| 5. Trucos | `route::tricks` | Pruebas empíricas de bugs/trucos y detector de trucos en una ruta. |
| 6. Búsqueda | `route::beam` | Beam search sobre snapshots reales del emulador. |
| 7. Replay | `deepscan` | Re-ejecuta la ruta desde power-on para verificar determinismo y generar telemetría. |

La pieza clave es el **snapshot determinista** (`ConsoleSnapshot`): CPU + RAM + PPU + APU, sin framebuffer ni buffer de audio. Restaurar un snapshot y repetir una secuencia de botones produce exactamente el mismo estado (lo comprueba `--mode selftest`).

---

## Uso

```powershell
cargo build --release

# 1. Determinismo del snapshot (2 tests)
.\target\release\deepscan.exe "ROM.nes" out_deep --mode selftest

# 2. Cartografía 1-1 con el piloto base
.\target\release\deepscan.exe "ROM.nes" out_deep --mode map --frames 4000

# 3. Plan heurístico + ejecución cerrada
.\target\release\deepscan.exe "ROM.nes" out_deep --mode plan

# 4. Búsqueda global con rollouts reales
.\target\release\deepscan.exe "ROM.nes" out_deep --mode global --beam 8 --frames 1800 --stride 8 --max-rollout 40 --time-weight 2.5

# 5. Probe empírico de trucos (arranque, low-gravity, fricción inversa)
.\target\release\deepscan.exe "ROM.nes" out_deep --mode probe

# 6. Replay determinista de un route_script.txt ya guardado
.\target\release\deepscan.exe "ROM.nes" out_deep --mode replay --script out_deep\route_script.txt

# 7. Todo + reporte
.\target\release\deepscan.exe "ROM.nes" out_deep --mode all --beam 12 --frames 2000
```

### Parámetros

| Flag | Default | Significado |
|---|---|---|
| `--mode` | `all` | `selftest`, `map`, `plan`, `global`, `probe`, `replay`, `all` |
| `--frames` | 4000 | Presupuesto de frames de nivel |
| `--beam` | 16 | Anchura del haz (nodos conservados por paso) |
| `--stride` | 6 | Frames por macro-acción de correr/esperar/frenar |
| `--goal-x` | 3160 | X de la bandera |
| `--max-rollout` | 60 | Frames máximos por rollout |
| `--time-weight` | 2.5 | Peso del tiempo en el score (= velocidad máxima, ver abajo) |
| `--script` | — | Script de ruta para `--mode replay` |
| `--quiet` | — | Sin trazas de progreso por stderr |

---

## Salidas

| Archivo | Contenido |
|---|---|
| `level_map.json` | Mapa de 13 filas × N columnas, enemigos, obstáculos y bandera. |
| `level_map.svg` | Mapa visual con enemigos y bandera. |
| `level_map.txt` | Mapa ASCII con regla de decenas. |
| `route_plan.json` | Plan heurístico en JSON. |
| `route_plan.md` | Tabla de maniobras y notas del planificador. |
| `route_plan.svg` | Mapa con marcadores de salto. |
| `plan_execution.csv` | Telemetría de la ejecución del plan (fase, x, y, vx, estado, vidas). |
| `route.json` | Configuración y resultado de la búsqueda global. |
| `route_script.txt` | Cinta `frame:boton:valor` compatible con `nesdump --script`. |
| `route_trace.csv` | Evolución del haz (mejor nodo por paso). |
| `route_telemetry.csv` | Replay determinista de la ruta encontrada. |
| `deepscan_report.md` | Reporte legible con todas las fases. |
| `selftest.txt` | Resultado de los tests de determinismo. |
| `trick_probe.md` | Tabla de pruebas de arranque y low-gravity (`--mode probe`). |
| `trick_report.md` | Detector de trucos sobre la ruta: velocidad máxima, fricción inversa, moonwalk, flagpole. |

---

## Acciones del beam

En el suelo:

- `Jump(hold)` con `hold` ∈ {8, 14, 20, 26}: corre y salta hacia delante.
- `JumpUp(hold)` ∈ {20, 26}: salto vertical y avance al final; sirve para escalones justo encima.
- `BackJump(back, hold)` con `back` ∈ {14, 28}: retrocede, coge carrerilla y salta; sirve para muros pegados.
- `Run(stride)`, `Wait(stride)`, `Brake(stride)`, `Moonwalk(stride)`.
- `AccelStart` (solo en el nodo raíz): `L+R` 1f, `A` 1f, carrera 22f, giro 1f sin B y salto; la aceleración inversa medida en `--mode probe`.

En el aire: `Run`, `Wait`, `Brake`.

`Run` se corta en el **punto de despegue seguro** del siguiente obstáculo (sensores de Block Buffer: muro/tubería, pozo, borde con caída). Sin esto el beam solo salta en múltiplos del stride y pierde velocidad contra tuberías.

Score de un nodo:

```text
x - time_weight * frame + 0.4 * vx + 2.0 * on_ground
```

Con `time_weight=2.5` (= velocidad máxima) el score equivale a una estimación de **tiempo total**: `frame + (goal_x - x) / vx_max`. El desempate final entre rutas que llegan a la bandera lo hace `best_goal` por número de frames. La deduplicación usa bloques de posición/velocidad, excepto cerca del mástil donde se conserva el X exacto porque la colisión del flagpole depende del nybble bajo de X (ventana 6..9). El objetivo también se dispara al registrarse la colisión del mástil (`$070F`, `$010F`, `Player_State=3`), no solo al superar `goal_x`.

---

## Puntos de extensión

1. **A\* sobre el mapa** — sustituir `plan_route` por A\* con estado `(columna, fila de apoyo, vx)` y `physics::jump_arc` como modelo de transición.
2. **Paralelizar rollouts** — cada `Console` es independiente; `std::thread` + canales puede reducir el tiempo del beam casi linealmente.
3. **Enemigos en el beam** — añadir los 5 slots (`$000F..$0013`) al estado y una acción `WaitForEnemy` / `Stomp`.
4. **Niveles verticales** — compensar `ScreenY_Pos` en `LevelMap::ingest` (1-2, 4-2, 8-4).
5. **Score multiobjetivo** — mantener un frente de Pareto `(frames, x)` en vez de un único escalar.
6. **Podar con el plan** — usar `plan_route` como prior para limitar las acciones del beam a las zonas de salto.
7. **Guardar/reanudar** — serializar `ConsoleSnapshot` a disco para continuar una búsqueda larga.
8. **Trucos avanzados** — `AccelStart`, moonwalk, walljump y FPG/FFPG ya tienen primitivas o detectores en `route::tricks`; falta añadir walljump como acción y medir el FPG como categoría de nivel completo (hasta el castillo), no como toque del mástil.

---

## Limitaciones

- El mapa asume scroll horizontal; los niveles verticales necesitan compensación de cámara Y.
- El ejecutor del plan es cerrado (detecta atascos, reintenta saltos y cae al piloto), pero no es un controlador óptimo.
- El beam es costoso: ~30-40 rollouts/s en este equipo. `--beam` y `--frames` son el presupuesto real.
- `route_script.txt` incluye los frames de boot (START) y se reproduce con `nesdump --script`.
- La detección de muerte usa `Player_Y_HighPos` (`$0B`), no el byte bajo de Y, porque un salto alto puede dar `y=255` estando por encima de la pantalla.

---

## Resultados 1-1 (ROM World)

ROM SHA-256: `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de`.

| Método | Frames de nivel | Tiempo (60.0988 fps) | X final | Bandera |
|---|---|---|---|---|
| Piloto base (`--mode map`) | 1511 | 25.15 s | 3161 | sí |
| Plan heurístico (`--mode plan`) | 2134 | 35.51 s | 3161 | sí |
| Beam (`--time-weight 0.7`) | 1426 | 23.73 s | 3161 | sí |
| Beam (`--time-weight 2.5`) | **1266** | **21.07 s** | **3161** | **sí** |

**Récord de 1-1 (21.43 s ≈ 1288 frames): batido por 22 frames (0.37 s).** El tiempo termina en el frame de contacto con el mástil —ese es el que mide el récord—, así que el flagpole glitch (FPG/FFPG), que solo acelera la animación posterior, no mejora esta marca. Ver sección 32 de [`BUGS_Y_TRUCOS.md`](BUGS_Y_TRUCOS.md) para el análisis de trucos (aceleración inversa +11 px, low-gravity sin ventaja, walljump y moonwalk sin uso en 1-1).

Configuración exacta del resultado más rápido:

```text
beam=8  stride=8  max_rollout=40  time_weight=2.5  frames=1800
rollouts=9154  nodos=880  boot=240f  nivel=1266f  total=1506f
```

Artefactos de esa ejecución en [`analysis/deepscan/`](deepscan/).

---

## Reproducir una ruta

```powershell
# route_script.txt ya contiene el boot + nivel
.\target\release\nesdump.exe "ROM.nes" out_replay --frames 2100 --scale 2 --script (Get-Content out_deep\route_script.txt -Raw)
```

Para comparar, `route_telemetry.csv` es el replay que hace el propio `deepscan` desde power-on; si la última fila coincide con `route.json` (`x`, `y`, `frames`), el determinismo está verificado.
