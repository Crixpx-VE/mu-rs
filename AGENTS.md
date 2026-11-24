# mu-rs: guide for AI agents (and humans)

MU Online Season 6 client and server in Rust (Bevy 0.20). This file holds the project rules.
AI coding agents (Codex, Cursor, Copilot, Gemini, Claude Code via `CLAUDE.md`...) read it
automatically. The rules matter more than the code style: follow them even if a shortcut looks easier.

## 1. Be faithful to the original, never guess

- **Primary reference: Main 5.2** (original Season 6 client source,
  [LouisEmulator/Main5.2](https://github.com/LouisEmulator/Main5.2)). Formulas, timings, scales,
  animation speeds, colours and packet layouts come from there, translated as they are.
- **Secondary: MuMain** ([sven-n/MuMain](https://github.com/sven-n/MuMain)), only where Main 5.2 is
  incomplete or disabled. Say so in a comment.
- **Server behaviour: OpenMU** ([MUnique/OpenMU](https://github.com/MUnique/OpenMU)); server data
  also from ProjectRebirthMu and Louis S6 (see README).
- **Cite the source** of every non-obvious number or rule in a comment: `ZzzCharacter.cpp:13464`,
  `GMKarutan1.cpp:371`, `OpenMU GameLogic/...`. Reviewers check them.
- If no reference says what to do, **stop and say so** instead of inventing a value. A visible
  "not implemented yet" is better than a plausible guess.
- Original preprocessor flags: Main 5.2 has features compiled out (`#ifdef` with undefined macros).
  The active set is `DEFINED` in `tools/cparse.py`; extra ones are enabled there on purpose, with
  a comment saying why.

## 2. Layout

```
crates/mu-data       file readers (.bmd, .map, .att, .OZJ/.OZT...), monster/NPC tables
crates/mu-protocol   packets, encryption, client/server messages (shared by client and server)
crates/mu-client     the game (Bevy)
crates/mu-connect    stand-alone Connect Server
crates/mu-server     Connect + Game Server (early)
crates/mu-launcher   launcher and update server
tools/               Python generators (read the references, write Rust tables)
```

References live **outside** the repo, in `../referencias/<name>` (`Main5.2`, `MuMain`, `OpenMU`,
`xulek-muonline`, `muonlinejs`, `datos-servidor-s6/{rebirth,louis}`). They are never committed.

## 3. Generated files: do not edit by hand

- `crates/mu-data/src/monsters/table.rs` ← `tools/gen_monsters.py`
- `crates/mu-client/src/fx/tables/*` ← `tools/gen_fx_tables.py`
- `crates/mu-server/datos/{Monster,MonsterSetBase}.txt`, `diferencias.txt` ← `tools/gen_spawns.py`

Change the generator, run it (`python tools/<script>.py`), and check that the diff only contains
what you expected. Commit the generator and its output together.

## 4. Engine rules

- **Frame-rate independent.** The original runs its logic at 25 ticks per second. Per-tick logic
  runs on a fixed 25 Hz tick with interpolation for drawing (`crate::fx::sim::TICKS_PER_SECOND`),
  so the game looks the same at 60, 144 or 240 fps. Never scale "per frame" values by the frame
  rate.
- Network work happens on separate threads (`mu-client/src/net.rs`); systems read `FromServer`.
- New packets go in `mu-protocol` with an encode/decode test using real bytes (from the original
  or captured from OpenMU).

## 5. Build and test

```sh
cargo build --release
cargo test                                   # run the tests of the crates you touched
cargo clippy --all-targets
```

Without a server: `cargo run -p mu-client -- --world 1` (Lorencia), `--monstruos-prueba` spawns
some monsters, `--screenshot out.png` captures and exits.

Against a server (OpenMU on 127.0.0.1:44405, test account `test1`/`test1`):

```sh
cargo run -p mu-protocol --example login -- test1 test1             # login only
cargo run -p mu-server --example bot -- test1 test1 Probador1 127.0.0.1:44405 6
```

The bot logs in, enters the world with that character (creating it if needed) and prints every
packet it receives. Use it to check protocol changes without opening the game.

Debug examples in `crates/mu-data/examples` (`bmdinfo`, `models`, `attrmap`, `tex2png`...).

## 6. Never commit

- Game files: `Data/`, any `.bmd`, `.OZJ`, `.OZT`, `.wav`, `.mp3`... (you need your own client data).
- The reference sources (`../referencias`).
- Accounts (`cuentas.json`), logs, `mu-launcher.txt` (holds a server address), `.env`.
- Keys, tokens, passwords, IP addresses or host names of real servers.

`.gitignore` blocks the usual ones; still check `git diff --cached` before committing.

## 7. Style

- **English is the project language**: commit messages, docs, code comments and identifiers.
  Older code still has Spanish comments; translate them to English when you touch that code.
  In-game texts (the localisation the player sees) are not affected by this rule.
- Keep comments about *why* and *where it comes from*; the code says *what*.
- Small, focused commits. Tests for parsers, packets and game rules.
