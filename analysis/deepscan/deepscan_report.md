# deepscan — reporte de ruta 1-1

- ROM: `C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes`
- SHA-256: `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de`
- Modo: `global`
- Frames máx: 1800

## Mapa

- Columnas cartografiadas: 206
- Filas: 13 (y=32..239)
- Suelo base: fila 11
- Enemigos vistos: 16
- Obstáculos: 36
- Bandera: col Some(197)

- Pozos: 4
- Muros/tuberías/escalones: 32

## Recorrido (piloto o replay)

En `map`/`plan` son estadísticas del piloto base; en `global` son del replay determinista de la ruta encontrada por el beam.

- Frames: 1265
- X final: 3161
- Bandera: true

## Trucos detectados

- Velocidad máxima: 40 (frame 33)
- Frames a velocidad tope: 1232
- Fricción inversa: 44 frames
- Moonwalk (L+R): 1 frames
- Aire: 937 frames
- Saltos: 36
- Primer salto: frame Some(2), airtime Some(20)
- Flagpole: frame Some(1265), Y=144, score=4, ventana=sí

## Búsqueda global (beam)

- Beam: 8 | stride: 8 | max_rollout: 40 | time_weight: 2.5
- Frames de nivel: 1266 (21.07s)
- Frames totales (con boot): 1506
- X final: 3161 | Y: 144
- Bandera: true
- Rollouts: 9154
- Nodos conservados: 880

## Reproducir

```powershell
.\target\release\nesdump.exe "C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes" out_replay --frames 1506 --script (Get-Content analysis\deepscan\tricks\route_script.txt -Raw)
```

## Limitaciones

- El mapa asume niveles sin scroll vertical (1-1). Para 1-2/4-2/8-4 hay que compensar la cámara Y.
- El plan heurístico usa velocidad de carrera constante; la ejecución real acelera desde 0.
- La búsqueda global es costosa: `--beam` y `--frames` controlan el presupuesto. Un beam mayor explora más pero tarda más.
- `route_script.txt` incluye los frames de boot (START) y se puede pasar a `nesdump --script`.
