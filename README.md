# mu-rs: MU Online Season 6 client & server in Rust

**English** · [Español](#español)

A MU Online Season 6 client and server written from scratch in Rust, on the
[Bevy](https://bevyengine.org) engine. The goal is to look and behave like the original game:
formulas, timings and numbers are taken from the original client logic, not guessed.

> **Built with AI.** This project is developed with the help of artificial intelligence
> ([Claude Code](https://claude.com/claude-code)). The repository ships the tooling that makes
> that workflow possible, so anyone can keep developing it the same way: [`AGENTS.md`](AGENTS.md)
> (project rules AI agents read on their own; [`CLAUDE.md`](CLAUDE.md) imports it), table generators in [`tools/`](tools) and
> test bots that log in and play against a server.

## Status

| Crate | What it is | Status |
|---|---|---|
| `mu-data` | Readers for the client files: terrain (`.map`, `.att`, `.OZB`), textures (`.OZJ`, `.OZT`), models (`.bmd`), map objects, monster/NPC tables, path finding | Works with S6 data |
| `mu-protocol` | Shared network protocol: C1–C4 packets, Xor32/Xor3, SimpleModulus, Connect Server and Game Server messages | Tested against OpenMU |
| `mu-client` | The game (Bevy 0.20) | Login, server list, character select/create, worlds, walking, other players, monsters and NPCs from the server. No combat yet |
| `mu-connect` | Stand-alone Connect Server (server list + redirect) | Works |
| `mu-server` | Connect + Game Server in one program: accounts, characters, enter world, walking, monster spawns | Early, not playable yet |
| `mu-launcher` | `mu-launcher` (updates the client and starts it) and `mu-update-server` (serves the client folder) | Works |

## Features

**Overall progress: 46 of 79 features done (58%)**

`████████████░░░░░░░░` 58%

Checked items work in the client against [OpenMU](https://github.com/MUnique/OpenMU) unless they
say otherwise; the last section is our own server. The list grows as the game does, so the
percentage measures what is listed, not the whole original game.

- **Engine and world**
  - [x] Terrain: heights, light map, texture layers, slopes, tiles without ground
  - [x] Terrain water, Atlans/Kalima seas, Vulcanus lava
  - [x] Grass with wind
  - [x] Dynamic lights on the ground (fires, effects)
  - [x] Map objects of every Season 6 map with the original drawing rules (additive meshes, glow, scrolling textures, hidden objects)
  - [x] Animated objects, roofs that fade when you walk inside
  - [x] Leaves, rain, snow, embers, sand and smoke layers per map
  - [x] Birds, bats, butterflies, fish, crows and the other ambient creatures
  - [x] Character and monster shadows, original model lighting
  - [x] Ambient sounds and music of every map
  - [x] Fixed 25 Hz game logic with interpolation (smooth at any frame rate)
  - [x] Effects engine (particles, trails, sprites, effect models)
  - [ ] All effect types of the original (many are translated, not all)
  - [x] Dragon invasion event (visual)
- **Login and characters**
  - [x] Title and loading screens
  - [x] Login screen with the original tour camera
  - [x] Server list and login with the original error messages
  - [x] Character selection, creation (7 classes) and deletion
  - [x] Options and credits windows
- **Hero**
  - [x] Click to walk and hold to follow the mouse (path finding)
  - [x] Original camera and zoom levels
  - [x] Armour, weapons and wings on the character, with their glow by level
  - [x] Sitting, leaning and healing objects, chat gestures
  - [x] Dark Knight skills with their effects (offline)
  - [ ] Skills of the other classes
  - [ ] Pets and mounts (Dark Horse, Fenrir, Dinorant...)
- **Interface**
  - [x] Main bar with life/mana/AG/SD orbs and experience
  - [x] Chat, whispers, server notices, text over heads
  - [x] Character window (C) with the original formulas
  - [x] Inventory window (V) with 3D items, tooltips and equipment
  - [x] Move window (M) and minimap (Tab)
  - [x] Names and outline on mouse over
  - [ ] Skill bar and hotkeys for potions
  - [ ] Quest window
  - [ ] Party, guild and friends windows
  - [ ] Trade and personal store
  - [ ] Chaos Machine mixes
- **Network and gameplay (against the server)**
  - [x] Connect Server, encryption (SimpleModulus, Xor32, Xor3)
  - [x] Enter the world, other players in view, walking sync
  - [x] Chat, whispers, notices, animations
  - [x] Map travel (M window, walk-in gates)
  - [x] Monsters and NPCs from the server (appear, walk, leave the view)
  - [x] Inventory from the server and moving items (confirmed by the server)
  - [x] Talking to NPCs (talk cursor, walk to the NPC, dialog opens and closes like the original)
  - [ ] NPC shops: buy, sell, repair
  - [ ] Vault
  - [ ] Quests (Sevina, Marlon...)
  - [ ] Attacking monsters, damage numbers
  - [ ] Monsters attacking, death and respawn
  - [ ] Experience, level up, stat points
  - [ ] Item drops, picking up items and zen
  - [ ] Potions and consumables
  - [ ] Party, guild, trade, duel
  - [ ] Events (Blood Castle, Devil Square, Chaos Castle...)
- **Launcher**
  - [x] Automatic updates, compressed and parallel downloads
  - [x] Self-update
  - [x] Update server (`mu-update-server`)
  - [x] Client logs uploaded to the server
- **Our server (`mu-server`)**
  - [x] Accounts and characters (JSON), login, character list, create, delete
  - [x] Enter the world, walking, players in view
  - [x] Chat, whispers, notices, Game Master `/goldnotice`
  - [x] Map travel and walk-in gates
  - [x] Monster and NPC spawns from three Season 6 data sets, monsters wandering in their area
  - [x] Connect Server
  - [ ] Database storage (instead of JSON)
  - [ ] Inventory and items with atomic operations (no duplication possible)
  - [ ] NPC shops and vault
  - [ ] Combat, experience and drops
  - [ ] Skills
  - [ ] Quests
  - [ ] Party and guild
  - [ ] Trade
  - [ ] Events
  - [ ] Character stats from the original formulas for every class
  - [ ] Game Master commands beyond notices
  - [ ] Anti-cheat checks (speed, distance, packet validation)
  - [ ] Monster AI: chasing and attacking players
  - [ ] Pets and mounts
  - [ ] Personal stores

## Building

Requirements:
- [Rust](https://rustup.rs) stable, 1.85 or newer (edition 2024).
- Windows 10/11 is what we test on. Linux and macOS should build (Bevy supports them) but are untested.
- **Your own copy of a MU Online Season 6 client `Data` folder.** No game files are included here,
  and none will be.

```sh
git clone https://github.com/Crixpx-VE/mu-rs
cd mu-rs
cargo build --release          # all programs, into target/release
cargo test                     # unit and protocol tests (no game data needed for most)
```

## Running

### 1. A server to play against

`mu-server` is not playable yet, so for testing the client we use **[OpenMU](https://github.com/MUnique/OpenMU)**:
follow its docs to start it (it needs .NET and PostgreSQL). Its "GMO client" Connect Server
listens on port `44405`, which is the client's default. OpenMU ships test accounts
(`test1`/`test1`, `testgm`/`testgm`...).

To try our own server instead: `cargo run --release -p mu-server -- --data <path to Data>`.

### 2. The client

```sh
cargo run --release -p mu-client -- --data <path to Data>
```

The client looks for `Data` in this order: `--data <path>`, the `MU_DATA` variable, a `Data`
folder next to the executable, `./Data`, and finally `../referencias/MuMain/src/bin/Data` (if you
cloned [MuMain](https://github.com/sven-n/MuMain) next to this repo).

It connects to `127.0.0.1:44405` by default. To use another server, write a `mu-launcher.txt`
next to the executable:

```
servidor=your.update.server          # mu-update-server host (the game uses the same host, port 44405)
juego=your.game.server:44405          # optional: Connect Server address, if different
```

Useful flags: `--world <n>` (start in a map without a server: 1 Lorencia, 3 Devias...),
`--resolucion <w>x<h>`, `--fps`, `--sin-vsync`.

### 3. Distributing to players (optional)

`mu-update-server --dir <client folder>` serves a client folder (port `44480`) and
`mu-launcher` downloads what changed and starts the game. `publicar.ps1` is the script we use to
build and publish; adapt the paths to your machine.

## Regenerating the tables

Monster/NPC tables, effect tables and monster spawns are generated by the Python scripts in
`tools/` from the reference projects below, and the generated files are committed, so you
**don't need the references to build**. To regenerate them, clone the references into
`../referencias/` (next to this repo) and run e.g. `python tools/gen_monsters.py`. Each script
explains which sources it reads.

## References and inspiration

| Project | Used for |
|---|---|
| [LouisEmulator/Main5.2](https://github.com/LouisEmulator/Main5.2) | Original Season 6 client source: the primary reference for behaviour, formulas and tables |
| [sven-n/MuMain](https://github.com/sven-n/MuMain) | Modernised fork of the client: second reference, names and data where Main 5.2 is incomplete |
| [MUnique/OpenMU](https://github.com/MUnique/OpenMU) | Open-source server: protocol, game rules, map spawns; our test server |
| [ProjectRebirthMu/Server](https://github.com/ProjectRebirthMu/Server) | Season 6 server data (`Monster.txt`, `MonsterSetBase.txt`) |
| [32bitsarg/MuOnline-Season6-Louis-Update31](https://github.com/32bitsarg/MuOnline-Season6-Louis-Update31) | Season 6 server data (monster spawns per map) |
| [xulek/muonline](https://github.com/xulek/muonline) | C# / MonoGame client: inspiration for structure and file formats |
| [afrokick/muonlinejs](https://github.com/afrokick/muonlinejs) | Web client: inspiration for file formats and data |
| [Bevy](https://github.com/bevyengine/bevy) | Game engine |

Monster spawns in `crates/mu-server/datos` are what at least two of OpenMU, ProjectRebirthMu and
Louis agree on (`tools/gen_spawns.py`).

## Contributing

Issues and pull requests are welcome, made by hand or with AI. Read [`AGENTS.md`](AGENTS.md) first:
it has the project rules (stay faithful to the original, cite the source of every number, never
commit game data or secrets). English is the project language for commits, docs and code comments.

## License

Licensed under either of [Apache License 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your option.

## Legal

MU Online is a trademark of Webzen Inc. This is a non-commercial fan project, not affiliated with
or endorsed by Webzen. It contains no game assets: you need your own client data. See
[LEGAL.md](LEGAL.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

---

## Español

Cliente y servidor de MU Online Season 6 escritos desde cero en Rust, con el motor
[Bevy](https://bevyengine.org). El objetivo es que se vea y se comporte como el juego original:
las fórmulas, tiempos y números salen de la lógica del cliente original, no se inventan.

> **Hecho con IA.** El proyecto se desarrolla con ayuda de inteligencia artificial
> ([Claude Code](https://claude.com/claude-code)). El repositorio incluye las herramientas que hacen
> posible esa forma de trabajar, para que cualquiera pueda seguir desarrollándolo igual:
> [`AGENTS.md`](AGENTS.md) (las reglas del proyecto, que los agentes de IA leen solos; [`CLAUDE.md`](CLAUDE.md) lo importa),
> generadores de tablas en [`tools/`](tools) y bots de prueba que entran y juegan contra un servidor.

### Estado

La lista completa de funciones y lo que falta está en [Features](#features) (en inglés).

| Crate | Qué es | Estado |
|---|---|---|
| `mu-data` | Lectores de los archivos del cliente: terreno, texturas, modelos `.bmd`, objetos del mapa, tablas de monstruos y NPC, búsqueda de caminos | Funciona con los datos S6 |
| `mu-protocol` | Protocolo de red: paquetes C1–C4, Xor32/Xor3, SimpleModulus, mensajes del Connect y Game Server | Probado contra OpenMU |
| `mu-client` | El juego (Bevy 0.20) | Login, lista de servidores, selección y creación de personaje, mapas, caminar, otros jugadores, monstruos y NPC del servidor. Sin combate todavía |
| `mu-connect` | Connect Server suelto (lista de servidores y redirección) | Funciona |
| `mu-server` | Connect + Game Server en un programa: cuentas, personajes, entrar al mundo, caminar, monstruos | Inicial, aún no jugable |
| `mu-launcher` | `mu-launcher` (actualiza el cliente y lo abre) y `mu-update-server` (sirve la carpeta del cliente) | Funciona |

### Compilar

Necesitas:
- [Rust](https://rustup.rs) estable, 1.85 o más nuevo (edición 2024).
- Probamos en Windows 10/11. Linux y macOS deberían compilar (Bevy los soporta), pero no están probados.
- **Tu propia carpeta `Data` de un cliente MU Online Season 6.** Aquí no hay archivos del juego, ni los habrá.

```sh
git clone https://github.com/Crixpx-VE/mu-rs
cd mu-rs
cargo build --release          # todos los programas, en target/release
cargo test                     # tests (la mayoría no necesita los datos del juego)
```

### Jugar

**1. Un servidor.** `mu-server` todavía no es jugable, así que para probar el cliente usamos
**[OpenMU](https://github.com/MUnique/OpenMU)**: arráncalo según su documentación (necesita .NET y
PostgreSQL). Su Connect Server del "GMO client" escucha en el puerto `44405`, el que usa el cliente
por defecto. Trae cuentas de prueba (`test1`/`test1`, `testgm`/`testgm`...).

Para probar el nuestro: `cargo run --release -p mu-server -- --data <ruta a Data>`.

**2. El cliente.**

```sh
cargo run --release -p mu-client -- --data <ruta a Data>
```

Busca `Data` en este orden: `--data <ruta>`, la variable `MU_DATA`, una carpeta `Data` junto al
ejecutable, `./Data` y por último `../referencias/MuMain/src/bin/Data` (si clonaste
[MuMain](https://github.com/sven-n/MuMain) junto al repo). Se conecta a `127.0.0.1:44405`; para otro servidor, crea un
`mu-launcher.txt` junto al ejecutable con `servidor=host` (y, si el juego está en otra
dirección, `juego=host:puerto`).

Opciones útiles: `--world <n>` (entrar a un mapa sin servidor: 1 Lorencia, 3 Devias...),
`--resolucion <ancho>x<alto>`, `--fps`, `--sin-vsync`.

**3. Repartirlo a jugadores (opcional).** `mu-update-server --dir <carpeta del cliente>` sirve la
carpeta (puerto `44480`) y `mu-launcher` baja lo que cambió y abre el juego. `publicar.ps1` es el
script que usamos para compilar y publicar; cambia las rutas a las de tu máquina.

### Regenerar las tablas

Las tablas de monstruos, NPC, efectos y apariciones las generan los scripts de `tools/` a partir
de los proyectos de referencia, y ya están en el repo: **no hacen falta las referencias para
compilar**. Para regenerarlas, clona las referencias en `../referencias/` (junto al repo) y ejecuta,
por ejemplo, `python tools/gen_monsters.py`. Cada script explica qué fuentes lee.

### Referencias e inspiración

Las de la tabla de arriba ([References and inspiration](#references-and-inspiration)): el código
del cliente original Main 5.2 (la referencia principal), MuMain (la segunda), OpenMU (protocolo,
reglas y nuestro servidor de pruebas), los datos de servidor de ProjectRebirthMu y Louis, y los
clientes xulek/muonline y muonlinejs como inspiración.

### Contribuir

Se aceptan issues y pull requests, hechos a mano o con IA. Lee antes [`AGENTS.md`](AGENTS.md): tiene
las reglas del proyecto (fidelidad al original, citar de dónde sale cada número, no subir nunca
datos del juego ni secretos). El idioma del proyecto es el inglés: commits, documentación y comentarios del código.

### Licencia

A tu elección, [Apache License 2.0](LICENSE-APACHE) o [MIT](LICENSE-MIT).

### Aviso legal

MU Online es una marca de Webzen Inc. Es un proyecto de fans sin ánimo de lucro, sin relación con
Webzen ni respaldado por ellos. No contiene archivos del juego: necesitas tus propios datos del cliente.
Ver [LEGAL.md](LEGAL.md) y [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
