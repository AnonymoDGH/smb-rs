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
| 5. Tricks | `route::tricks` | Empirical bug/trick probes and a trick detector over a route. |
| 6. Search | `route::beam` | Beam search over real emulator snapshots. |
| 7. Replay | `deepscan` | Re-runs the route from power-on to verify determinism and generate telemetry. |

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
.\target\release\deepscan.exe "ROM.nes" out_deep --mode global --beam 8 --frames 1800 --stride 8 --max-rollout 40 --time-weight 2.5

# 5. Empirical trick probes (startup, low gravity, reverse friction)
.\target\release\deepscan.exe "ROM.nes" out_deep --mode probe

# 6. Deterministic replay of a saved route_script.txt
.\target\release\deepscan.exe "ROM.nes" out_deep --mode replay --script out_deep\route_script.txt

# 7. Everything + report
.\target\release\deepscan.exe "ROM.nes" out_deep --mode all --beam 12 --frames 2000
```

### Parameters

| Flag | Default | Meaning |
|---|---|---|
| `--mode` | `all` | `selftest`, `map`, `plan`, `global`, `probe`, `replay`, `all` |
| `--frames` | 4000 | Level frame budget |
| `--beam` | 16 | Beam width (nodes kept per step) |
| `--stride` | 6 | Frames per run/wait/brake macro-action |
| `--goal-x` | 3160 | Flag X |
| `--max-rollout` | 60 | Maximum frames per rollout |
| `--time-weight` | 2.5 | Time weight in the score (= max speed, see below) |
| `--script` | — | Route script for `--mode replay` |
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
| `trick_probe.md` | Startup / low-gravity probe table (`--mode probe`). |
| `trick_report.md` | Trick detector over the route: max speed, reverse friction, moonwalk, flagpole. |

---

## Beam actions

On the ground:

- `Jump(hold)` with `hold` ∈ {8, 14, 20, 26}: run and jump forward.
- `JumpUp(hold)` ∈ {20, 26}: vertical jump and advance at the end; used for a step directly above.
- `BackJump(back, hold)` with `back` ∈ {14, 28}: back up, build speed and jump; used against a wall Mario is touching.
- `Run(stride)`, `Wait(stride)`, `Brake(stride)`, `Moonwalk(stride)`.
- `AccelStart` (root node only): `L+R` 1f, `A` 1f, 22f run, 1f turn without B, jump; the reverse-acceleration trick measured by `--mode probe`.

In the air: `Run`, `Wait`, `Brake`.

`Run` stops at the **safe takeoff point** for the next obstacle (Block Buffer sensors: wall/pipe, pit, big-drop edge). Without this the beam can only jump on stride multiples and loses speed against pipes.

Node score:

```text
x - time_weight * frame + 0.4 * vx + 2.0 * on_ground
```

With `time_weight=2.5` (= max speed) the score is a **total-time estimate**: `frame + (goal_x - x) / vx_max`. The final tie-break among flag-reaching routes is `best_goal` by frame count. Deduplication uses position/velocity blocks, except near the flagpole where the exact X is kept because the pole collision depends on the low nybble of X (window 6..9). The goal also fires when the pole collision is registered (`$070F`, `$010F`, `Player_State=3`), not only when `goal_x` is crossed.

---

## Extension points

1. **A\* over the map** — replace `plan_route` with A\* using state `(column, support row, vx)` and `physics::jump_arc` as the transition model.
2. **Parallel rollouts** — each `Console` is independent; `std::thread` + channels can cut beam time almost linearly.
3. **Enemies in the beam** — add the 5 slots (`$000F..$0013`) to the state and a `WaitForEnemy` / `Stomp` action.
4. **Vertical levels** — compensate `ScreenY_Pos` in `LevelMap::ingest` (1-2, 4-2, 8-4).
5. **Multi-objective score** — keep a Pareto front `(frames, x)` instead of a single scalar.
6. **Plan-guided pruning** — use `plan_route` as a prior to limit beam actions to jump zones.
7. **Save/resume** — serialize `ConsoleSnapshot` to disk to continue a long search.
8. **Advanced tricks** — `AccelStart`, moonwalk, walljump and FPG/FFPG already have primitives or detectors in `route::tricks`; walljump still needs its own action and the FPG should be measured as a full-level category (to the castle), not as a pole touch.

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

| Method | Level frames | Time (60.0988 fps) | Final X | Flag |
|---|---|---|---|---|
| Baseline pilot (`--mode map`) | 1511 | 25.15 s | 3161 | yes |
| Heuristic plan (`--mode plan`) | 2134 | 35.51 s | 3161 | yes |
| Beam (`--time-weight 0.7`) | 1426 | 23.73 s | 3161 | yes |
| Beam (`--time-weight 2.5`) | **1266** | **21.07 s** | **3161** | **yes** |

**1-1 record (21.43 s ≈ 1288 frames):** with the start at first movement (the *All-Stars* definition) the route is 1266f (21.07 s, **22 frames under**). With the SDA "400" start it is 1290f (21.47 s, 2 frames over). The time ends on the pole touch, so the flagpole glitch (FPG/FFPG), which only skips the later animation, does not improve this mark; it was probed in 550 combos with no success (see section 32). See [`BUGS_Y_TRUCOS.md`](BUGS_Y_TRUCOS.md) for the analysis (reverse acceleration +11 px, low gravity with no advantage, subspeed and 1-frame pulses with no gain, walljump/moonwalk unused).

Exact configuration for the fastest result:

```text
beam=8  stride=8  max_rollout=40  time_weight=2.5  frames=1800
rollouts=9154  nodes=880  boot=240f  level=1266f  total=1506f
```

Artifacts from that run are in [`analysis/deepscan/`](deepscan/).

---

## Replaying a route

```powershell
# route_script.txt already contains boot + level
.\target\release\nesdump.exe "ROM.nes" out_replay --frames 2100 --scale 2 --script (Get-Content out_deep\route_script.txt -Raw)
```

For comparison, `route_telemetry.csv` is the replay that `deepscan` itself performs from power-on; if its last row matches `route.json` (`x`, `y`, `frames`), determinism is verified.
