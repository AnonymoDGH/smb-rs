# deepscan — reporte de ruta 1-1

- ROM: `C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes`
- SHA-256: `f61548fdf1670cffefcc4f0b7bdcdd9eaba0c226e3b74f8666071496988248de`
- Modo: `global`
- Frames máx: 1800

## Mapa

- Columnas cartografiadas: 206
- Filas: 13 (y=32..239)
- Suelo base: fila 11
- Enemigos vistos: 15
- Obstáculos: 36
- Bandera: col Some(197)

- Pozos: 4
- Muros/tuberías/escalones: 32

## Recorrido (replay determinista)

- Frames de nivel: 1425 (el nodo del beam se marca en 1426; el replay
  dispara el estado de mástil un frame antes)
- X final: 3161
- Bandera: true

## Búsqueda global (beam)

- Beam: 8 | stride: 8 | max_rollout: 40 | time_weight: 0.7
- Frames de nivel: 1426 (23.73s)
- Frames totales (con boot): 1666
- X final: 3161 | Y: 152
- Bandera: true
- Rollouts: 2868
- Nodos conservados: 477

## Reproducir

```powershell
.\target\release\nesdump.exe "C:\Users\USUARIO\Downloads\Super Mario Bros. (World)\Super Mario Bros. (World).nes" out_replay --frames 1666 --script (Get-Content out_deep_full2\route_script.txt -Raw)
```

## Limitaciones

- El mapa asume niveles sin scroll vertical (1-1). Para 1-2/4-2/8-4 hay que compensar la cámara Y.
- El plan heurístico usa velocidad de carrera constante; la ejecución real acelera desde 0.
- La búsqueda global es costosa: `--beam` y `--frames` controlan el presupuesto. Un beam mayor explora más pero tarda más.
- `route_script.txt` incluye los frames de boot (START) y se puede pasar a `nesdump --script`.
