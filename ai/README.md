# IA — Red neuroevolutiva (train / watchall)

> [English](README.md) | **Español** (este archivo)

Entrenador de alto rendimiento en `src/bin/train.rs` (~1200 líneas) + visor multicuadrícula `src/bin/watchall.rs`.

## Arquitectura (en `train.rs`)

```
120 sensores → 48 ocultos (ReLU) → 7 acciones
6151 parámetros totales (120·48+48 + 48·7+7)
```

**120 sensores** (`extract()`):

- Posición/velocidad: `x = $6D*256+$86`, `ypix=$03B8`, `vx=$57/16`, `vy=$9F`, `on_ground = $1D==0`, facing
- Mundo (Block Buffer `$0500-$069F`): `world_tile_kind()` → 0 aire / 1 sólido / 2 moneda. Rejilla 4×10 delante (filas `mty-2..mty+1`)
- Muros: `wall_dist` (0..96px) + `wall_height` (hasta 80px) vía `row_wall_low/mid`
- Pozos: `gap_dist` + `gap_width` (`is_pit_column` = sin sólido en 208/224)
- Enemigos (5 slots `$000F`): `ewx=$6E*256+$87`, `ewy=$CF`, `evx=$58`; `enemy_dist`, `enemy_height`
- HUD BCD: score `$07DE`, coins `$07ED`, timer `$07F8` (BCD a decimal)
- Progreso: `x/WORLD_W` (`WORLD_W=212*16=3392`), `steps/MAX_FRAMES`, `og_avg`

**7 acciones** (`ACT_BUTTONS`): `0, R, R+A, R+B, R+A+B, A, L`.

**Variante Colab GRRN-v5** (`watchall.rs`): `159 → 128 → 128 → 39` (entrada 120 + memoria 32 + 7 flags, recurrente). Constante `NW_V5` lista en `watchall.rs`.

## Pipeline de entrenamiento

```powershell
cargo build --release --bin train --bin watchall

# 1) Entrenar 300 generaciones x 64 individuos, frame-skip 4
.\target\release\train.exe train --gens 300 --pop 64 --skip 4 --seed-file ai\models\best_real.json --out ai\models\best_ai.json

# 2) Evaluar
.\target\release\train.exe eval --weights ai\models\best_ai.json

# 3) Ver en ventana (minifb)
.\target\release\train.exe watch --weights ai\models\best_ai.json
.\target\release\watchall.exe --weights ai\models\best_ai1.json ai\models\best_ai2.json ai\models\best_ai3.json
```

| Parámetro | Default | Qué hace |
|---|---|---|
| `--gens` | 300 | Generaciones neuroevolutivas |
| `--pop` | 64 | Población por generación |
| `--skip` | 4 | Frame-skip (decide cada 4 frames). Se interrumpe al aterrizar |
| `MAX_FRAMES` | 3000 | Tope por episodio (~50 s) |
| `STUCK_TIMEOUT` | 150 (180 en watch) | Corta episodio si 2.5s sin avanzar |
| `JUMP_HOLD` | 26 | Frames forzados de `A` tras decidir "saltar" (salto completo; sin esto salta 20px y choca el tubo) |

### Fitness (orden de importancia)

1. Distancia X máxima alcanzada (prueba principal de "llegar lejos").
2. Tiempo (speedrun) dentro del mismo X.
3. Esfuerzo (penaliza saltos inútiles) vs. necesidad de saltar.
4. Bonus por monedas/puntos/victoria (bandera).

### Trucos del entrenador (por qué funciona)

1. **Snapshot tras título** (`create_warmup_snapshot`): espera demo hasta `op($0770)==$01 && timer 1..3 && $0E==$08 && x>30 && en suelo`, luego empuja `R+B` hasta x>80. Si el mando se ignora (demo), o si pulsa START en pausa, hay recovery. Ahorra millones de frames por generación al clonar `ram+cpu+ppu+oam+...` en vez de rebootear.
2. **Sensores exactos**: `Block Buffer` real (`$0500` 208 bytes/página, `col=mt_x&15, row=(wy-32)/16`) — 0 falsos positivos con nubes/arbustos (que en VRAM son sólido pero en Block Buffer son aire). Enemigos exactos desde RAM, no sprites OAM.
3. **Ballistic prediction**: altura de muro + ancho de pozo para decidir si saltar y cuánto (ver `scan_environment` en `playbot.rs`, misma idea).
4. **Frame-skip inteligente**: si el agente decide "saltar", fuerza `A` durante `JUMP_HOLD` frames y corta si aterriza pronto (flanco `A` exige `PreviousA==0`).
5. **Smart prior**: init Gauss×0.35 + sesgo a `R+B` / `R+A+B` para población inicial que ya corre.

## Modelos

| Archivo | Tamaño | Descripción |
|---|---|---|
| `ai/models/best_real.json` | 58 KB | Cerebro principal actual (evaluate/watch con este) |
| `ai/models/best_ai1.json`, `best_ai2.json`, `best_ai3.json` | 58 KB c/u | Población para `watchall` (carrera) |
| `ai/models/best_seed.json`, `best_flag.json` | 58 KB | Seeds / flagpole |
| `ai/models/best_gpu.json` | 937 KB | GRRN-v5 Colab (159→128→128→39). Requiere parser distinto (`watchall` lo detecta por tamaño) |
| `ai/logs/train*.log` | — | Logs de entrenamiento |
| `ai/tmp/tmp*.json` | — | Intermedios (no versionar) |

Formato JSON: array plano de floats `w[0..6151]` (o `NW_V5` para GRRN).

## Verificación rápida

```powershell
.\target\release\train.exe eval --weights ai\models\best_real.json
# debe imprimir fitness, distancia max y frames sin panicar

.\target\release\watchall.exe --weights ai\models\best_real.json ai\models\best_ai1.json ai\models\best_ai2.json ai\models\best_ai3.json
# ventana 2x2, HUD con progreso/récord/victorias/fitness y borde dorado al líder
# [R] sincronizada  [Tab] turbo 1x/2x/4x  [M] rondas  [Espacio] pausa
```

## Integración con `playbot`

El bot TAS heurístico (`src/bin/playbot.rs`) comparte los mismos sensores (`scan_environment`, `world_tile_kind`) que la IA. Su `tricks_audit.log` es el que genera los vídeos de `assets/videos/`. La IA busca trucos por prueba-error; `playbot --macro` permite inyectar los trucos ya descubiertos por nombre (`name=...@x=...@fr=...:SEQ(...)`) sin reentrenar.
