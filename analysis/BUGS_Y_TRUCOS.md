# Bugs, glitches y trucos — Super Mario Bros. (World, NES)

Análisis estático sobre la ROM original `Super Mario Bros. (World).nes` (mapper 0, NROM, mirroring vertical), verificada con el emulador `smb-rs` (CPU 6502 + PPU 2C02). Todo lo que sigue incluye direcciones ROM y variables de RAM para que se puedan reproducir y verificar.

## 0. Referencias de la ROM

| Campo | Valor |
|---|---|
| Archivo | `Super Mario Bros. (World).nes` |
| Tamaño | 40 976 B |
| CRC32 | `3337EC46` |
| SHA-256 | `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de` |
| PRG SHA-256 | `5374abb64cfb9b5d961856c60166cedc55859164cf2b8867730e144fa2bdd594` |
| Mapper / mirroring | 0 / vertical |
| Vectores | NMI `$8082`, RESET `$8000`, IRQ `$FFF0` |

## 1. La regla de los 21 frames (frame rule)

### 1.1 El contador `$077F`

Bucle de timers en la NMI (área `$80F6-$8119`):

```
$80F6  LDA $0747
$80F9  BEQ $8100
$80FB  DEC $0747
$80FE  BNE $8119          ; frames con delay, no se tocan timers
$8100  LDX #$14
$8102  DEC $077F          ; interval timer control
$8105  BPL $810E          ; si sigue >=0, solo timers "cortos"
$8107  LDA #$14
$8109  STA $077F          ; reset cada 21 frames
$810C  LDX #$23
$810E  LDA $0780,X
$8111  BEQ $8116
$8113  DEC $0780,X        ; decrementa el timer del índice X
$8116  DEX
$8117  BPL $810E
$8119  INC $09
```

- `$077F` arranca en `$14` (20). Se decrementa cada frame; cuando pasa de `$00` a `$FF` se resetea a `$14` y se usa `X=$23`.
- En frames normales `X=$14`: solo los timers `$0780-$0794` (21 bytes) se decrementan cada frame.
- En el frame de wrap `X=$23`: se decrementan todos `$0780-$07A3` (36 bytes), incluidos los timers "largos".
- Periodo: **21 frames = 0.3495 s** a 60.0988 Hz. Ese es el "frame rule".

### 1.2 `ScreenTimer` y las transiciones de pantalla

```
$889D  LDA $07A0          ; ScreenTimer
$88A0  BNE $88AD          ; si >0, no hacer nada
$88A2  JSR $8220          ; avanzar la rutina de pantalla
$88A5  LDA #$07
$88A7  STA $07A0          ; ScreenTimer = 7
$88AA  INC $073C          ; ScreenRoutineTask++
$88AD  RTS
```

- `$07A0` está en el bloque `$0795-$07A3`, así que solo se decrementa en frames de wrap. Por tanto, cada tick de `ScreenTimer` cuesta **7 wraps = 147 frames = 2.446 s**.
- Todas las transiciones de pantalla (inicio de nivel, secuencia de castillo, muerte, etc.) están cuantizadas a esa rejilla de 21 frames.
- Los spawns de enemigos, tuberías y timers comparten la misma rejilla.

### 1.3 Por qué importa

- Un "frame perfecto" en SMB casi siempre es "perfecto al frame rule": el ahorro es múltiplo de 21 frames.
- Cualquier acción que dependa de un timer largo debe alinearse al wrap; si no, se espera al siguiente.
- **8-4 es la excepción** (ver sección 7).

### 1.4 Cómo se nota jugando (con botones)

- Si un trick te sale mal una vez no es que fallaste el botón, es que lo hiciste en el frame contrario del ciclo de 21.
- La referencia es el **punto medio del nivel**: cuando la pantalla hace scroll y aparece el primer bloque. Todo lo demás cuenta desde ahí.
- Los spawns de enemigos, tuberías y las transiciones de pantalla comparten esa rejilla. Por eso un salto idéntico puede agarrar el flagpole en un frame o en el siguiente según dónde caiga el wrap de `$077F`.

## 2. El flagpole

### 2.1 Detección de colisión

- Metatiles: `$24` = bola superior, `$25` = asta.
- `HandleClimbing` (alrededor de `$DE29`) restringe el hitbox: el nybble bajo de la coordenada horizontal debe estar entre `$06` y `$09` (solo 4 px de ancho, aunque el sprite ocupe 16).
- Tabla de posiciones verticales `$DE29` = `18 22 50 68 90`.
- El índice se elige empezando en `X=4` y bajando mientras `Player_Y >= tabla[X]` sea falso; se guarda en `$010F` (`FlagpoleScore`).
- `GameEngineSubroutine = 4` en `$DE73` (rutina de slide).
- `PutPlayerOnVine` en `$DE88`: `Player_State = 3`, anula `Player_X_Speed` y `Player_X_MoveForce`, encaja a Mario en la asta.

### 2.2 Puntuación

Tablas en `$B84B` (cantidad) y `$B850` (dígito):

```
$B84B  05 02 08 04 01      ; FlagpoleScoreMods
$B850  03 03 04 04 04      ; FlagpoleScoreDigits
```

- El índice 0 corresponde a la parte superior del asta y se muestra como **5000**; el 4 como **100**. Los tiles floatey en `FlagpoleScoreNumTiles` (`$f9,$50` = "5000", `$f7,$50` = "2000", `$fa,$fb` = "800", `$f8,$fb` = "400", `$f6,$fb` = "100") confirman los valores mostrados.
- La puntuación se otorga en `FlagpoleRoutine` (`$B855`) cuando la bandera llega a `Y >= $AA` y Mario a `Y >= $A2` (`$B86D`/`$B873`), entre `$B899` y `$B8AA`: se carga el mod y el dígito, se llama a `AddToScore` y se pone `GameEngineSubroutine = 5`.
- Bonus de final de nivel: 50 puntos por unidad de tiempo que quede, en la rutina de fin de área (líneas `7675-7687` del `smbdis.asm`).

### 2.3 Secuencia normal de fin de nivel

1. `$B2A4` (`FlagpoleSlide`): baja a Mario hasta `Player_Y = $9E`.
2. `$B2CA` (`PlayerEndLevel`): espera, suma el bonus de tiempo, pasa al siguiente área en `$B2FD`.
3. Aproximadamente 1.5 s de slide + 2 s de caminata al castillo.

### 2.4 Variantes del flagpole glitch

Nombres de la comunidad: **flagpole glitch (FPG)**, **fast flagpole glitch (FFPG)** y **half flagpole glitch**.

- El mecanismo a nivel de código se basa en que la colisión (`$DE39`) solo se registra con el hitbox estrecho y en un frame concreto; si Mario llega al asta con un subpíxel y velocidad tales que `PutPlayerOnVine` (`$DE88`) lo encaja pero la rutina de slide (`$B2A4`) no llega a bajarlo (o el `PlayerEndLevel` se dispara antes de tiempo), se salta parte o toda la animación.
- Código implicado: colisión `$DE39`, slide `$B2A4`, fin `$B2CA`, siguiente área `$B2FD`, además de los adders de encaje en `$DE24/$DE26`.
- Ahorro típico: entre ~1 s (half) y ~3 s (fast) por flagpole, según el nivel.
- ⚠️ Los valores exactos de subpíxel y ventanas de frame dependen de la posición y velocidad de Mario; se recomienda verificar con el emulador (`nesdump --script`) antes de afirmar un valor concreto.

### 2.5 Cómo hacerlo (con botones, sin bytes)

**Dónde:** cualquier mástil. Para practicar, el 1-1.

**Qué pasa:** El **FPG** salta el deslizamiento del mástil; el **FFPG** salta también la caminata al castillo (~1.5 s de slide + ~2 s de caminata según sección 2.3).

**Botones (FPG básico):**
1. Mantén **B + Derecha** para ir a máxima velocidad (2.5 px/frame, `0x28`).
2. Salta justo antes del mástil tocando la bola superior o el inicio de la asta. No necesitas ningún input extra al tocarlo, solo caer en el aire a tope de velocidad.
3. Mario queda pegado al mástil sin deslizarse normalmente.

**Qué tienes que acertar:** no es un botón, es **velocidad + altura de toque + subpíxel**. Si vas lento, desliza normal. Si vas a tope y a la altura exacta, se salta la animación.

**Cómo saber si salió:** Mario no baja por el mástil, la bandera baja directamente y el contador de tiempo sigue corriendo.

## 3. Wrong warp

- `$0750` (`AreaPointer`) es una variable global; los objetos de la fila `$0E` la escriben en `$C235-$C24D` mientras el nivel hace scroll.
- `HandlePipeEntry` (`$DEE8`) lee `$0750` al entrar en una tubería y carga el área de destino.
- Si Mario entra en la tubería antes de que el objeto `$0E` de destino se haya parseado, el juego usa un `AreaPointer` obsoleto (por ejemplo, el de una zona de warp anterior) y envía al jugador a un área no prevista.
- La posición de Mario determina el momento: colocarse unos 20 px más a la derecha de lo normal hace que la comprobación de entrada al pipe se dispare antes de la actualización de `$0750`.
- Tabla de destinos `$87F2` (3 filas de 4 bytes) indexada por `WarpZoneControl` (`$06D6`) y por la posición X de Mario con umbrales `$60`/`$A0` (`$DF16-$DF22`):

```
$87F2  04 03 02 00
       24 05 24 00
       08 07 06 00
```

- `LoadAreaPointer` en `$9C03` y `GetAreaDataAddrs` en `$9C22` consumen el puntero.
- ⚠️ El mundo/área exacto al que se llega depende del estado previo del `AreaPointer`; verificar con el emulador antes de afirmar un destino concreto.

### 3.1 Cómo hacerlo (con botones, sin bytes)

**Dónde:** 4-2, en la zona de warps de tuberías de abajo a la derecha. Normalmente esa tubería te manda a 5-1.

**Botones:**
1. Llega a la zona de warps a máxima velocidad (**mantén B + Derecha**). Salta sobre las tuberías para no perder velocidad.
2. Colócate en la tubería de tal forma que Mario quede casi a la mitad de la tubería, **~20 px a la derecha de la pared izquierda** (no en el borde izquierdo).
3. Pulsa **Abajo** para entrar.

**Cómo saber si salió:** la pantalla de `World X-1` que carga no es la esperada (suele verse 5-1 u 8-1 según el estado anterior). Si cargas el destino normal, entraste en la posición de siempre.

## 4. Minus World (mundo -1 / 36)

- Las entradas `$24` de la tabla `$87F2` son valores de relleno sin validación de destino.
- El juego interpreta `$24` como un área inexistente: el contador de mundo acaba en `$23` (35) y se muestra como **36** (o "-1").
- Activación: en el warp de 1-2, entrar en la tubería correspondiente a una entrada `$24` (con el estado adecuado del `AreaPointer`).
- Resultado: se carga un área con datos residuales; el nivel se comporta como un acuático corrupto y al terminarlo se repite indefinidamente.
- `ParseRow0e` en `$C235-$C24D` no valida el valor resultante.

### 4.1 Cómo hacerlo (con botones, sin bytes)

**Dónde:** final del 1-2, en la última tubería (la del warp).

**Botones:**
1. Ve al final del 1-2 y **no entres directamente** en la tubería.
2. Vete a la pared justo anterior a la tubería y ponte pegado a ella.
3. Mantén **Arriba + Derecha** intentando subir por el lado izquierdo de la tubería hasta quedarte a la derecha de la pared.
4. Pulsa **Abajo** para entrar.

**Cómo saber si salió:** la pantalla carga `World 36-1` (en real es `-1`) y el nivel es un acuático corrupto que se repite.

## 5. Física: aceleración, giro y fricción

Rutinas: `PlayerPhysicsSub` `$B343`, `X_Physics` `$B51C`, `MovePlayerHorizontally` `$BF11`.

Tablas:

```
$B440  MaxLeft   = d8 e8 f0        (velocidades máximas a la izquierda)
$B443  MaxRight  = 28 18 10 0c     (velocidades máximas a la derecha)
$B447  Friction  = e4 98 d0        (fricción según estado)
```

- Unidad de velocidad: 1/16 de px por frame; velocidad máxima de carrera `0x28` = 2.5 px/frame.
- `$0705` (`Player_X_MoveForce`): 1/4096 de px; se suma a la velocidad con arrastre de acarreo.
- `$0702/$0701`: acumulador de fricción (byte bajo/alto).
- Cuando la dirección de Mario (`$33`) difiere de la dirección de movimiento (`$45`), el juego duplica la fricción: `ASL $0702; ROL $0701` en `$B57F-$B588`. Por eso al girar se frena el doble de rápido.
- `ImposeFriction` en `$B5CC`; clamps en `$B5EF` y `$B60D`; valor absoluto en `$B617`.
- `MovePlayerHorizontally` en `$BF11` aplica la velocidad a la posición con acarreo.
- Consecuencia práctica: mantener B + dirección da la máxima; soltar B en el aire conserva la velocidad; la fricción doble permite cambios de dirección más rápidos de lo esperado, y el subpíxel decide en qué frame se alcanza la velocidad máxima.

### 5.1 Cómo se aplica (botones)

- **B + Derecha** = velocidad máxima. **Soltar B** en el aire conserva la velocidad.
- **Girar** (de Derecha a Izquierda) frena el doble de rápido por la fricción duplicada de `$B57F-$B588`. Esto permite cambios de dirección más rápidos de lo esperado y se usa para colocarse en la X exacta de un warp o de un flagpole.

## 6. Subpíxeles

- Posición: píxel entero + fracción. `Player_X_Speed` es 1/16 px; `MoveForce` es 1/4096 px.
- La fracción persiste entre saltos y giros y determina si el siguiente incremento cruza un píxel entero.
- Las comprobaciones de colisión usan coordenadas enteras, así que un subpíxel favorable permite rozar una pared, entrar en una tubería o agarrar el flagpole un frame antes.
- En el emulador, la posición X es un byte y la velocidad se suma con acarreo; el acumulador fraccional son los bits bajos.
- Términos de speedrun: "subpíxel" = parte fraccional; "frame rule" = rejilla de 21 frames.

## 7. 8-4: la excepción sin frame rule

- Al tocar el hacha, `HandleAxeMetatile` (`$9384`) ejecuta:

```
$9384  LDA #$00
$9386  STA $0772      ; OperMode_Task = 0
$9388  LDA #$02
$938A  STA $0770      ; OperMode = 2 (victory / auto-control)
$938C  LDA #$18
$938E  STA $0705? ... ; Player_X_Speed = $18
```

- Pone el juego en modo automático (`OperMode=2`), tarea 0 y velocidad `$18`, **sin** pasar por la pantalla intermedia ni por `ScreenTimer`.
- Por eso el final de 8-4 no está cuantizado a 21 frames: la transición de victoria evita el bucle de timers.
- Es el único nivel del juego donde el tiempo final depende del frame exacto, no del frame rule.

## 8. Otros hallazgos

- `AutoControlPlayer` en `$B0E6` / `PlayerCtrlRoutine` en `$B0E9`: el jugador se mueve solo al final del nivel.
- `ImposeFriction` y los clamps permiten "deslizarse" sobre hielo? No hay hielo en SMB1, pero la fricción es distinta según el estado (tablas indexadas por `$1D`).
- La máquina de estados del juego (`$0770` OperMode, `$0E` GameEngineSubroutine) separa claramente los modos: título, juego, victoria, game over.
- `$075F` (world) y `$0760` (area) se incrementan en `$B2FD` al pasar de área; no hay validación contra la tabla de warps.
- El timer de juego (`$07F8-$07FA`, 3 dígitos) usa el mismo `DigitsMathRoutine`; decrementa una unidad cada 21 frames (a través de `$0795-$07A3`).

## 9. Bugs corregidos en el emulador

- **Selección de pattern table**: se leía de `$2001` en lugar de `$2000` (bit 4 para fondo, bit 3 para sprites). Corregido en `src/ppu.rs`.
- **Paleta**: aplicada la paleta FCEUX/NESdev para que el cielo (`$22`) salga correcto.
- **Sprite-0 hit**: operativo (primer hit en scanline 25); necesario para el scroll.
- **CPU**: validada contra `nestest` (5260 instrucciones, 0 divergencias en opcodes legales).
- **Render de 1-1**: la anomalía de nametable en las filas 4-7 quedó resuelta; captura limpia en frame 500.

## 10. Reproducción

```powershell
# Compilar
cargo build --release

# Ejecutar el juego
.\target\release\smb-rs.exe "C:\ruta\Super Mario Bros. (World).nes" 3

# Capturar frames con input scriptado
.\target\release\nesdump.exe "ROM.nes" out --frames 500 --scale 2 --script "60:start:1,64:start:0"

# Volcar estado de RAM al final
.\target\release\nesdump.exe "ROM.nes" out --frames 500 --state

# Validar CPU
.\target\release\nesdump.exe testroms\nestest.nes out --nestest 5260
```

Formato de `--script`: `frame:boton:valor,...` con botones `a,b,select,start,up,down,left,right`. Para mantener Derecha + B desde el frame 100 y soltar en el 150:

```powershell
.\target\release\nesdump.exe "ROM.nes" out --frames 500 --scale 2 --script "100:right:1,100:b:1,150:right:0,150:b:0"
```

## 11. Referencias

- Desensamblado etiquetado (doppelganger / 1wErt3r, reorganizado por pgattic): <https://github.com/pgattic/smb1-disasm>
- TASVideos — *GameResources/NES/SuperMarioBros* (frame rules, tricks, RNG).
- SDA Knowledge Base — *Super Mario Bros.*
- Brandon's Thoughts — *Super Mario Bros. framerules*.
- Data Crystal — *Super Mario Bros.* (mapa de RAM).
- Código del emulador: `src/cpu.rs`, `src/ppu.rs`, `src/console.rs`, `src/cartridge.rs`.

## 12. Pendiente de verificación empírica

- Valores exactos de subpíxel y ventana de frames para cada variante del flagpole glitch.
- Mundo/área exacta del wrong warp según el estado previo del `AreaPointer`.
- Secuencia exacta de activación del Minus World (qué tubería y en qué frame).
- Confirmación del mapeo dígito↔tabla en el flagpole (índice 0..4 → 5000/2000/800/400/100) mediante volcado de RAM con el emulador.
- Valores exactos para walljump en agua ("swim of death"), para `Small Fiery Mario` y para `Koopa walking in mid-air` varían según subpíxel; se resumen en las secciones 13, 24 y 26.

---

# Catálogo extendido de trucos y bugs confirmados en el código

El resto de secciones (13-27) no estaban en el informe inicial. Todos se contrastaron con el desensamblado `analysis/smbdis.asm`, `analysis/playermove.asm`, `analysis/sidepipe.asm`, `analysis/vertpipe.asm` y `analysis/consts.asm`, y con la documentación de TASVideos (`GameResources/NES/SuperMarioBros`). Se indican direcciones ROM cuando se pudieron localizar por firma de bytes en `prg.bin`, y etiquetas + variables RAM en todos los casos.

## 13. Walljump (salto en pared)

**Nivel de dificultad:** fácil (el de la izquierda) a TAS (el de la derecha, con mapa custom). En consola real se puede practicar el simple.

**Qué es:** Saltas hacia una pared y el pie "engancha", pudiendo saltar otra vez y propulsarte de la pared. Si lo haces bien, trepas la pared sin tocar suelo. Es el mismo mecanismo que el "walljump" de SMB3 copiado en la página de TASVideos.

**Cómo se detecta en el código:**

- `PlayerBGCol` / `CheckSideMTiles` ($9313 aprox.) chequea los tiles laterales. Si chocas con un sólido hace `StopPlayerMove` → `ImpedePlayerMove`.
- `ImpedePlayerMove` (luego de `StopPlayerMove`, línea ~9556 en `smbdis.asm`) anula `Player_X_Speed` (`$57`) y mueve a Mario **1 px** hacia el lado contrario de la colisión (según `$00` = qué mitad chocó). Eso es la "ejección" (wall-eject).
- La clave: el juego hace el **foot check** incluso durante la ejección. Es decir, mientras te expulsan de la pared, sigue comprobando si hay suelo debajo. Si tu pie cae justo en un borde de bloque (cada 16 px) en ese frame, el juego cree que estás en el suelo y te deja saltar otra vez.

**Condiciones (TASVideos, verificadas en el código):**

1. Velocidad horizontal hacia la pared: a la derecha `Player_X_Speed > $10` (16), a la izquierda `< $F0` (240 en complemento a dos).
2. El pie debe pegar el borde de bloque (cada 16 px). Por eso el subpíxel importa: necesitas llegar con `Player_X_Position & $0F` cerca del borde.
3. Tienes que entrar **al menos 1 px** dentro de la pared. Si no entras, la ejección no ocurre.

**Botones (versión fácil, en hardware real):**

- Corre hacia la pared con **B + Derecha**, salta y mantén **Derecha** en el aire.
- Para entrar en una pared bajo tierra al salir de la tubería: mantén **Izquierda** 6-7 frames, suelta 1 frame, vuelve a mantener **Izquierda** hasta poder saltar.

**Doble walljump en la misma pared (sin caer):** con la misma velocidad y Y en cada salto. El método más rápido: salto corriendo, luego **Izquierda+Derecha** 1 frame mientras sigues dentro de la pared (sin clip total) y salto parado.

**En agua:** la física de agua es casi la misma que en aire (`PlayerPhysicsSub` / `X_Physics` no distingue salvo `SwimmingFlag` `$0704`), así que también se puede hacer walljump bajo el agua. Eso es el "**swim of death**" (bisqwit lo descubrió intentando entrar en la tubería bajo el agua lo más suave posible en SMB2J). En SMB1, el ejemplo canónico es el nivel bajo el agua donde entras la pared antes del final.

> Variables: `Player_X_Speed` `$57`, `Player_X_MoveForce` `$0705`, `Player_State` `$1D` (0 suelo, 1 salto, 2 caída, 3 trepar), `Player_CollisionBits` `$0490`, `PlayerFacingDir` `$33`, `Player_MovingDir` `$45`, `Player_X_Position` `$86` / `Player_PageLoc`.

## 14. Alternate pipe glitch (el wrong warp explicado de verdad)

**Teoría (TASVideos):** Hay **una sola variable global** que guarda a dónde llevan las entradas "pipe/vine" de la pantalla actual.

- Si estás **lejos a la derecha** de la pantalla, la variable ya está **obsoleta** → la tubería que tocas te lleva al destino anterior (ej. 4-2: usas la tubería para ir a la sala de warps porque todavía vale el destino de la vid).
- Si la pantalla está **por delante de Mario** (Mario a la izquierda), la variable ya se adelantó → puedes acceder a un destino **futuro** desde una entrada temprana (ej. 8-4: saltas la tubería de lava, das la vuelta y entras en la tubería anterior, y te lleva a la zona de agua).

En el código es `$0750` (`AreaPointer`) que `ParseRow0e` (`$C235-$C24D`) escribe al hacer scroll, y `HandlePipeEntry` (`$DEE8`) + la rutina de pipe vertical al entrar. Lo mismo pero para tuberías laterales: ver sección 17.

**Cómo mover la pantalla sin moverte tú (3 métodos de TASVideos):**

1. **Vine teleport** (sección 16) → te lleva al borde derecho sin scroll.
2. **Entrar en una pared** → la pared te transporta al borde derecho sin scroll.
3. **Chocar con el borde izquierdo de un obstáculo mientras saltas hacia atrás** (Mario mirando a la izquierda): el scroll se detiene un momento aunque sigas avanzando a la derecha. Como aceleras más rápido hacia donde no miras, úsalo saltando hacia atrás.

**Botones genéricos:** a máxima velocidad (**B + Derecha**), colócate en la X exacta y pulsa **Abajo** antes de que se actualice `$0750`. El `+20 px` del informe original es solo un ejemplo; la condición real es "antes de que el parser de fila `$0E` pase".

## 15. Pipe clip / entrar por el lado de la tubería (pipe glitch)

**Dónde se ve en SMB1:** 8-4 (tubería tras la lava) y en cualquier tubería lateral. En SMB3 es el clásico pipe glitch; en SMB1 el equivalente es entrar por el lado.

**Código:**

- `CheckSideMTiles` ($9313) → `ChkPBtm` ($9327): si `Player_State != 0` o miras a la izquierda (`PlayerFacingDir` != derecha) → `StopPlayerMove`.
- Solo si estás en suelo (`Player_State=0`) y miras a la derecha, comprueba metatiles de tubería lateral: `$6C` (pipe bottom lateral) o `$1F` (water pipe bottom). Si es uno de esos → `PipeDwnS` ($9338).
- `PlyrPipe` ($9343): pone bit `$20` en `Player_SprAttrib` (prioridad de fondo), y si `Player_X_Position & $0F != 0` pone `ChangeAreaTimer` desde `AreaChangeTimerData` (`$A0` / `$34`, según `ScreenLeft_PageLoc` `$071A`). Si el nybble bajo es 0, salta el timer.
- Luego: si `GameEngineSubroutine` es `$07` (entrada) o `$08` (control), sale; si no, pone `$02` (side pipe entry) y el `SidePipeEntry` (`sidepipe.asm`) hace `EnterSidePipe`: velocidad `$08`, etc., y `ChgAreaPipe` decrementa `ChangeAreaTimer` y pone `AltEntranceControl`.

**El glitch:** si pulsas **Abajo** justo cuando la pared te expulsa por la derecha del borde de la tubería, el juego cree que estás centrado en la tubería y te deja entrar. Necesitas precisión de **1 frame**. En TAS se usa en 8-4 tras el walljump.

**Botones (8-4):**

1. Llega al lava pit a tope (**B + Derecha**).
2. Haz walljump en la pared anterior para quedarte en el borde derecho de la tubería anterior.
3. Estando en el aire y siendo expulsado, pulsa **Abajo** en el frame exacto en que Mario está a la derecha del borde.

## 16. Vine teleporting glitch (4-2)

**Qué es:** Si tocas una vid en el borde izquierdo extremo de la pantalla, te transporta al borde derecho.

**Código (`Setup_Vine` línea 3613, `VineObjectHandler` línea 3642):**

```
Setup_Vine: Enemy_ID = VineObject, Enemy_Flag=1
            copia PageLoc/X/Y desde Block_*
            VineFlagOffset (cuál vid), VineHeight=0
VineObjectHandler (slot 5):
  - si VineHeight < $30 / $60 no sigue
  - frameCounter d1 cada 2 de 4 frames: Enemy_Y+5--, VineHeight++
  - dibuja la vid
  - si Enemy_OffscreenBits & $0C ≠ 0 → mata la vid, VineFlagOffset=VineHeight=0
  - si VineHeight >= $20 y el tile en block buffer ($04,$10) está vacío → escribe metatile $26 (climbable) en block buffer
HandleClimbing (9411) luego te agarra: Player_State=3, velocidad 0.
```

**Uso en 4-2 (TASVideos):** pega el bloque de vid, sube al techo, ve a la derecha hasta que **la mitad del bloque de vid sale de pantalla** y la vid desaparece. Vuelve a la izquierda y toca la vid: si fuiste la distancia correcta a la derecha, apareces en el borde derecho extremo. Si te equivocas, te quedas atrapado en el borde izquierdo.

**Botones:**

1. Golpea el bloque de vid en 4-2 y trepa (**Arriba**).
2. Camina a la derecha hasta que la vid se corta.
3. Vuelve a la izquierda, toca la vid en el borde izquierdo. Te teletransportas al derecho.

## 17. Going through bricks (atravesar paredes) — 5 aplicaciones

El truco base: si estás bajo un bloque de modo que **menos de la mitad** de Mario está debajo, y saltas, no golpeas el bloque: la pared te empuja lejos. Pero si en ese momento **mueves el stick hacia fuera** (lejos del bloque), el bloque **te tira hacia dentro** en vez de expulsarte. Es la inversión de `ImpedePlayerMove`.

### 17.1 Atrapar un power-up antes de que salga

Si eres rápido, puedes atrapar el champiñón/flor a través del bloque antes de que termine de salir. Además, si tienes **A** pulsado al terminar la transformación, puedes saltar otra vez inmediatamente.

### 17.2 Saltar a un sólido bajo un suelo

Necesitas velocidad de caminar (no correr: con tubería de 2 bloques como la del final de 1-2, con velocidad de correr acabas encima). Instrucciones de TASVideos:

1. Velocidad correcta: camina, no corras. Con tubería de 2 bloques, usa **Izquierda+Derecha** 1 frame para desacelerar rápido antes de saltar.
2. Posición correcta: lo más cerca posible. Si estás demasiado cerca, chocas sin ejección pixel a pixel.
3. Cuando la pared empieza a expulsarte, empieza a mover **hacia fuera** (lejos de la pared). Eso hace que te meta dentro.
4. Cuando el movimiento vertical se detiene, empieza a moverte en la dirección que quieres. Si te expulsa ahora, fallaste.

El salto más fácil: **salto mínimo (A 1 frame)** a la distancia exacta. Si mantienes A más de 1 frame o la plataforma está a menos de 2 bloques, acabas encima.

### 17.3 Atravesar una pared bajo un techo (corner clip)

**Mario pequeño:** haz walljump justo debajo del techo. Cuando tengas apoyo, salta inmediatamente. Al caer, salta otra vez. Normalmente 3-4 saltos te meten dentro. En cada salto, **empuja fuerte en dirección contraria**; si no, te expulsa.

**Mario grande:** no necesitas nada complicado: **salto agachado** (agáchate y salta) hacia la esquina y camina. Su cabeza ya está dentro del techo, así que la ejección va a la derecha. Si te agachas otra vez antes de estar dentro del todo, te expulsa a la izquierda.

### 17.4 Atravesar una pared sin techo

El más difícil. Necesitas un **subpíxel X** exacto (16 subpíxeles = 1 px). Si al saltar a una pared normalmente te expulsa sin entrar, prueba a pulsar **Izquierda 1 frame** corriendo a la derecha para bajar el subpíxel en 1, hasta que al saltar quedas parcialmente dentro. Profundidad mínima necesaria: **3 px**.

1. Salta hacia la pared, acércate lo máximo posible al borde y entra lo más profundo posible en el primer frame.
2. Cuando la pared te expulsa, mueve **hacia fuera** para que te meta dentro.
3. En cuanto caigas dentro de la pared, salta 1 frame hacia fuera para golpear el bloque de arriba dentro de la pared. Repite hasta estar tan dentro que andar hacia delante no te expulse.
4. Cuando estés dentro, muévete hacia donde quieres en cuanto caigas.

### 17.5 Trepar una pared fuera de pantalla (off-screen wall climbing)

Trivial: cuando hay un agujero a la izquierda de la pantalla, mete a Mario entre el borde izquierdo y la pared y trepa hacia arriba. No puede caerse. Sirve para llegar al techo de la mazmorra donde no hay ruta normal. Saltando repetidamente y empujando a la izquierda también puedes entrar en la pared.

**En el código:** `BlockBufferColli_Side` ($9306) chequea ambas mitades del jugador (`$00` contador, loop `SideCheckLoop`). Si el lado superior es `$6B/$6C/$6A` (pipe) va a `BHalf`. Luego `CheckSideMTiles` → `HandleClimbing` si es trepable, si no `CheckForCoinMTiles`, `ChkJumpspringMetatiles`, y al final `ChkPBtm` (sección 15). La ejección sale de `StopPlayerMove` → `ImpedePlayerMove`.

## 18. Accelerating faster (aceleración inversa)

**Qué es:** Desde velocidad 0, aceleras más rápido si das un **salto pequeño hacia atrás** y empujas en dirección contraria a donde mira Mario.

**Botones (TASVideos):**

1. Pulsa **Izquierda + Derecha** 1 frame.
2. Pulsa **A** 1 frame para un salto pequeño a la derecha.
3. Empieza a mantener **B**.
4. Al caer, mira hacia atrás 1 frame **sin B**.
5. Al frame siguiente, empieza a saltar de nuevo manteniendo **B** y ya tienes velocidad de correr.

Sirve para empezar un nivel, acelerar tras salir de tubería y para walljump. **Verificado en 1-1:** la mejor variante gana +11 px a los 90 frames (≈4.4 frames); ver sección 32.1.

**Por qué:** En `X_Physics` (`playermove.asm`, `X_Physics` línea ~285), si `Player_State=0` (suelo) se usa `Y=1`, si no `Y=0`, con ajuste por agua y por `Left_Right_Buttons != Player_MovingDir` (salto hacia atrás aumenta Y). La fricción (`ImposeFriction`, `FrictionData` `e4 98 d0`, `MaximumLeftSpeed`/`MaximumRightSpeed` de `MaxLeftXSpdData`/`MaxRightXSpdData` `28 18 10 0c`) se duplica en giro (`ASL $0702` / `ROL $0701` cuando `PlayerFacingDir != Player_MovingDir`). El salto hacia atrás invierte `MovingDir` sin invertir `FacingDir`, así que la fricción se duplica y ganas velocidad antes.

## 19. Shooting faster (disparar más rápido/inverso)

**Qué es:** Si pulsas **Izquierda + Derecha** a la vez, Mario puede disparar a la izquierda y las bolas viajan horizontalmente al **doble de velocidad**. Se nota muy agresivo comparado con las bolas que rebotan normal. En niveles de agua no ralentiza a Mario.

**Código:** `ProcFireball_Bubble` (línea 3200 aprox.) chequea `PlayerStatus >= $02`, `A_B_Buttons & B_Button`, `PreviousA_B_Buttons`, `Fireball_State`, `Player_Y_HighPos`, `CrouchingFlag`, `Player_State != $03`. `FireballXSpdData` es `.db $40, $C0` (derecha/izquierda). Si `PlayerFacingDir` vale `$03` por pulsar L+R a la vez, `dey` da índice 2... en realidad el índice se trunca a 1, pero el bug real es que `PlayerFacingDir` se queda incoherente 1 frame y `Fireball_State` pone la bola con `Player_X_Position+4` y `Fireball_X_Speed` invertida.

## 20. Stomping desde abajo + Moustache stomp

**Stomp desde abajo:** No necesitas caer sobre el enemigo. Basta con que `Player_Y_Speed` (`$9F`) esté cayendo (`>=0`) cuando tocas: `ChkForPlayerInjury` (línea ~8600) hace:

```
LDA Player_Y_Speed
BMI ChkInj       ; si subiendo, va a chequeo manual
BNE EnemyStomped ; si cayendo, stomp
ChkInj:
  LDA Enemy_ID ; Enemy_ID < $07 (Bloober - atajo)
  CLC ADC #$0C
  CMP Enemy_Y_Position,x
  BCC EnemyStomped ; si (Player_Y+12) < Enemy_Y → stomp aunque solo toque el bigote
```

**Moustache stomp:** si solo el bigote toca el enemigo por abajo y `Player_Y+12 < Enemy_Y`, se considera stomp y te salva. Útil contra koopas que parecen demasiado altos para saltar.

**Botones:** salta bajo un enemigo cayendo (mantén **A** para caer). Si tocas con el bigote por debajo, lo matas.

## 21. Enemy hitboxes y "Under the Piranha Plant"

**Hitbox intermitente:** `PlayerEnemyCollision` (línea 8508) empieza con `LDA FrameCounter; LSR; BCS NoPUp` y chequea solo **cada otro frame** (d0 de `FrameCounter` `$09`). Además `Enemy_CollisionBits` d0 y `EnemyOffscrBitsMasked`. Por eso a veces en un run chocas y en otro no con los mismos inputs: **el frame de inicio** (ej. frame 33 vs 40 en 1-1) cambia la fase de la hitbox.

**Under the Piranha Plant:** Normalmente debes agacharte bajo enemigos. Pero si tu hitbox toca la del enemigo en un frame y te agachas al siguiente, **no recibes daño**. Solo recibes daño si estás en la hitbox **2 frames consecutivos o más**. Recuerda: las hitboxes enemigas solo afectan cada otro frame. Acordarte de mantener agachado **solo 1 frame** tras rozar te salva. Funciona mejor como Mario grande.

**En el código:** `ChkForPlayerInjury` usa `StompTimer` y `InjuryTimer` (`$08` de invencibilidad tras `ForceInjury`). `Enemy_CollisionBits` d0 se pone en `HandlePECollisions`. `InjurePlayer` chequea `InjuryTimer != 0` → sale.

## 22. Walking inside enemies (caminar dentro de enemigos)

Cuando pisas un enemigo, por un momento se vuelve "amigo". Si lo pisas **desde abajo** (stomp desde abajo, sección 20), no debes preocuparte de que te mate al rebotar hacia arriba.

Puedes **entrar dentro del enemigo** justo después de pisarlo, y no te daña mientras estás dentro. En el código `EnemyStomped` hace `SetStun`, pone `Enemy_State d5 ($20)` que hace que `PlayerEnemyCollision` lo saltee (`AND #%00100000; BNE NoPECol`). Luego `Player_Y_Speed = $FD` (o `$FC` para shells) te rebota.

**Botones:** stomp desde abajo con 1 frame de retraso y quédate dentro del enemigo 1-2 frames.

## 23. Bouncing high off turtles / shells

**Cómo:** Si saltas hacia una tortuga **mientras está en el aire**, puedes pisarla **mientras subes** y seguir subiendo. La altura máxima depende del frame exacto del salto y de qué parte de la tortuga tocas; mantén **A** presionado.

**En SMB2J:** por la fuerza de rebote alta, al tocar enemigos voladores (Koopa Paratroopa) en ascenso, ganas mucha velocidad vertical y algunas veces Mario/Luigi salen disparados al cielo.

**En el código:** `EnemyStomped` pone `Player_Y_Speed = $FD` (normal) o `HandleStompedShellE` / `SBnce` pone `$FC`. `StompChainCounter` / `StompTimer` acumulan puntos de combo (`KickedShellPtsData` `0a 06 04`). `RevivalRateData` `$10 $0b` controla cuánto tarda en revivir la concha.

## 24. Jumping through hammers (saltar a través de martillos)

Bowser tira martillos desde 6-4. Aunque parezca que no hay hueco en la hilera de martillos, muy probablemente puedas pasar sin que te golpeen.

**Por qué:** `PlayerHammerCollision` (línea ~8415) hace `LDA FrameCounter; LSR; BCC ExPHC` (cada otro frame), chequea `TimerControl` / `Misc_OffscreenBits`, calcula `PlayerCollisionCore` con `Misc_Collision_Flag`, y solo entonces daña. El hitbox es muy malo: a veces puedes atravesar un martillo sin daño y a veces te golpea uno que parece que no te tocó.

**Botones:** más fácil si eres **pequeño o agachado** (hitbox más pequeño). Salta directo a través de la hilera.

## 25. Turning around quickly + Moonwalking

**Turning around rápido:** pulsa **Izquierda + Derecha** a la vez 1 frame. Mario empieza a caminar hacia atrás a la derecha. La velocidad de correr se reduce **instantáneamente** a velocidad de caminar o menos. Permite frenar de golpe donde necesites.

**Moonwalking:** si pulsas **Izquierda + Derecha** cuando estás frenando en suelo, entras en un estado especial donde Mario camina en el sitio sin más input. Es el estado que se ve en 8-4 del walkathon de HappyLee.

Mecanismo (TASVideos): mientras camina en el sitio, `Player_X_Speed` va `"1, $FF, 1, $FF..."` y nunca se detiene, así que el subpíxel va `x, x-16, x, x-16...`. Cuando `x=0`, `x-16` pasa a ser 240, así que el píxel X cambia. Mario tiembla y la pantalla empieza a hacer scroll sin más acciones. Por eso sirve para mover la pantalla sin input.

**En el código:** `OnGroundStateSub` hace `LDA Left_Right_Buttons; STA PlayerFacingDir` (si L+R, vale `$03`). `GetPlayerAnimSpeed` tiene `ProcSkid` que si `Player_XSpeedAbsolute < $0B` pone `PlayerFacingDir` como `Player_MovingDir` y anula velocidad y `MoveForce`. Ese loop crea el moonwalk.

## 26. Avoiding fireworks (evitar fuegos artificiales)

En SMB1 hay fuegos artificiales cuando terminas el nivel con el **último dígito del timer en 1, 3 o 6**. En SMB2J es cuando el último dígito de monedas coincide con el del timer.

Más de un fuego artificial pierde tiempo, así que hay que evitarlos. Ejemplo: en 8-3 del warped run, no se usa flagpole glitch para evitar 3 fuegos; en console speedrun, normalmente se espera a que el último dígito sea 2. Un solo fuego artificial normalmente no pierde tiempo, excepto en casos especiales.

**Código (`GameTimerFireworks` línea ~7640):**

```
LDA GameTimerDisplay+2 ; $07FA último dígito
CMP #$01 BEQ SetFWC  ; 1 → Y=5
CMP #$03 BEQ SetFWC  ; 3 → Y=3
CMP #$06 BEQ SetFWC  ; 6 → Y=0
LDA #$FF               ; si no, sin fuegos
SetFWC: STA FireworksCounter ($06D7); STY Enemy_State,x
```

Luego `StarFlagTaskControl` `$0746` avanza a `AwardGameTimerPoints` (resta 1 de timer cada tick y suma 50 pts), `RaiseFlagSetoffFWorks` (sube la bandera hasta `$72`), y `InitFireworks` / `RunFireworks` los lanzan cada `$20` frames.

**Botones:** si vas a terminar con timer en 1/3/6, espera 1 segundo (o 2) en el castillo/mástil hasta que sea 2/4/5 antes de tocar el mástil.

## 27. Sliding on one foot (skid) + Lip jumps + Firebar hiding

**Sliding on one foot (Mario de fuego):** truco conocido que requiere Mario de fuego. Al empezar el nivel o salir de tubería, mantén **B** y salta, y sigue manteniendo **B**. Cuando te muevas, Mario desliza en vez de correr, por un rato. Muy fácil en consola real.

**Código:** en `X_Physics`, si `Player_State=0`, `Y=1`, luego `AreaType` (agua), luego `Left_Right_Buttons != Player_MovingDir` (salto hacia atrás), luego `A_B_Buttons & B_Button` → `SetRTmr` pone `RunningTimer=$0A`. Si no, chequea `RunningTimer`. El skid es cuando `PlayerFacingDir != Player_MovingDir` y la velocidad pasa de un signo a otro.

**Lip jumps:** puedes pararte en el borde de una tubería incluso si sale una planta. Hay unos píxeles a ambos lados de la tubería donde puedes aterrizar y saltar inmediatamente. (TASVideos: "a few pixels of room at both sides of the pipe").

**Firebar hiding trick:** a veces Mario sale ileso tras parecer tocar una Firebar, porque el giro de la Firebar no es tan suave como parece. La Firebar se mueve demasiado rápido para dañar a Mario porque **no hay ningún frame renderizado donde Mario esté "dentro"** de la Firebar. `FirebarSpinSpdData` / `FirebarSpinSpeed` y `RelativeEnemyPosition` hacen que la colisión (`CheckPlayerVertical` + `PlayerCollisionCore`) solo vea cada otro frame (igual que enemigos).

**Botones:** pasa pegado a la Firebar con **B + Derecha** y sigue; si te rozas, no te golpea.

## 28. Small Fiery Mario / negative Mushroom (desync de tamaño y poder)

**Resumen (TASVideos / Wikipedia):** Internamente Mario tiene dos variables independientes:

- `PlayerSize` `$0754`: 0 = pequeño, 1 = grande (decide el sprite).
- `PlayerStatus` `$0756`: 0 = sin mejora, 1 = super, 2 = flor de fuego (decide qué power-up sale del bloque y qué pasa al ser golpeado).

Pueden desincronizarse. Encolando una colisión con el rey Koopa (el puente que se rompe) y con un enemigo en el mismo frame:

1. La colisión con Koopa te pone `PlayerStatus=0` (sin mejora).
2. La colisión con el interruptor del puente te mete en el loop de fin de nivel que hace caminar a Mario a la derecha **y nunca cambia `PlayerSize`**.

Resultados:

- Si venías naturalmente grande (`PlayerStatus=1/2`, `Size=1`), te quedas **grande con `PlayerStatus=0`**: mueres al tocar enemigo (porque `Status=0`) pero puedes romper bloques. Champiñón lo achica y no puedes romper bloques; si te golpean otra vez, crece de nuevo. Flor lo pone de fuego pero con piernas pequeñas si te mueves.
- Si venías pequeño (`Status=0, Size=0`), pareces morir pero el nivel se completa y no pierdes vida. Si venías anormalmente pequeño (`Status=1/2, Size=0`, haciéndolo dos veces), revierte a normal.

Al morir, size y status se resetean a pequeño/sin mejora sin importar cómo muriste.

Un Game Genie de 3 códigos `"Always Stay Big"` (`SZLIVO`) produce resultados similares.

**En el código:** `ForceInjury` (línea 8627): `LDX PlayerStatus; BEQ KillPlayer; STA PlayerStatus` (a pequeño), `InjuryTimer=$08`, `GameEngineSubroutine=$0A`, `Player_State=1`, `TimerControl=$FF`. `CastleBridgeObj` (línea 2809, 13 columnas, `ChainObj` → `C_ObjectRow`/`C_ObjectMetatile`) crea el puente `$89`. El orden de colisiones (`CheckForPUpCollision` → `EColl` → `HandlePECollisions` → `InjurePlayer` / `ForceInjury`) determina el desync.

**Botones:** en niveles de castillo/puntales, toca la cadena del puente y un enemigo (ej. martillo de Bowser o fuego) en el **mismo frame**. Si eras grande, quedas "Little Fiery Mario": pequeño pero con fuego y capaz de romper bloques.

## 29. Low-Gravity Mario (gravedad baja al empezar)

Cuando un nivel empieza, Mario está en un estado especial de velocidad de caída hasta que salta por primera vez. Lo recupera en cada transición de pantalla (nivel nuevo o salir de tubería). No sirve para speedrun, pero aparece en el "minimum presses" TAS.

**En el código:** `InitJS` (línea ~230 en `playermove.asm`) pone `JumpSwimTimer=$20`, `Player_YMF_Dummy=0`, `Player_Y_MoveForce=0`, `JumpOrigin_* = Player_Y_*`, `Player_State=1`. Elige `VerticalForce` de `JumpMForceData` (`$20 $20 $1e $28 $28 $0d $04`) según `Player_XSpeedAbsolute` (`$09/$10/$19/$1c` umbrales) y `FallMForceData` (`$70 $70 $60 $90 $90 $0a $09`) para `VerticalForceDown`. `PlayerYSpdData` (`$FC $FC $FC $FB $FB $FE $FF`). `DiffToHaltJump=1` siempre. `OnGroundStateSub` copia `VerticalForceDown` → `VerticalForce` (`$0709`/`$070A`). `FallingSub` hace lo mismo. `JumpSwimSub` resetea a `VerticalForceDown` si sueltas A o pasas `DiffToHaltJump` desde `JumpOrigin_Y_Position`.

El primer salto tras entrar en nivel tiene `Player_Y_MoveForce=0` y `JumpOrigin` fresco, así que la gravedad tarda más en acumular (`ImposeGravity` suma `VerticalForce` cada frame a `Player_Y_MoveForce` y a `Player_Y_Speed` con tope `$04`).

**Botones:** tras entrar en nivel o salir de tubería, haz un salto mínimo y verás que flota un poco más que el normal. Para aprovecharlo, salta desde el suelo inmediatamente al entrar (1-1 tras tubería, sobre los dos Goombas).

**Medido en 1-1:** el primer salto no es más rápido en X; saltar en el frame 0 pierde 20-35 px frente a correr 20 frames antes de saltar. No sirve para batir tiempos (ver sección 32.2).

## 30. Koopa walking in mid-air + Enemies not appearing + Whirlpools y Springs

### Koopa walking in mid-air
Normalmente, si caes sobre una concha en el aire, se gira y sigue cayendo. Pero si la concha se pisa **abajo del todo de la pantalla** (dentro de las primeras dos filas de bloques), la concha deja de caer misteriosamente. Cuando la Koopa Troopa empieza a caminar, camina **en el aire** y atraviesa obstáculos sólidos.

### Enemies, firebars, or platforms not appearing
A veces enemigos/objetos no aparecen cuando deberían. Cuando hay **5 o más sprites de objetos** en pantalla (pueden ser firebars, bolas de fuego y plataformas) ocurre. Caso bizarro en 4-4 de SMB2J: si te acercas a Bowser en el momento en que hay 5 otros sprites, **Bowser no aparece**, como si ya lo hubieras derrotado.

**Código:** solo hay **5 slots** de enemigos (`FindEmptyEnemySlot` línea 2672): `LDX #0; loop: LDA Enemy_Flag,x; BEQ Exit; INX; CPX #$05; BNE loop; Exit: carry set si llenos`. Si no hay slot libre, `bcs DrawPipe` salta el spawn. El slot 5 es especial (bandera de flagpole / fuegos / power-up). Los firebars, fireballs y plataformas comparten los mismos slots. Además `EnemyIntervalTimer` `$0796` y `FrenzyEnemyTimer` controlan el frenesí (bolas de cañón, cheeps voladores, fuegos). Si llegas con 5 ocupados, Bowser, plantas o resortes no spawnean.

### Whirlpools (remolinos) — 2-2, 7-2, etc.
`ProcessWhirlpools` (línea 3415) solo en `AreaType` agua (`!=0` sale). `WhirlpoolActivate` (línea 3449):

- centro = `LeftExtent + Length/2`, page = `Whirlpool_PageLoc + carry`.
- Cada otro frame (`FrameCounter` d0): si Mario está a la izquierda del centro, tira 1 px a la derecha; si a la derecha, 1 px a la izquierda (con `Player_CollisionBits` d0).
- Luego impone gravedad vertical `$10` con `Whirlpool_Flag=1` y max `$01` → te hunde.

**Botones:** mantén **Izquierda/Derecha** para salir del remolino; si te quedas en el centro, te succiona. Puedes salir saltando (**A**) cuando estás en el borde.

### Jumpsprings (resortes)
`ChkForLandJumpSpring` (línea 9512): si caes sobre metatile `$67/$68`, pone `VerticalForce=$70`, `JumpspringForce=$F9`, `JumpspringTimer=$03`, `JumpspringAnimCtrl=1`. `JumpspringHandler` (línea 3558):

- Si `TimerControl != 0` → solo dibuja.
- Si `AnimCtrl=0` → solo dibuja.
- Si `FrameCounter & $02` → sube 2 px, si no baja 2 px.
- `Enemy_Y = FixedYPos + Y_PosData` (`$08 $10 $08 $00`).
- Si offset >=1 y pulsas **A** con flanco ( `A_B_Buttons & PreviousA_B_Buttons == 0` ) → `JumpspringForce=$F4` (más fuerte).
- Si offset ==3 → `Player_Y_Speed = JumpspringForce`, `AnimCtrl=0`.
- Timer: si `JumpspringTimer==0` → `=4`, `AnimCtrl++`.

Y en `CheckSideMTiles` (línea 9322): si tocas un resorte por el lateral y `AnimCtrl != 0`, sales sin detenerte; si está parado, te detiene (`StopPlayerMove`).

**Botones:** para rebotar más alto, pulsa **A** en el frame en que el resorte está en el offset 1-2. Para no rebotar, no toques A.

### Staircase (escalera) 
`StaircaseHeightData` `$07 $07 $06 $05 $04 $03 $02 $01 $00`, `StaircaseRowData` `$03 $03 $04 $05 $06 $07 $08 $09 $0A`, `StaircaseObject` línea 2918: empieza con `StaircaseControl=$09`, decrementa y dibuja escalón `$61`. Para saltarte la escalera no hace falta truco: con velocidad máxima puedes subirla corriendo; si vas lento, haz walljump/pipe clip para entrar por la pared de la escalera.

## 31. Tabla rápida de memoria (añadida para verificar)

| Variable | Dirección | Uso |
|---|---|---|
| `Player_X_Speed` | `$57` | Velocidad X (1/16 px) |
| `Player_X_MoveForce` | `$0705` | Fracción X (1/4096 px) |
| `Player_X_Position` | `$86` | X entero |
| `Player_PageLoc` | `$06E?` (página) | Página de X |
| `PlayerFacingDir` | `$33` | 1 derecha, 2 izquierda |
| `Player_MovingDir` | `$45` | Dirección de movimiento |
| `Player_State` | `$1D` | 0 suelo, 1 salto, 2 caída, 3 trepar |
| `Player_Y_Speed` | `$9F` | Velocidad Y |
| `Player_Y_Position` | `$CE` | Y entero |
| `PlayerStatus` | `$0756` | 0 pequeño, 1 super, 2 fuego |
| `PlayerSize` | `$0754` | 0 pequeño, 1 grande (sprite) |
| `Player_CollisionBits` | `$0490` | Colisiones (derecha/izquierda/arriba) |
| `Player_SprAttrib` | `$03C4` | Prioridad/atributos |
| `SwimmingFlag` | `$0704` | Bajo el agua |
| `VerticalForce` | `$0709` | Fuerza vertical actual |
| `VerticalForceDown` | `$070A` | Fuerza de caída |
| `JumpSwimTimer` | `$0782` | Timer salto/nado |
| `JumpspringAnimCtrl` | `$070E` | Animación del resorte |
| `JumpspringForce` | `$06DB` | Fuerza del resorte |
| `AreaPointer` | `$0750` | Dónde lleva la tubería/vid |
| `EntrancePage` | `$0751` | Página de entrada |
| `WarpZoneControl` | `$06D6` | Cuál zona de warp (0-4) |
| `WorldNumber` | `$075F` | Mundo actual |
| `AreaNumber` | `$0760` | Área actual |
| `AreaType` | `$074E` | 0 agua, 1 suelo, 2 bajo tierra, 3 castillo... |
| `OperMode` | `$0770` | Modo general (0 título, 1 juego, 2 victoria) |
| `OperMode_Task` | `$0772` | Sub-modo |
| `GameEngineSubroutine` | `$0E` | 4 flagpole slide, 5 end, 7 entrada, 8 control, 0B muerte |
| `IntervalTimerControl` | `$077F` | Contador 21 frames |
| `ScreenTimer` | `$07A0` | Transiciones de pantalla |
| `ScreenLeft_PageLoc` | `$071A` | Página izquierda de pantalla |
| `ScrollLock` | `$0723` | Bloquea scroll |
| `FlagpoleScore` | `$010F` | Índice 0-4 de puntos |
| `FlagpoleCollisionYPos` | `$070F` | Y al tocar mástil |
| `StarFlagTaskControl` | `$0746` | Tarea de la bandera estrella |
| `FireworksCounter` | `$06D7` | Fuegos restantes |
| `Fireball_State` | `$24` | Estado bola de fuego (0 inactiva, 1 activa, 2 lanzando) |
| `Fireball_X_Speed` | `$5E` | Velocidad bola (`$40`/`$C0`) |
| `Enemy_State` | buffer `$02Bx` | Estado enemigo (d5 stun, d7 shell) |
| `Enemy_Flag` | buffer `$05Bx` | 1 si slot ocupado |
| `EnemyIntervalTimer` | `$0796` | Timer de enemigo |

---

## 32. Verificación empírica con `deepscan` (1-1)

Esta sección no es teoría: sale de ejecutar el emulador con `deepscan --mode probe` y de la ruta ganadora de `deepscan --mode global`. Los números son de la ROM `Super Mario Bros. (World).nes` (SHA-256 `f61548fd…`) y del snapshot del primer frame controlable de 1-1 (x=40, y=176).

### 32.1 Arranque: aceleración inversa (sección 18)

Cada prueba ejecuta un prefijo exacto y después mantiene `RIGHT+B` hasta el frame 90. `x` es la posición absoluta; el baseline es correr desde el frame 0.

| prueba | x a 90f | vx | max_vx | altura | aterrizaje |
|---|---:|---:|---:|---:|---:|
| `accel_lr_a_turn24` | **218** | 40 | 40 | 29 | 23 |
| `accel_lr_a_turn26` | 218 | 40 | 40 | 29 | 23 |
| `accel_lr_a_turn28` | 217 | 40 | 40 | 29 | 23 |
| `lr_then_run` | 209 | 40 | 40 | 0 | — |
| `baseline_run` | 207 | 40 | 40 | 0 | — |
| `first_jump_right_hold2` | 192 | 40 | 40 | 26 | 24 |
| `first_jump_hold26` | 127 | 40 | 40 | 65 | 50 |

**Resultado:** la mejor variante documentada gana **+11 px a los 90 frames** respecto a correr (≈4.4 frames a 2.5 px/f). No es un atajo mágico, pero es real. La secuencia medida es:

```
frame 0: L+R
frame 1: A
frames 2..23: RIGHT+B
frame 24: LEFT (sin B)
frame 25: RIGHT+B+A
después: RIGHT+B
```

La ruta ganadora usa una variante aún más simple: **L+R+B durante 1 frame** y luego `RIGHT+B`. Esa variante llega a x=212 a los 90 frames (solo 6 px por debajo de la mejor) y fue la que eligió el beam.

### 32.2 Low-gravity (sección 29)

| prueba | x a 120f | altura | aterrizaje |
|---|---:|---:|---:|
| `first_jump_h4` | 258 | 31 | 26 |
| `run20_jump_h4` | 260 | 34 | 49 |
| `first_jump_h26` | 202 | 65 | 50 |
| `run20_jump_h26` | 237 | 68 | 72 |

**Resultado:** el primer salto con gravedad baja **no es más rápido en X**; de hecho, saltar en el frame 0 pierde 20-35 px frente a correr 20 frames antes de saltar. Confirma la nota de la sección 29: sirve para el TAS de "minimum presses", no para speedrun. En 1-1 conviene correr primero y saltar cuando ya hay velocidad.

### 32.3 Moonwalk (sección 25)

El moonwalk sostenido no aparece en la ruta óptima. Solo se usa **1 frame de L+R+B al inicio** como parte del truco de aceleración; después el beam mantiene `RIGHT+B`. Mover la pantalla sin input no ayuda en 1-1 porque la cámara ya avanza con Mario.

### 32.4 Walljump (sección 13) y flagpole glitch (sección 2)

- **Walljump:** no se usa. En 1-1 las paredes útiles (tubería de 4 tiles en x≈912, escaleras de x≈2144) se superan antes con un salto corriendo a 2.5 px/f. El walljump exige chocar y perder velocidad; no compensa.
- **FPG/FFPG:** la ruta toca el mástil en **1266 frames de nivel (21.07 s a 60.0988 fps)**. Ese tiempo termina en el **frame de contacto** con el mástil, que es el que cuenta en el récord de 1-1 (21.43 s ≈ 1288 frames). El FPG/FFPG solo acelera la animación posterior (slide + caminata al castillo), así que **no mejora el tiempo de toque**. Si la categoría midiera el nivel completo hasta el castillo, el FPG sí ahorraría ~1-3 s; habría que añadir esa variante al beam y medirla aparte.

### 32.5 Resultado de la ruta

| métrica | valor |
|---|---:|
| Frames de nivel | **1266** |
| Tiempo a 60.0988 fps | **21.07 s** |
| Récord objetivo | 21.43 s (1288 frames) |
| Margen | **22 frames (0.37 s)** |
| Beam / stride / max_rollout | 8 / 8 / 40 |
| `time_weight` | 2.5 (= velocidad máxima) |
| Rollouts / nodos | 9154 / 880 |
| X final | 3161 (nybble bajo 9, ventana del mástil) |
| Estado final | `Player_State=3` (flagpole) |
| Velocidad máxima | frame 33 (40 = 2.5 px/f) |
| Fricción inversa | 44 frames (arranque + giro) |
| Moonwalk (L+R) | 1 frame (salida) |
| Saltos / aire | 36 / 937 frames |

El perfil de velocidad solo tiene un tramo lento: la aceleración inicial (frames 1-21, media 0.84 px/f). A partir del frame 33 la velocidad es 40 (2.5 px/f) hasta el mástil. La diferencia con el mínimo teórico (3121 px / 2.5 = 1248 frames) son ~18 frames, todos de la aceleración inicial. Para bajar de 1266 habría que romper la aceleración desde 0, no la física del resto del nivel.

### 32.6 Reproducir

```powershell
# Probe de trucos de arranque
.\target\release\deepscan.exe "Super Mario Bros. (World).nes" analysis\deepscan --mode probe

# Ruta global (la que batió el récord)
.\target\release\deepscan.exe "Super Mario Bros. (World).nes" analysis\deepscan --mode global `
  --beam 8 --stride 8 --max-rollout 40 --time-weight 2.5 --frames 1800

# Replay determinista del script guardado
.\target\release\deepscan.exe "Super Mario Bros. (World).nes" analysis\deepscan --mode replay `
  --script analysis\deepscan\route_script.txt
```

---

## Referencias ampliadas

- Desensamblado etiquetado (doppelganger / 1wErt3r, reorganizado por pgattic): <https://github.com/pgattic/smb1-disasm>
- TASVideos — *GameResources/NES/SuperMarioBros* (todas las secciones 13-30).
- TASVideos — *GameResources/NES/SuperMarioBros* (línea "Alternate pipe glitches", "Vine teleporting", "Going through bricks", "Accelerating faster", etc.).
- Negative Seven — *SMB1 Explained*: <https://negative-seven.github.io/smb1explained/>
- Simplistic6502 — *SMBPedia movement*: <https://simplistic6502.github.io/smb1_tll/smbpedia_movement.html>
- SDA Knowledge Base — *Super Mario Bros.*
- Brandon's Thoughts — *Super Mario Bros. framerules*.
- Data Crystal — *Super Mario Bros.* (mapa de RAM).
- Código del emulador: `src/cpu.rs`, `src/ppu.rs`, `src/console.rs`, `src/cartridge.rs`.
- Herramientas: `tools/analyze.py`, `analysis/disasm.txt`, `analysis/smbdis.asm`.

---

*Documento generado a partir del análisis estático de la ROM y del emulador `smb-rs`. Las direcciones ROM corresponden a la PRG de 32 KiB con base `$8000`. La sección 13-31 se añadió el 2026-11-08 a partir de TASVideos y de verificación en el desensamblado; los valores de subpíxel y ventanas de frames que figuran como "verificar con emulador" deben confirmarse con `nesdump --script` y volcado de RAM antes de usarlos en un TAS/RTA.*