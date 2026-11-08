# AI — Neuroevolution network (train / watchall)

> **[Español](README.md)** | **English** (this file)

High-performance trainer in `src/bin/train.rs` (~1200 lines) + multi-grid viewer `src/bin/watchall.rs`.

## Architecture (`train.rs`)

```
120 sensors → 48 hidden (ReLU) → 7 actions
6151 total parameters (120·48+48 + 48·7+7)
```

**120 sensors** (`extract()`): pos/vel (`$57,$9F`), ground (`$1D`), enemies (`$000F`), 4×10 Block Buffer (`$0500`), wall (dist+height), pit (dist+width), BCD score/coins/timer.

**7 actions**: `0, R, R+A, R+B, R+A+B, A, L`.

**Colab GRRN-v5**: `159 → 128 → 128 → 39` (recurrent memory 32).

## Training

```powershell
cargo build --release --bin train --bin watchall
.\target\release\train.exe train --gens 300 --pop 64 --skip 4 --seed-file ai\models\best_real.json --out ai\models\best_ai.json
.\target\release\train.exe eval --weights ai\models\best_ai.json
.\target\release\train.exe watch --weights ai\models\best_ai.json
.\target\release\watchall.exe --weights ai\models\best_ai1.json ai\models\best_ai2.json ai\models\best_ai3.json
```

Fitness: max X distance, speedrun time, effort, coin/flag bonus. Tricks: instant title snapshot, exact Block Buffer sensors, ballistic prediction, frame-skip + `JUMP_HOLD=26`, smart prior (Gauss×0.35 + R+B bias).

## Models

`ai/models/best_real.json` (58KB main), `best_ai1/2/3.json`, `best_seed/flag.json`, `best_gpu.json` (937KB GRRN-v5).

## Integration with `playbot`

Heuristic TAS bot shares sensors with AI. `tricks_audit.log` generates `assets/videos/`. `playbot --macro "name=...@x=...@fr=...:SEQ(...)"` injects discovered tricks without retraining.
