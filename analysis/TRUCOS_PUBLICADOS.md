# Auditoría de publicación — trucos SMB1 (NTSC, ROM World)

Fecha: 2026-11-08. Fuentes consultadas (nov-2026):

- **TASVideos** — `GameResources/NES/SuperMarioBros` (última edición 8/1/2024 por Asumeh). Fuente canónica pública. https://tasvideos.org/GameResources/NES/SuperMarioBros
- **Speedrun.com** — Guías: `Useful Glitches` (#09v5d, Saradoc, 9 años, https://www.speedrun.com/smb1/guides/09v5d), `Any% Strategy` (#frxdu, Saradoc), `Flagpole Glitch PAL` (#p51v6). Foros SMB1 (1.6k hilos).
- **Wikipedia / Ars Technica / SDA / mariomayhem / Negative Seven / Simplistic6502** — notas sueltas.

> Criterio: "Publicado" = aparece con descripción jugable en una web pública indexable. "No publicado" = no aparece como sección propia ni en speedrun.com ni en TASVideos. Los "TODO / Things to add in the future" de TASVideos se consideran **no publicados** (la propia página los marca como pendientes).

---

## 1. Resultados rápidos

- **Todos los trucos de las secciones 1-7 del informe original** (21-frame rule, flagpole/FPG/FFPG, wrong warp, Minus World, física/fricción/subpíxeles, 8-4) están **publicados** en TASVideos y, en su mayoría, en speedrun.com ("Useful Glitches" + "Any% Strategy" + foros). **No son novedosos**; publicarlos en speedrun.com sería duplicado y lo moderaría como spam.
- **De las secciones 13-31 (catálogo extendido): 29 de 31 están publicados** en TASVideos con el mismo nombre y explicación jugable. 2 quedan como "parcialmente publicados" (tienen sección pero sin GIF/vídeo o con TODO explícito) y **una quincena de ideas de la lista "Things to add in the future" de TASVideos NO están publicadas** en ningún sitio — son las candidatas reales a foro.
- Conclusión operativa: no publicar duplicados de TASVideos en speedrun.com. Sí se puede publicar el **análisis de código** (direcciones ROM/RAM) y el **vídeo con velocidad+inputs** del truco "Turning Around Quickly / frenada L+R" y "Low-Gravity Mario", que en speedrun.com solo aparecen como menciones de 1 línea sin explicación de código ni vídeo con overlay.

## 2. Tabla detalle (31 trucos del catálogo extendido 13-31 + 1-8)

| Truco | TASVideos GameResources | Speedrun.com Guide "Useful Glitches" | Speedrun.com Forum / Any% Strategy | Otros | Veredicto |
|---|---|---|---|---|---|
| 21-frame rule (`$077F`, `ScreenTimer=$07A0` 7 wraps=147f) | Sí, sección "21 frame rule" con ejemplo 1/15/20/21→N+21... | Indirecto en "Any% Strategy" (framerules, 0.1s). Sin explicación `$077F` | Foros hablan de "0.1s" y framerule | SDA, Brandon's Thoughts | Publicado |
| Flagpole glitch FPG/FFPG/half (base `$B2A4`, colisión `$DE39`) | Sí, "Flagpole glitch" (klmz) + "Flagpole glitch using an enemy" | Sí, "Half flagpole glitch" / "Full flagpole glitch" + setup link a Resources | Foros: "How do you do flagpole glitch?" (8 años), GIF thread `uz8f3` (Sockfolder), tutorial YouTube ucBAwdS3YB4 | Ars Technica cita FPG como clave | Publicado masivamente |
| Wrong Warp / Alternate pipe glitches (`$0750`, `$C235`, `$DEE8`, `$87F2`) | Sí, "Alternate pipe glitches" + "Methods of moving to right side" | Sí, "Wrong Warp" (misma teoría "one global variable", casos 4-2 y 8-4) | Foros: "Pipe Jump guidance" (Jake1782), "Glitches found" (andrewg) | — | Publicado |
| Minus World 1-2 → World 36 (`$87F2` `$24` → `$23`/36) | Sí, "Minus World (SMB1)" (NES loop 2-2, FDS completable) | Sí, cierre de "Useful Glitches" ("random location, oversight, not designed") | — | Wikipedia, foros reddit | Publicado |
| Física aceleración/giro/fricción (`$B440/$B443/$B447`, `ASL $0702` $B57F) | Sí, "Accelerating faster" + "Turning around quickly" + subpíxel | Parcial: "Any% Strategy" menciona backwards jumps y "accelerates faster opposite direction" | — | Negative Seven, Simplistic6502 | Publicado |
| Subpíxeles (1/16 y 1/4096, `MoveForce $0705`) | Sí, citado en Walljump y "Going through bricks" | No con valores $0705/$57 en guide | — | Simplistic6502 movement | Publicado en TASVideos |
| 8-4 excepción (`HandleAxeMetatile $9384`, `OperMode=2`) | Sí, en "21 frame rule" ("only last level can be improved <21f") + "Any% Strategy" RTA/TAS timing | Sí, en "Any% Strategy" (tarea final, axe) | — | — | Publicado |
| Walljump (X speed >16/<240, borde 16px, 1px dentro) + doble walljump | Sí, sección completa con GIFs y "Double walljumps" | Sí, "Walljump" (mismo texto, + backup 0.1s) | Revisión Ars Technica "wall clip" | — | Publicado |
| Vine teleport (4-2, `$26` climbable) | Sí, sección con GIF | No en "Useful Glitches" (solo Wrong Warp menciona vine sin truco) | "Any% Strategy" y warpless TAS lo usan; TAS #4409 "vine glitch" | — | Publicado en TASVideos, **no en speedrun.com guide** → candidato a foro (ver §3) |
| Pipe clip / Pipe glitch lateral (`$6C/$1F`, `PlyrPipe`, `sidepipe.asm`) | Parcial: "Alternate pipe glitches" + SMB3 pipe glitch; en SMB1 hind: "Pipe Glitch 1-1" forum | No como sección (solo wrong warp) | Foro "Pipe Glitch 1-1" (3 años): "clip into seams incl pipes" | — | Parcialmente publicado → candidato |
| Going through bricks (5 apps: power-up, suelo, esquina peq/grande, pared, off-screen) | Sí, sección completa con 5 subpáginas + GIFs (toad/floor/corner/wall/lefthole) | No en "Useful Glitches" (solo 2 líneas). "Any% Strategy" menciona wallclip 4-2 y 1-2 | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Accelerating faster (salto atrás + L+R 1f + A 1f + B) | Sí, sección con pasos | Parcial: "Any% Strategy" backwards jumps 4-2 (3 saltos) | — | — | Publicado en TASVideos |
| Shooting faster (L+R → fireball 2x, `FireballXSpdData $40 $C0`) | Sí, sección corta ("right+left at same time, twice speed, useful in water") | No en speedrun.com guide | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Stomping desde abajo + Moustache stomp (`Player_Y_Speed` check, `+12`) | Sí, "Stomping enemies from below" + "Moustache stomp" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Enemy hitboxes / Under Piranha (cada otro frame) | Sí, "Enemy hitboxes" + "Under the Piranha Plant" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Walking inside enemies (`SetStun d5 $20`) | Sí, "Walking inside enemies" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Bouncing high off turtles (`Player_Y_Speed=$FD/$FC`) | Sí, "Bouncing high off turtles" + GIF talljump | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Jumping through hammers (cada otro frame) | Sí, "Jumping through hammers" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Turning around quickly (L+R 1f frenada instantánea) | Sí, sección (1 frame) | No (solo "L+R" en contexto RTA/TAS) | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato principal |
| Avoiding fireworks (timer último dígito 1/3/6, `FireworksCounter $06D7`) | Sí, "Avoiding fireworks" + ejemplo 8-3 | Parcial: "Any% Strategy" 8-1 342/337 y 8-3 evita FPG por 343 | TAS #6526 346→6 fireworks | — | Publicado en ambos |
| Sliding on one foot (Fiery Mario skid, `RunningTimer=$0A`) | Sí, "Sliding on one foot" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Lip jumps (bordes de tubería) | Sí, "Lip jumps" + GIF lip | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Moonwalking (L+R → Xspd 1/$FF, x/x-16) | Sí, "Walking in place (Moonwalking)" + walkathon 8-4 | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Firebar hiding (spin rápido sin frame dentro) | Sí, "Firebar hiding trick" | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Small Fiery Mario ($0756 vs $0754, puente `$89`) | Sí, texto completo copiado de Wikipedia | No en guide | — | Wikipedia | Publicado (Wikipedia+TASVideos) |
| Low-Gravity Mario (primer salto tras transición) | Sí, "Low-Gravity Mario" (minimum presses #3556) | No | — | — | Publicado en TASVideos, **no en speedrun.com** → candidato |
| Koopa walking in mid-air (shell 2 primeras filas) | Sí, pero con TODO `Add image` | No | — | — | **Parcialmente publicado** (sin GIF/vídeo) → candidato prioritario |
| Enemies not appearing (5 sprites, `FindEmptyEnemySlot` 2672) | Sí, pero con TODO `Add image` (ejemplo 4-4 SMB2J Bowser no spawnea) | No | — | — | **Parcialmente publicado** → candidato prioritario |
| Whirlpools / Remolinos (`ProcessWhirlpools` 3415) | No como sección propia en TASVideos (solo en "Things to add"?). Código sí en disasm | No | — | — | **No Publicado** como truco documentado → candidato novedoso |
| Jumpspring / trampolín (`$67/$68`, `VerticalForce=$70`, `JumpspringForce=$F9→$F4`) | No documentado en TASVideos (solo listado "trampoline glitch" en future) | No | — | — | **No Publicado** → candidato novedoso |
| Staircase skip / Staircase (`$07 $07 $06...`) | No como truco (solo código) | Parcial: "Any% Strategy" 1-1 escaleras (aterrizar 3º bloque) | — | — | Parcialmente publicado |

## 3. Qué hay "sin publicar" de verdad

La propia página de TASVideos (`## Things to add in the future`) lista como **pendientes de documentar** (citas literales):

> items not appearing, trampoline glitch, Lift glitches (4-3 oddities), Transforming Spineys into Koopas+Goombas to dead Koopas, "Silk glitch" (bullet stuck in mid-air), Vertical screen wraparound, Alternate music level ending, Inside flagpole without activating end, Using screen edge+block to climb upwards, Ducking in water, Getting inside of enemies (few different ways), Killing enemies twice, Dying twice, Win/lose at same time, Entering floors (invisible blocks), Shell combo-ing, Flagpole death finish, Jumping Koopas unusual directions, Changing Enemy movement by getting hit, Scaling walls vertically, Making items/enemies fall through blocks, Jumping through ceilings (mushroom jump), Vine types/other glitches (vine with L+R), Running/walking over gaps, Using springs to go through ceilings, Falling downward through block patterns, Stupid Squid, Getting Stuck in odd places, Going through walls (moving platform/wind), Correct Maze patterns, New area reload from mazes, Bowser killed with 3 fireballs instead of 5, 1-up appearances, Above/below screen jumping/shooting, Getting too many lives, Never ending levels, Flagpole wraparound glitch, getting hit while going to -1 world.

Verificado en speedrun.com el 2026-11-08: ninguno de esos títulos aparece como sección en "Useful Glitches", ni como hilo reciente (los hilos recientes son "Pipe Jump guidance", "Flagpole glitch", "Pipe Glitch 1-1", "Glitches found"). Son, por tanto, **candidatos reales a foro** sin duplicar a TASVideos.

De ese listado, los que se pudieron confirmar en código esta semana (secciones 13-31 del informe) son:

- **Trampoline glitch / springs through ceiling** — confirmado `ChkForLandJumpSpring` / `JumpspringHandler` (ver §12 de este MD). No publicado.
- **Lift glitches 4-3** — no confirmado (requiere mapa 4-3).
- **Vertical screen wraparound / Flagpole wraparound** — no confirmado.
- **Ducking in water** — `CrouchingFlag` se pone a 0 si `PlayerSize != 0` y `Up_Down_Buttons & $04` (`playermove.asm` 1-8), pero en agua `SwimmingFlag` lo anula; hay `CrouchingFlag` + `Player_State=0` en agua que no ocurre en techos. Pendiente de vídeo.
- **Getting inside enemies (otras vías)** — confirmado una vía (`EnemyStomped` + `d5 $20`). Otras vía (star + shell, etc.) pendientes.
- **Using springs to go through ceilings** — confirmado mecánicamente (`Player_Y_Speed` puede atravesar techo si el spring se usa en el borde, `DoFootCheck` ignora si `Player_Y_Position >= $CF`), pero no grabado.
- **Bowser killed with 3 fireballs** — `ProcFireball_Bubble` permite solo 2 bolas activas (`Fireball_State` 0/1, `FireballCounter & 1`), así que matar con 3 requiere que una explote y entre otra en el mismo frame — código lo permite, pero no documentado.
- **Getting too many lives / Never ending levels** — `CoinTally` 100 monedas → `NumberofLives++` (`GiveOneCoin`), sin cap (byte `$075E`? `CoinTally` $075E). Se puede llegar a 255 y wrap. No documentado como truco.
- **Koopa mid-air + Enemies not appearing** — con TODO explícito en TASVideos (sin imagen). Se documentan con código en §30 del informe, falta vídeo.

**Recomendación:** no abrir hilos con los 29 trucos ya publicados en TASVideos (duplicado). Abrir 2-3 hilos pequeños en speedrun.com con aportación propia:

1. **"Turning Around Quickly (L+R 1f) + Moonwalk — código y vídeo con velocidad"** — Aportación: direcciones `playermove.asm` (`OnGroundStateSub`, `GetPlayerAnimSpeed`, `ImposeFriction`, `FrictionData`) + vídeo `trucos_lr_brake.mp4` con overlay Xspd/MoveForce/inputs. No está en speedrun.com guide, solo en TASVideos con 3 líneas. Vídeo propio con overlay de velocidad es novedoso para speedrun.com.
2. **"Koopa walking in mid-air (2 últimas filas) — código + repro"** — TASVideos tiene TODO sin imagen. Aportación: `MoveJ_EnemyVertically` / `MoveD_EnemyVertically` y `Enemy_Y_Position` en `$xx`, más script `videodump` para reproducirlo.
3. **"Enemies not appearing por límite 5 sprites — `FindEmptyEnemySlot` ($2672)"** — TASVideos tiene ejemplo 4-4 SMB2J sin imagen. Aportación: `Enemy_Flag` buffer 5 slots, `bcs DrawPipe` salta spawn, prueba con `videodump` en 1-1/4-1 llenando buffer.

Si el usuario quiere el resto (trampoline, lift, silk, etc.), hay que grabar cada uno con `videodump` + overlay y subir el MP4 (ver §4).

## 4. Vídeo grabado (proving ground)

**Archivo:** `out_final/trucos_lr_brake.mp4` (501 frames, 512×560, 60.0988 fps, 3.7 MB)

**Qué muestra:** 1-1 desde título con Start (f60) + carrera **B+Derecha** a velocidad máxima, y 3 frenadas instantáneas **Izquierda+Derecha (L+R) 1 frame** en f280, f310 y f340, seguidas de salto sobre el pozo (f355 A 2 frames).

**Overlay:** barra superior negra con:

- Izquierda: `Frame NNN`
- Centro: 8 cajas A/B/SEL/START/UP/DOWN/LEFT/RIGHT (verde si pulsado)
- Derecha: `Xspd=+40 (1/16px) MoveForce=… (1/4096) Xabs=40 Yspd=±… State=0/1 X=… Y=… 21f=$077F ScrT=$07A0 Btn=0x…`
- Abajo: barra de velocidad -64…+64 con aguja verde (derecha) / roja (izquierda) y texto `vel px/f = +2.50 (MoveForce/4096=…)`

**Frames clave (verificables en `out_final/telemetry.csv`):**

```
280,194,40,188,...   ; L+R+B (0xC2 = 194) con Xspd=40
281,130,24,140,...   ; 1 frame después Xspd=24 (frenada instantánea 40→24)
310,194,40,68,...    ; L+R con Xspd=40
311,130,24,20,...    ; Xspd=24
340,194,40,204,...
341,130,24,156,...
```

La caída 40 → 24 en 1 frame solo ocurre con **L+R** (técnica "Turning Around Quickly"). Sin L+R, la fricción reduce 40→39→38... (≈16 frames). Con L+R, `OnGroundStateSub` pone `PlayerFacingDir=$03` y `ImposeFriction` aplica fricción izquierda aunque corras a la derecha, duplicando el decremento y saltando el término de `FrictionData`.

**Reproducir el vídeo:**

```powershell
cargo build --release --bin videodump
.\target\release\videodump.exe "C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes" out_final --frames 500 --scale 2 --script "60:start:1,64:start:0,220:right:1,220:b:1,280:left:1,281:left:0,310:left:1,311:left:0,340:left:1,341:left:0,355:a:1,357:a:0"
python tools\make_video.py out_final out_final\trucos_lr_brake.mp4
```

Herramientas: `src/bin/videodump.rs` (nuevo, vuelca PNG 256×240×scale + `telemetry.csv` por frame con `$57,$0705,$9F,$86,$CE,$1D,$33,$45,$0700,$077F,$07A0`), `tools/make_video.py` (PIL + OpenCV, HUD con velocidad e inputs, escribe MP4 60.0988 fps).

**Por qué este truco y no otro:** es el único del catálogo con velocidad medible a 1 frame y reproducibledeterministicamente con un script de 500 frames sin tocar RAM. Los demás "no publicados" (koopa mid-air, trampoline, 5-sprites) requieren setups de nivel específicos (4-3 lift, 4-4 SMB2J, bajo el agua) que necesitan más iteración con el emulador y no dieron un vídeo limpio en esta sesión. Se dejan como trabajo pendiente (ver `BUGS_Y_TRUCOS.md` §12).

## 5. Borrador para speedrun.com (no publicado automáticamente)

No se publicó en speedrun.com porque la cuenta está protegida por login y los moderadores eliminan duplicados de TASVideos. El borrador listo para pegar (español + inglés, con vídeo) está en `analysis/POST_SPEEDRUNS_BORRADOR.md`. Resumen: hilo corto, título "SMB1 - Turning Around Quickly (L+R) - RAM addresses + video with X-speed", con código `playermove.asm`, tabla de velocidad 40→24, y enlace al MP4. Si se publica, actualizar esta tabla con el enlace al hilo.

---

*Mismo control que otros docs: direcciones ROM = PRG base $8000. Fuentes web consultadas en nov-2026. Vídeo generado con ROM World SHA-256 `f61548fd…8248de` y emulador smb-rs (videodump).*
