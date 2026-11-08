# smb-rs — contexto para IA
Repo: C:\Users\USUARIO\Desktop\smb-rs
ROM: C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes
ROM SHA-256: f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de | CRC32: 3337EC46 | Mapper 0, vertical, PRG 32K base $8000, NMI $8082 RESET $8000 IRQ $FFF0
Docs: analysis/BUGS_Y_TRUCOS.md (técnico), analysis/disasm.txt (17209 líneas), analysis/smbdis.asm, consts.asm
Build: cargo build --release | Run: .\target\release\smb-rs.exe "<ROM>" [escala]
Controls: Flechas=D-pad X=A Z=B Enter=Start R=Reset
Harness: .\target\release\nesdump.exe "<ROM>" out --frames 500 --scale 2 --script "60:start:1,64:start:0" [--state]
Script format: frame:boton:valor,... botones a,b,select,start,up,down,left,right (1=press 0=release). Ej: 100:right:1,100:b:1,150:right:0,150:b:0
Validación: CPU nestest 5260 ops 0 divergencias (legales); PPU pattern table $2000 bit4 BG/bit3 SPR, sprite-0 OK scanline 25, paleta FCEUX.
Regla 21 frames: $077F IntervalTimerControl bucle $80F6-$8119 reset $14 en $8107; ScreenTimer $07A0 tick en wrap => 7 wraps=147 frames=2.446s ($88A5). Excepción 8-4: HandleAxeMetatile $9384 pone OperMode=2 tarea 0 vel $18 sin ScreenTimer.
Wrong Warp 4-2: mantén B+Derecha, salta tuberías, en warp zona entra ~20px a la derecha de la pared, Abajo. $0750 AreaPointer escrito fila $0E $C235-$C24D leído HandlePipeEntry $DEE8; tabla $87F2=04 03 02 00 /24 05 24 00 /08 07 06 00 umbrales $DF16.
Minus World 1-2: pared antes de última tubería, Arriba+Derecha para quedarte a la derecha de pared, Abajo => World 36. Entradas $24 sin validar, mundo $23->36, $075F mundo $0760 área.
Flagpole: B+Derecha, salta al mástil a tope tocando bola/asta, sin input extra. FPG salta slide, FFPG salta slide+caminata. Hitbox $DE39 nybble X $06-$09, tabla Y $DE29=18 22 50 68 90, $010F índice, $DE73 sub=4, $DE88 PutPlayerOnVine, slide $B2A4 hasta Y $9E, fin $B2CA/$B2FD, puntos $B84B/$B850 y $B855.
Física: PlayerPhysicsSub $B343 X_Physics $B51C MovePlayer $BF11; vel 1/16px MoveForce $0705 1/4096px max carrera $28=2.5px/f; giro fricción doble $B57F ASL $0702 ROL $0701; tablas $B440 d8 e8 f0 $B443 28 18 10 0c $B447 e4 98 d0 ImposeFriction $B5CC clamps $B5EF/$B60D abs $B617.
Estructura: src/cpu.rs src/ppu.rs src/apu.rs src/console.rs src/cartridge.rs src/bin/nesdump.rs
Pendiente: ventana exacta subpíxel/frame por variante FPG, destino exacto wrong warp según AreaPointer, secuencia Minus World.
