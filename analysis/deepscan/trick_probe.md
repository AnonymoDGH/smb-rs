# deepscan probe — trucos de arranque (1-1)

Snapshot de inicio: x=40 y=176 vx=0 (boot 240 frames).

Cada prueba ejecuta un prefijo exacto y luego mantiene `RIGHT+B` hasta el frame 90. `altura` es la altura máxima sobre la Y inicial; `aterrizaje` es el frame del primer contacto con el suelo tras el salto.

| prueba | frames | x | vx | max_vx | altura | aterrizaje | flag |
|---|---:|---:|---:|---:|---:|---:|---|
| `accel_lr_a_turn24` | 90 | 218 | 40 | 40 | 29 | Some(23) | no |
| `accel_lr_a_turn26` | 90 | 218 | 40 | 40 | 29 | Some(23) | no |
| `accel_lr_a_turn28` | 90 | 217 | 40 | 40 | 29 | Some(23) | sí |
| `accel_lr_a_turn30` | 90 | 217 | 40 | 40 | 29 | Some(23) | sí |
| `accel_lr_a_turn32` | 90 | 216 | 40 | 40 | 29 | Some(23) | sí |
| `accel_lr_a_turn34` | 90 | 216 | 40 | 40 | 29 | Some(23) | sí |
| `accel_lr_a_turn36` | 90 | 216 | 40 | 40 | 29 | Some(23) | sí |
| `accel_lr_a_turn20` | 90 | 214 | 40 | 40 | 24 | Some(23) | sí |
| `accel_lr_a_turn18` | 90 | 213 | 40 | 40 | 24 | Some(23) | no |
| `lr_then_run` | 90 | 209 | 40 | 40 | 0 | None | no |
| `baseline_run` | 90 | 207 | 40 | 40 | 0 | None | no |
| `first_jump_right_hold2` | 90 | 192 | 40 | 40 | 26 | Some(24) | no |
| `first_jump_hold1` | 90 | 191 | 40 | 40 | 24 | Some(22) | no |
| `first_jump_right_hold4` | 90 | 191 | 40 | 40 | 31 | Some(26) | no |
| `accel_lr_a_turn22` | 90 | 191 | 40 | 40 | 27 | Some(23) | no |
| `first_jump_right_hold6` | 90 | 189 | 40 | 40 | 36 | Some(29) | no |
| `run20_back1_jump` | 90 | 189 | 40 | 40 | 27 | Some(46) | no |
| `first_jump_hold2` | 90 | 188 | 40 | 40 | 26 | Some(24) | no |
| `first_jump_right_hold8` | 90 | 188 | 40 | 40 | 41 | Some(31) | no |
| `run20_back2_jump` | 90 | 188 | 40 | 40 | 27 | Some(47) | no |
| `run20_back3_jump` | 90 | 188 | 40 | 40 | 24 | Some(45) | no |
| `run20_back4_jump` | 90 | 187 | 40 | 40 | 24 | Some(46) | no |
| `first_jump_right_hold12` | 90 | 186 | 40 | 40 | 49 | Some(36) | no |
| `first_jump_hold3` | 90 | 185 | 40 | 40 | 29 | Some(25) | sí |
| `first_jump_hold4` | 90 | 183 | 40 | 40 | 31 | Some(26) | sí |
| `first_jump_right_hold20` | 90 | 181 | 40 | 40 | 60 | Some(44) | no |
| `run20_back6_jump` | 90 | 181 | 40 | 40 | 24 | Some(48) | no |
| `first_jump_hold6` | 90 | 177 | 40 | 40 | 36 | Some(29) | no |
| `first_jump_right_hold26` | 90 | 175 | 40 | 40 | 65 | Some(50) | no |
| `first_jump_hold8` | 90 | 172 | 40 | 40 | 41 | Some(31) | no |
| `first_jump_hold10` | 90 | 167 | 40 | 40 | 45 | Some(34) | sí |
| `run20_back8_jump` | 90 | 158 | 40 | 40 | 24 | Some(50) | no |
| `first_jump_hold14` | 90 | 157 | 40 | 40 | 52 | Some(38) | no |
| `first_jump_hold20` | 90 | 142 | 40 | 40 | 60 | Some(44) | no |
| `first_jump_hold26` | 90 | 127 | 40 | 40 | 65 | Some(50) | no |

## Low-gravity: primer salto vs salto tras correr

| prueba | frames | x | vx | max_vx | altura | aterrizaje |
|---|---:|---:|---:|---:|---:|---:|
| `first_jump_h4` | 120 | 258 | 40 | 40 | 31 | Some(26) |
| `run20_jump_h4` | 120 | 260 | 40 | 40 | 34 | Some(49) |
| `first_jump_h8` | 120 | 247 | 40 | 40 | 41 | Some(31) |
| `run20_jump_h8` | 120 | 255 | 40 | 40 | 43 | Some(54) |
| `first_jump_h14` | 120 | 232 | 40 | 40 | 52 | Some(38) |
| `run20_jump_h14` | 120 | 248 | 40 | 40 | 54 | Some(61) |
| `first_jump_h26` | 120 | 202 | 40 | 40 | 65 | Some(50) |
| `run20_jump_h26` | 120 | 237 | 40 | 40 | 68 | Some(72) |

## Exhaustivo: 2 frames + correr (hasta 60f)

| prueba | frames | x | vx | max_vx | altura | aterrizaje | flag |
|---|---:|---:|---:|---:|---:|---:|---|
| `pre_LRB_A` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LRB_RA` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LRB_RBA` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LR_A` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LR_RA` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LR_RBA` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `lr_ra_documented` | 60 | 142 | 40 | 40 | 24 | Some(23) | no |
| `pre_LRB_RB` | 60 | 134 | 40 | 40 | 0 | None | sí |
| `pre_LR_RB` | 60 | 134 | 40 | 40 | 0 | None | sí |
| `pre_L_RA` | 60 | 133 | 40 | 40 | 24 | Some(23) | no |
| `pre_L_RBA` | 60 | 133 | 40 | 40 | 24 | Some(23) | no |
| `pre_LB_RA` | 60 | 133 | 40 | 40 | 24 | Some(23) | no |
| `pre_LB_RBA` | 60 | 133 | 40 | 40 | 24 | Some(23) | no |
| `pre_R_RB` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_RB_RB` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LRB_R` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LRB_LRB` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LRB_LR` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LR_R` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LR_LRB` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_LR_LR` | 60 | 132 | 40 | 40 | 0 | None | no |
| `pre_0_LRB` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_0_LR` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_B_LRB` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_B_LR` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_R_R` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_RB_R` | 60 | 131 | 40 | 40 | 0 | None | no |
| `pre_R_LRB` | 60 | 130 | 40 | 40 | 0 | None | no |
| `pre_R_LR` | 60 | 130 | 40 | 40 | 0 | None | no |
| `pre_L_A` | 60 | 130 | 40 | 40 | 24 | Some(23) | no |
