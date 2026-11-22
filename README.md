# mu-rs

Cliente y servidor de MU Online (Season 6) escritos desde cero en Rust. La referencia de
comportamiento y aspecto es el cliente Season 6 (MuMain): sus fórmulas y números se traducen tal cual.

## Estructura

| Crate | Qué es | Estado |
|---|---|---|
| `mu-data` | Lectores de los archivos del cliente: terreno (`.map`, `.att`, `.OZB`), texturas (`.OZJ`, `.OZT`), modelos (`.bmd`), objetos del mapa (`.obj`) y sus reglas de dibujo, búsqueda de caminos | Funciona con los datos S6 |
| `mu-protocol` | Protocolo de red compartido: paquetes C1–C4, Xor32, Xor3, SimpleModulus, Connect Server, entrada y login del Game Server | Cifrado verificado contra un paquete real; login sin probar aún contra un servidor real |
| `mu-client` | El juego (Bevy 0.20) | Sin red todavía (ver abajo qué tiene) |
| `mu-connect` | Connect Server: lista de servidores y redirección al Game Server | Funciona; tests TCP de punta a punta |
| `mu-launcher` | `mu-launcher` (actualiza el cliente y lo abre) y `mu-update-server` (publica la carpeta del cliente) | En uso |
| `mu-server` | Connect Server + Game Server en un programa: cuentas y personajes (`cuentas.json`), login, lista, crear, borrar, entrar al mundo, caminar y ver a los demás del mapa | Funciona; corre en la tarea "mu-rs servidor" (puertos 44405 y 55901) |

## Cliente

```
cargo run -p mu-client                    # Lorencia
cargo run -p mu-client -- --world 3       # Devias (2 = Dungeon, 4 = Noria...)
```

Qué tiene:
- Terreno con sus capas, agua que se mueve, hierba con viento, luces dinámicas en el suelo (fuegos).
- Objetos del mapa con las reglas del original (mallas aditivas, parpadeos, texturas que se
  desplazan, ocultos), animados, y techos que se desvanecen al entrar en casa.
- Pájaros/murciélagos/mariposas en bandada; invasión de dragones (`--invasion`).
- Dark Knight con armadura y espada (en la mano, o a la espalda en zona segura), que camina con
  clic, con sus habilidades (teclas 1-0) y la estela del arma.
- **Motor de efectos** (`src/fx/`) traducido de MuMain: partículas, sprites, estelas ("joints"),
  efectos con modelo y habilidades. Corre a 25 ticks/s como el original y se dibuja interpolado.
  Ver `src/fx/mod.rs`.
- Barra principal de la interfaz S6 (esferas, experiencia, botones) en la pantalla virtual 640×480.
- Cámara del original (DefaultCamera), resoluciones fijas (`resolucion=` en `mu-launcher.txt`), contador de fps (`--fps`).
- Sonido del original: efectos, ambiente y música por mapa, pasos, pájaros, dragones, habilidades.
  Los `.wav`/`.mp3` (`Data/Sound`, `Data/Music`) no vienen con MuMain: salen del cliente completo
  oficial de Webzen (MU Red 1.20.61). Sombras de personaje y bichos, luz de los modelos del original.
- Inventario del original (**V** o **I**): rejilla de 8×8 y equipo con los objetos en 3D y su brillo
  por nivel (+3 a +15), excelente y antiguo, cuadro de información, coger y dejar objetos; el equipo
  se ve en el personaje. Sin servidor no hay inventario: `--inventario-prueba` pone uno de prueba.

Controles, los del cliente original: **clic izquierdo** caminar (mantenido sigue al ratón) ·
**1-0** elegir habilidad · **clic derecho** usarla (fuera de zona segura) · **Esc** menú de sistema ·
**U** menú de ventanas.

Opciones de prueba: `--spawn x,y`, `--walk-to x,y`, `--cam ...`, `--zoom N`, `--resolucion AxB`,
`--sin-vsync`, `--invasion 1|3`, `--probar-habilidades`, `--screenshot archivo.png`.

La primera compilación de Bevy tarda (~15–20 min); después es rápida.

## Motor de efectos: cómo se traduce un tipo nuevo

1. `python tools/gen_fx_tables.py ../referencias/MuMain/src/source` regenera `src/fx/tables/`
   (constantes `BITMAP_*`/`MODEL_*`/... con sus valores exactos, texturas y modelos de efectos).
2. El código de cada tipo va en `src/fx/kinds/<familia>.rs` con `pub fn register(r: &mut Registry)`,
   registrado en `kinds/mod.rs`. Ver las reglas de traducción al principio de `src/fx/core.rs`.

## Publicar una versión para los jugadores

`.\publicar.ps1` compila el cliente y el lanzador y los copia (sin dejar archivos a medias) a
`..\dist\cliente`, que es la carpeta que sirve `mu-update-server`. Los lanzadores descargan solo lo
que cambió. La configuración de cada jugador va en `mu-launcher.txt` (las actualizaciones no la tocan).

## Connect Server

```
cargo run -p mu-connect                                          # 127.0.0.1:44405 → GS de OpenMU en 127.0.0.1:55901
cargo run -p mu-connect -- --listen 0.0.0.0:44405 --server 0=IP_PUBLICA:55901
```

## Servidor de juego

```
cargo run -p mu-server -- --data ../dist/cliente/Data --db cuentas.json --public-ip 127.0.0.1
cargo run -p mu-server --example bot -- cuenta clave Personaje [127.0.0.1:44405] [segundos]
```

Las cuentas se crean solas la primera vez que se entra (servidor privado). El cliente busca el
Connect Server en `juego=host:puerto` de `mu-launcher.txt`, o en el host de `servidor=` (el de las
actualizaciones) en el puerto 44405.

## Probar el login contra un servidor

```
cargo run -p mu-protocol --example login -- test0 test0 [127.0.0.1:44405]
```

Connect Server → lista → Game Server → saludo → login cifrado → lista de personajes. Con OpenMU debería
responder `Okay` (cuentas de prueba `test0`…`test9`, contraseña igual al usuario).

## Tests

```
cargo test -p mu-data -p mu-protocol -p mu-connect -p mu-launcher
```

## Referencias

En `../referencias/` (no forman parte de este repo):

- **MuMain**: el cliente Season 6 (C++). Referencia principal: se traducen sus fórmulas.
- **OpenMU** (MIT): servidor de pruebas y definición del protocolo.
- **muonlinejs** (MIT): lectores de formatos.
- **xulek-muonline**: cliente moderno en C# (licencia solo educativa: leer, no copiar).

## Notas

- Este servidor de desarrollo no tiene tarjeta gráfica: Bevy usa renderizado por software y no
  aguanta más de un minuto. Lo visual se comprueba en el PC del jugador con el lanzador.
- Pendiente: red (login, personajes, monstruos), resto de interfaz, sonido, resto de clases.
