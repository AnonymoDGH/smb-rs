# Bugs, glitches and tricks — Super Mario Bros. (World, NES)

> [Español](BUGS_Y_TRUCOS.md) | **English** (this file, translated)

Static analysis on the original ROM `Super Mario Bros. (World).nes` (mapper 0, NROM, vertical mirroring), verified with the `smb-rs` emulator (6502 + 2C02). All sections include ROM addresses and RAM variables.

## 0. ROM reference

| Campo | Valor |
|---|---|
| File | `Super Mario Bros. (World).nes` |
| Size | 40 976 B |
| CRC32 | `3337EC46` |
| SHA-256 | `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de` |
| PRG SHA-256 | `5374abb64cfb9b5d961856c60166cedc55859164cf2b8867730e144fa2bdd594` |
| Mapper / mirroring | 0 / vertical |
| Vectors | NMI `$8082`, RESET `$8000`, IRQ `$FFF0` |

## 1. The 21-frame rule

### 1.1 `IntervalTimerControl` (`$077F`)

NMI timer loop `$80F6-$8119`. `$077F` starts at `$14`. Decrements each frame; on `$FF` wrap it resets to `$14` and uses `X=$23`. Short timers `$0780-$0794` tick every frame (`X=$14`); long timers `$0780-$07A3` only on wrap. Period **21 frames = 0.3495s @60.0988Hz**.

### 1.2 `ScreenTimer` (`$07A0`)

Set to `7` at `$88A5`, only ticks on wrap → **7 wraps = 147 frames = 2.446s** per screen tick. All transitions quantized.

### 1.3-1.4 Why it matters (buttons)

Perfect frame ≈ perfect on the 21f grid. If a trick fails once you hit the wrong side of the cycle. Reference: level midpoint scroll. **8-4 is the exception** (§7).

## 2. Flagpole

- Metatiles `$24` (ball) / `$25` (shaft). `HandleClimbing` hitbox only 4px wide (low nibble X `$06-$09`). Y table `$DE29`= `18 22 50 68 90`, score index `$010F`, `subroutine=4` at `$DE73`, `PutPlayerOnVine $DE88`.
- Score tables `$B84B`/`$B850`, awarded in `FlagpoleRoutine $B855` when flag `Y>=$AA` and Mario `Y>=$A2`.
- Normal sequence: `FlagpoleSlide $B2A4` (down to `Y=$9E`) + `PlayerEndLevel $B2CA` (~1.5s + ~2s walk).
- **FPG / FFPG / half**: hitbox+frame lets slide be skipped (FPG) or slide+walk (FFPG, ~3s). Hold **B+Right**, jump to pole at top speed.
- Buttons §2.5: B+Right, jump just before pole, no extra input.

## 3. Wrong warp

- `$0750 AreaPointer` written by row `$0E` at `$C235-$C24D`, read by `HandlePipeEntry $DEE8`. Enter ~20px right → stale pointer → wrong area. Table `$87F2`=`04 03 02 00 / 24 05 24 00 / 08 07 06 00`, X thresholds `$60/$A0` (`$DF16`).
- Buttons: 4-2 warp zone, B+Right, middle of pipe (~20px right of left wall), **Down**.

## 4. Minus World

- `$24` entries in `$87F2` unvalidated → world `$23` (35) shown as **36** / `-1`. 1-2 final pipe. Buttons: wall before pipe, **Up+Right**, **Down** → `World 36-1`.

## 5. Physics: acceleration, turn, friction

- `PlayerPhysicsSub $B343`, `X_Physics $B51C`, `MovePlayerHorizontally $BF11`. Tables `MaxLeft d8 e8 f0 ($B440)`, `MaxRight 28 18 10 0c ($B443)`, `Friction e4 98 d0 ($B447)`.
- Speed 1/16px, `MoveForce $0705` 1/4096px, max run `$28`=2.5px/f. Turn (facing≠moving) doubles friction (`ASL $0702`/`ROL $0701` $B57F). Hold B+direction for max; releasing B in air keeps speed.

## 6. Subpixels

- 1/16px speed + 1/4096 MoveForce fraction persists across jumps. Decides which frame crosses a pixel → pipe/flagpole/wall frame.

## 7. 8-4 exception

- `HandleAxeMetatile $9384`: `OperMode=2`, task 0, speed `$18`, **no** `ScreenTimer`/intermediate screen → not quantized to 21f. Only level where final time is frame-exact.

## 8-31. Extended catalog (see Spanish for full addresses)

Walljump (X>16/<240, block boundary, 1px in), Vine teleport (4-2, `$26`), Pipe clip (`$6C/$1F`, `PlyrPipe`), Going through bricks (5 apps), Backwards accel (L+R 1f + A 1f + B + face back 1f), Shooting faster (L+R → fireball 2×, `FireballXSpdData $40 $C0`), Stomp from below / Moustache (`Player_Y_Speed` check +12), Enemy hitboxes (every other frame) / Under Piranha, Walking inside enemies (`d5 $20`), Bouncing high (`$FD/$FC`), Jumping through hammers (every other frame), Turning around (L+R 1f 40→24) + Moonwalking (Xspd `1/$FF`, `x/x-16`), Avoiding fireworks (timer digit 1/3/6 → `FireworksCounter $06D7`), Sliding on one foot, Lip jumps, Firebar hiding, **Small Fiery Mario** (`PlayerStatus $0756` vs `PlayerSize $0754`, bridge `$89`), Low-Gravity Mario (first jump after transition, `JumpMForceData`), Koopa mid-air (shell in first 2 rows), 5-sprite limit (`FindEmptyEnemySlot` $2672, 5 slots), Whirlpools (3415, 1px/2f), Springs (`$67/$68`, `VerticalForce=$70`), Staircase (`$07 $07 $06...`). RAM table §31.

## 9-11. Emulator fixes, reproduction, references

Pattern table `$2000` bug, palette, sprite-0, nestest 5260. Commands `cargo build --release`, `nesdump --frames/--script/--state/--nestest`. TASVideos, SDA, Negative Seven, Simplistic6502.

*Generated from static ROM analysis + `smb-rs`. ROM addresses = PRG base `$8000`.*
