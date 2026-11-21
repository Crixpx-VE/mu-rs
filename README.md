# mu-rs

Cliente y servidor de MU Online (Season 6) escritos desde cero en Rust.

## Estructura

| Crate | Qué es | Estado |
|---|---|---|
| `mu-data` | Lectores de los archivos del cliente: terreno (`.map`, `.att`, `.OZB`), texturas (`.OZJ`, `.OZT`), modelos (`.bmd`), objetos del mapa (`.obj`), búsqueda de caminos | Funciona con los datos S6 |
| `mu-protocol` | Protocolo de red compartido por cliente y servidor: paquetes C1–C4, Xor32, Xor3, SimpleModulus, mensajes del Connect Server | Verificado contra un paquete real del cliente oficial |
| `mu-client` | El juego (Bevy 0.20): mapa con objetos, personaje animado que camina con clic | Sin red todavía |
| `mu-connect` | Connect Server: lista de servidores y redirección al Game Server | Funciona; probado con tests TCP de punta a punta |
| `mu-server` | Game Server | Vacío (mientras tanto, OpenMU) |

## Requisitos

- Rust (stable) y, en Windows, Visual Studio Build Tools
- La carpeta `Data` de un cliente Season 6. Por defecto se usa `../referencias/MuMain/src/bin/Data`;
  se puede cambiar con `--data <ruta>` o la variable `MU_DATA`.

## Cliente

```
cargo run -p mu-client                    # Lorencia
cargo run -p mu-client -- --world 3       # Devias (2 = Dungeon, 4 = Noria...)
```

Controles: **clic izquierdo** caminar · **rueda** zoom · **Tab** cámara libre (botón derecho + WASD, Q/E, Shift).

Opciones útiles para pruebas: `--spawn x,y`, `--walk-to x,y`, `--cam x,y,alto,mx,my,ma`,
`--zoom N`, `--screenshot archivo.png` (renderiza sin ventana, captura y sale).

La primera compilación de Bevy tarda (~15–20 min); después es rápida.

## Connect Server

```
cargo run -p mu-connect                                          # 127.0.0.1:44405 → GS de OpenMU en 127.0.0.1:55901
cargo run -p mu-connect -- --listen 0.0.0.0:44405 --server 0=IP_PUBLICA:55901
```

Por defecto solo escucha en local. Abrirlo a la red es una decisión explícita (`--listen 0.0.0.0:...`).

## Tests

```
cargo test -p mu-data -p mu-protocol -p mu-connect
```

Los tests de `mu-data` usan los datos reales si existen en `../referencias/` y si no, se saltan.

## Referencias

En `../referencias/` (no forman parte de este repo):

- **OpenMU** (MIT): servidor de pruebas y definición del protocolo. Se puede traducir su código.
- **muonlinejs** (MIT): lectores de formatos.
- **xulek-muonline**: arquitectura de un cliente moderno (licencia solo educativa: leer, no copiar).
- **MuMain**: el cliente original (código de origen filtrado): solo para consultar comportamiento.

## Pendiente / notas

- Reglas de dibujo por modelo (mallas aditivas, ocultas) solo están para Lorencia; en otros mapas
  algunos objetos salen con zonas negras.
- Sin efectos (fuego, agua animada, hojas), sin niebla, sin sonido.
- En un servidor sin tarjeta gráfica (renderizado por software WARP) Bevy pierde memoria muy
  rápido y se cierra al minuto, incluso con una escena vacía. Pendiente comprobar en un PC con GPU
  que no pasa.
