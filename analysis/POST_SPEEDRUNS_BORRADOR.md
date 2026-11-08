# Borrador para foro Speedrun.com — SMB1 (listo para copiar/pegar)

> No publicado automáticamente (speedrun.com exige login). Pega esto en https://www.speedrun.com/smb1/forums (nuevo hilo). Adjunta el MP4 `out_final/trucos_lr_brake.mp4` (512×560, 8.3 s, 3.7 MB). Si lo subes a YouTube, pega el enlace en el hilo.

---

**Título:** SMB1 — Turning Around Quickly (L+R 1 frame) — código RAM + vídeo con velocidad

**Foro:** Super Mario Bros. — General / Tricks

**Cuerpo:**

```
Hi all. Most SMB1 tricks are already on TASVideos (GameResources/NES/SuperMarioBros),
so I won't duplicate them here. I'm posting a small writeup that is NOT in the
speedrun.com "Useful Glitches" guide (Saradoc): the L+R quick turn / brake, with
RAM addresses and a frame-by-frame speed overlay from my own NES emulator (smb-rs,
runs the real ROM PRG SHA-256 5374abb6...bdd594, mapper 0).

Video: (attach trucos_lr_brake.mp4, 501 frames, 60.0988 fps, 512x560)
- Top bar shows inputs (A/B/SEL/START/UP/DOWN/LEFT/RIGHT green when pressed)
  and speed: Xspd in 1/16 px, MoveForce in 1/4096 px, Yspd, State, X/Y, 21-frame
  counter $077F, ScreenTimer $07A0, button byte.
- Bottom bar is a speed gauge -64..+64 (green = right, red = left).
- 1-1 from title (Start f60), run with B+Right to max speed, then 3× L+R for 1
  frame at f280, f310, f340, then jump over the pit (A f355).

What you see (checkable in telemetry.csv):
 f280 buttons=0xC2 (L+R+B=194) Xspd=+40  -> f281 buttons=0x82 (R+B) Xspd=+24
 f310 0xC2 Xspd=+40  -> f311 0x82 Xspd=+24
 f340 0xC2 Xspd=+40  -> f341 0x82 Xspd=+24

So L+R for 1 frame drops X speed instantly from 40 (2.50 px/frame, max run
0x28) to 24 (1.50 px/frame, max walk 0x18). Without L+R, friction walks
40→39→38→... (~16 frames). The code is in playermove.asm / smbdis.asm:

- OnGroundStateSub: LDA Left_Right_Buttons ($03 when L+R) / STA PlayerFacingDir ($33)
- X_Physics ($B51C): picks max speeds from MaxLeftXSpdData (d8 e8 f0 at $B440)
  / MaxRightXSpdData (28 18 10 0c at $B443) and friction from FrictionData
  (e4 98 d0 at $B447).
- ImposeFriction ($B5CC): does AND with Player_CollisionBits ($0490). With L+R,
  the left-friction path runs while you're moving right, so the decrement
  is doubled (ASL $0702 / ROL $0701 at $B57F when Facing != Moving).

That insta-brake is the "Turning Around Quickly" from TASVideos, but with the
RAM trace. I used the same overlay to verify moonwalk (L+R held) keeps you at
24, and backwards acceleration (L+R 1f → A 1f → B → face back 1f without B →
jump with B) reaches 40 in ~10 frames instead of ~20. Full audit + more
tricks + telemetry tool in my repo (see thesis PDF analysis/BUGS_Y_TRUCOS.md
and analysis/TRUCOS_PUBLICADOS.md).

Reproduce:
  cargo build --release --bin videodump
  .\target\release\videodump.exe "Super Mario Bros. (World).nes" out_final --frames 500 --scale 2 --script "60:start:1,64:start:0,220:right:1,220:b:1,280:left:1,281:left:0,310:left:1,311:left:0,340:left:1,341:left:0,355:a:1,357:a:0"
  python tools\make_video.py out_final out_final\trucos_lr_brake.mp4

If the mods want it moved to Guides, I'll paste it as a section. Happy to
do the same for Koopa walking in mid-air (TASVideos has TODO no GIF) and the
5-sprite spawn limit (FindEmptyEnemySlot $2672) once I have clean overlays
for those levels.

— smb-rs (Nestest-validated 6502, 2C02)
```

---

**Flujo de publicación (manual):**

1. Ir a https://www.speedrun.com/smb1/forums → "New topic".
2. Título: `SMB1 — Turning Around Quickly (L+R 1 frame) — code + video`
3. Pegar el cuerpo de arriba, adjuntar `out_final/trucos_lr_brake.mp4` (o subir a YouTube y enlazar).
4. Tras publicar, anotar el enlace en `analysis/TRUCOS_PUBLICADOS.md` §5 y poner el vídeo en el informe `analysis/BUGS_Y_TRUCOS.md` §12.

**Alternativas (solo si el hilo de arriba se acepta):**

- "Koopa walking in mid-air — shell en las 2 últimas filas (código)" — requiere vídeo de 6-4/1-2 con overlay de `Enemy_Y_Position`. TASVideos lo marca TODO.
- "Enemies not appearing (5 sprites) — FindEmptyEnemySlot" — requiere vídeo llenando el buffer con 5 enemigos y mostrando que Bowser/planta no spawnea. TASVideos TODO.
- "Jumpspring (trampolín) through ceiling — VerticalForce=$70, Force=$F9→$F4" — listado en TASVideos "Things to add in future", no documentado. Requiere vídeo en 4-3.

No publiques los 20 trucos que ya están en "Useful Glitches" (Wrong Warp, Walljump, Full/Half flagpole, Minus World, Off-screen wall climb, Jump in mid-air) — duplicado.
