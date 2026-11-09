# deepscan — deep route scanner

> [Español](DEEPSCAN.md) | **English** (this file)

`deepscan` runs **Super Mario Bros. 1-1** on the real emulator, maps the level from the Block Buffer (`$0500-$069F`) and searches for a fast route with deterministic rollouts. It does not use copied level data or a game clone: everything comes from the ROM executing instruction by instruction.

---

## Pipeline

| Stage | Module | What it does |
|---|---|---|
| 1. Boot | `route::beam::boot_to_level` | Power-on → title → 1-1. Returns a `ConsoleSnapshot` and the START tape. |
| 2. Map | `route::map` | Merges the visible 16-column × 13-row window into an absolute map. Records enemies (per-slot spawn) and the flag column. |
| 3. Physics | `route::physics` | Jump/gravity model using the ROM constants (`JumpMForceData`, `FallMForceData`, `PlayerYSpdData`). |
| 4. Plan | `route::planner` | Splits the map into obstacles and picks takeoff/hold for each one. |
| 5. Search | `route::beam` | Beam search over real emulator snapshots. |
| 6. Replay | `deepscan` | Re-runs the route from power-on to verify determinism and generate telemetry. |

The key piece is the **deterministic snapshot** (`ConsoleSnapshot`): CPU + RAM + PPU + APU, without the framebuffer or audio buffer. Restoring a snapshot and repeating a button sequence produces exactly the same state (`--mode selftest` checks it).

---

## Usage

```powershell
cargo build --release

# 1. Snapshot determinism (2 tests)
.\target\release\deepscan.exe "ROM.nes" out_deep --mode selftest

# 2. Map 1-1 with the baseline pilot
.\target\release\deepscan.exe "ROM.nes" out_deep --mode map --frames 4000

# 3. Heuristic plan + closed-loop execution
.\target\release\deepscan.exe "ROM.nes" out_deep --mode plan

# 4. Global search with real rollouts
.\target\release\deepscan.exe "ROM.nes" out_deep --mode global --beam 8 --frames 1800 --stride 8 --max-rollout 40

# 5. Everything + report
.\target\release\deepscan.exe "ROM.nes" out_deep --mode all --beam 12 --frames 2000
```

### Parameters

| Flag | Default | Meaning |
|---|---|---|
| `--mode` | `all` | `selftest`, `map`, `plan`, `global`, `all` |
| `--frames` | 4000 | Level frame budget |
| `--beam` | 16 | Beam width (nodes kept per step) |
| `--stride` | 6 | Frames per run/wait/brake macro-action |
| `--goal-x` | 3160 | Flag X |
| `--max-rollout` | 60 | Maximum frames per rollout |
| `--time-weight` | 0.7 | Time weight in the score |
| `--quiet` | — | No progress traces on stderr |

---

## Outputs

| File | Content |
|---|---|
| `level_map.json` | 13-row × N-column map, enemies, obstacles and flag. |
| `level_map.svg` | Visual map with enemies and flag. |
| `level_map.txt` | ASCII map with a tens ruler. |
| `route_plan.json` | Heuristic plan in JSON. |
| `route_plan.md` | Maneuver table and planner notes. |
| `route_plan.svg` | Map with jump markers. |
| `plan_execution.csv` | Plan execution telemetry (phase, x, y, vx, state, lives). |
| `route.json` | Global search configuration and result. |
| `route_script.txt` | `frame:button:value` tape compatible with `nesdump --script`. |
| `route_trace.csv` | Beam evolution (best node per step). |
| `route_telemetry.csv` | Deterministic replay of the found route. |
| `deepscan_report.md` | Human-readable report with every stage. |
| `selftest.txt` | Determinism test results. |

---

## Beam actions

On the ground:

- `Jump(hold)` with `hold` ∈ {8, 14, 20, 26}: run and jump forward.
- `JumpUp(hold)` ∈ {20, 26}: vertical jump and advance at the end; used for a step directly above.
- `BackJump(back, hold)` with `back` ∈ {14, 28}: back up, build speed and jump; used against a wall Mario is touching.
- `Run(stride)`, `Wait(stride)`, `Brake(stride)`, `Moonwalk(stride)`.

In the air: `Run`, `Wait`, `Brake`.

Node score:

```text
x - time_weight * frame + 0.4 * vx + 2.0 * on_ground
```

A low `time_weight` (0.7) makes the beam prioritize advancing; the final tie-break among flag-reaching routes is done by `best_goal` using frame count. Deduplication uses `(x/8, y/8, vx/4, state, on_ground, last_a)` to avoid keeping near-identical nodes.

---

## Extension points

1. **A\* over the map** — replace `plan_route` with A\* using state `(column, support row, vx)` and `physics::jump_arc` as the transition model.
2. **Parallel rollouts** — each `Console` is independent; `std::thread` + channels can cut beam time almost linearly.
3. **Enemies in the beam** — add the 5 slots (`$000F..$0013`) to the state and a `WaitForEnemy` / `Stomp` action.
4. **Vertical levels** — compensate `ScreenY_Pos` in `LevelMap::ingest` (1-2, 4-2, 8-4).
5. **Multi-objective score** — keep a Pareto front `(frames, x)` instead of a single scalar.
6. **Plan-guided pruning** — use `plan_route` as a prior to limit beam actions to jump zones.
7. **Save/resume** — serialize `ConsoleSnapshot` to disk to continue a long search.

---

## Limitations

- The map assumes horizontal scrolling; vertical levels need camera-Y compensation.
- The plan executor is closed-loop (detects stalls, retries jumps and falls back to the pilot), but it is not an optimal controller.
- The beam is expensive: ~30-40 rollouts/s on this machine. `--beam` and `--frames` are the real budget.
- `route_script.txt` includes the boot frames (START) and replays with `nesdump --script`.
- Death detection uses `Player_Y_HighPos` (`$0B`), not the low byte of Y, because a high jump can read `y=255` while above the screen.

---

## 1-1 results (World ROM)

ROM SHA-256: `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de`.

| Method | Level frames | Final X | Flag |
|---|---|---|---|
| Baseline pilot (`--mode map`) | 1511 | 3161 | yes |
| Heuristic plan (`--mode plan`) | 2134 | 3161 | yes |
| Beam (`--beam 8`, `--stride 8`, `--frames 1800`) | **1426** | **3161** | **yes** |

The beam finds a route **85 frames (~1.4 s) faster than the baseline pilot** without using its sensors: only real rollouts over snapshots. The heuristic plan is slower because it executes maneuvers with retries and does not optimize global time.

Exact result configuration:

```text
beam=8  stride=8  max_rollout=40  time_weight=0.7  frames=1800
rollouts=2868  nodes=477  boot=240f  level=1426f  total=1666f
```

Artifacts from that run are in [`analysis/deepscan/`](deepscan/).

---

## Replaying a route

```powershell
# route_script.txt already contains boot + level
.\target\release\nesdump.exe "ROM.nes" out_replay --frames 2100 --scale 2 --script (Get-Content out_deep\route_script.txt -Raw)
```

For comparison, `route_telemetry.csv` is the replay that `deepscan` itself performs from power-on; if its last row matches `route.json` (`x`, `y`, `frames`), determinism is verified.
