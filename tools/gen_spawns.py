"""Junta los monstruos y NPC de cada mapa de tres servidores S6 y escribe los que salen en al menos dos.

    python tools/gen_spawns.py [--refs ../referencias]

Fuentes (en `referencias`):
- OpenMU: `OpenMU/src/Persistence/Initialization/Version*/Maps/*.cs` (las llamadas a `CreateMonsterSpawn`
  de `CreateNpcSpawns` y `CreateMonsterSpawns`, siguiendo la herencia entre versiones). Solo las de
  `SpawnTrigger.Automatic` o `Wandering` (las de eventos no).
- ProjectRebirthMu (S6.3): `datos-servidor-s6/rebirth/MonsterSetBase.txt`.
- Louis S6 Ep3: `datos-servidor-s6/louis/NNN - Mapa.txt`.

Una entrada es (mapa, monstruo, zona) con su cantidad; las de una casilla sola (NPC y monstruos sueltos)
cuentan cuántas hay en esa casilla. Se queda si está en dos fuentes o en las tres; la cantidad, la que
digan dos (si las tres dicen una distinta, la de OpenMU). Lo que solo dice una fuente va al informe.

Escribe `crates/mu-server/datos/MonsterSetBase.txt` (formato clásico, secciones 0, 1 y 2),
`crates/mu-server/datos/Monster.txt` (el de ProjectRebirthMu; el informe dice en qué cambia el de Louis)
y `crates/mu-server/datos/diferencias.txt`.
"""
import argparse
import pathlib
import re
from collections import Counter, defaultdict

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "mu-server" / "datos"

# Mapas normales del S6 que tiene el cliente (sin los de eventos: Blood Castle, Chaos Castle, Devil
# Square, Kalima, Illusion Temple, Doppelganger, Imperial Guardian, Kanturu 3, Raklion jefe, Castle
# Siege, Land of Trials, Crywolf y Refugio de Balgass).
MAPS = {
    0: "Lorencia", 1: "Dungeon", 2: "Devias", 3: "Noria", 4: "Lost Tower", 6: "Arena", 7: "Atlans",
    8: "Tarkan", 10: "Icarus", 33: "Aida", 37: "Kanturu 1", 38: "Kanturu 2", 41: "Barracks of Balgass",
    51: "Elbeland", 56: "Swamp of Calmness", 57: "Raklion", 62: "Santa Town", 63: "Vulcanus",
    79: "Loren Market", 80: "Karutan 1", 81: "Karutan 2",
}

SOURCES = ("openmu", "rebirth", "louis")


# ---------------------------------------------------------------------------------------------
# MonsterSetBase.txt (Rebirth y Louis)
# ---------------------------------------------------------------------------------------------

def parse_setbase(text, only_map=None):
    """[(seccion, mapa, monstruo, rango, x1, y1, x2, y2, dir, cantidad)] de las secciones 0, 1 y 2."""
    rows = []
    section = None
    for raw in text.splitlines():
        line = raw.split("//", 1)[0].strip()
        if not line:
            continue
        if line.lower() == "end":
            section = None
            continue
        parts = line.split()
        if section is None:
            if len(parts) == 1 and parts[0].lstrip("-").isdigit():
                section = int(parts[0])
            continue
        if section not in (0, 1, 2):
            continue
        try:
            v = [int(p) for p in parts]
        except ValueError:
            continue
        if section in (0, 2):
            if len(v) < 6:
                continue
            monster, map_, rng, x, y, d = v[:6]
            row = (section, map_, monster, rng, x, y, x, y, d, 1)
        else:
            if len(v) < 9:
                continue
            monster, map_, rng, x1, y1, x2, y2, d, qty = v[:9]
            row = (1, map_, monster, rng, x1, y1, x2, y2, d, qty)
        if only_map is None or row[1] == only_map:
            rows.append(row)
    return rows


def load_rebirth(refs):
    return parse_setbase((refs / "datos-servidor-s6" / "rebirth" / "MonsterSetBase.txt").read_text("latin-1"))


def load_louis(refs):
    rows = []
    for f in sorted((refs / "datos-servidor-s6" / "louis").glob("[0-9][0-9][0-9] - *.txt")):
        rows += parse_setbase(f.read_text("latin-1"), only_map=int(f.name[:3]))
    return rows


# ---------------------------------------------------------------------------------------------
# OpenMU (C#)
# ---------------------------------------------------------------------------------------------

# Direction de OpenMU: Undefined 0, West 1 ... NorthWest 8; en MonsterSetBase es una menos (-1 = al azar).
DIRECTIONS = {"Undefined": 0, "West": 1, "SouthWest": 2, "South": 3, "SouthEast": 4, "East": 5,
              "NorthEast": 6, "North": 7, "NorthWest": 8}
KEEP_TRIGGERS = {"Automatic", "Wandering"}


def body_of(text, start):
    """Lo que hay entre la llave que abre después de `start` y la que la cierra."""
    i = text.index("{", start)
    depth = 0
    for j in range(i, len(text)):
        if text[j] == "{":
            depth += 1
        elif text[j] == "}":
            depth -= 1
            if depth == 0:
                return text[i + 1:j]
    raise ValueError("llaves sin cerrar")


def calls_in(body):
    """Los argumentos de cada `this.CreateMonsterSpawn(...)`."""
    out = []
    for m in re.finditer(r"this\.CreateMonsterSpawn\(", body):
        i, depth = m.end(), 1
        j = i
        while depth:
            if body[j] == "(":
                depth += 1
            elif body[j] == ")":
                depth -= 1
            j += 1
        out.append(body[i:j - 1])
    return out


class CsMap:
    def __init__(self, key, base, number, map_number, methods):
        self.key, self.base, self.number, self.map_number, self.methods = key, base, number, map_number, methods


def load_openmu_classes(refs):
    init = refs / "OpenMU" / "src" / "Persistence" / "Initialization"
    classes = {}
    for version in ("Version075", "Version095d", "VersionSeasonSix"):
        for f in (init / version / "Maps").glob("*.cs"):
            text = f.read_text("utf-8-sig")
            m = re.search(r"internal (?:abstract )?class (\w+) : ([\w.]+)", text)
            if not m:
                continue
            name, base = m.groups()
            base = base if "." in base else f"{version}.{base}" if base != "BaseMapInitializer" else None
            if base and base.startswith("Version") and base.count(".") == 2:
                v, _, n = base.split(".")
                base = f"{v}.{n}"
            num = re.search(r"const byte Number = (\d+);", text)
            mapnum = re.search(r"override byte MapNumber => (\d+);", text)
            methods = {}
            for meth in ("CreateNpcSpawns", "CreateMonsterSpawns"):
                mm = re.search(rf"override IEnumerable<MonsterSpawnArea> {meth}\(\)", text)
                if mm:
                    body = body_of(text, mm.end())
                    methods[meth] = (calls_in(body), f"base.{meth}()" in body)
            classes[f"{version}.{name}"] = CsMap(
                f"{version}.{name}", base, int(num.group(1)) if num else None,
                int(mapnum.group(1)) if mapnum else None, methods)
    return classes


def resolve(classes, key, attr):
    c = classes.get(key)
    while c is not None:
        v = getattr(c, attr)
        if v is not None:
            return v
        c = classes.get(c.base) if c.base else None
    return None


def method_calls(classes, key, meth):
    c = classes.get(key)
    if c is None:
        return []
    if meth in c.methods:
        calls, with_base = c.methods[meth]
        return (method_calls(classes, c.base, meth) if with_base and c.base else []) + calls
    return method_calls(classes, c.base, meth) if c.base else []


def parse_call(args):
    """(monstruo, x1, y1, x2, y2, dir, cantidad) o None si es de un evento o no se entiende."""
    parts = [a.strip() for a in args.split(",")]
    m = re.fullmatch(r"this\.NpcDictionary\[(\d+)\]", parts[1]) if len(parts) > 1 else None
    if not m:
        return None
    monster = int(m.group(1))
    rest = parts[2:]
    trigger = next((a.split(".")[1] for a in rest if a.startswith("SpawnTrigger.")), "Automatic")
    if trigger not in KEEP_TRIGGERS:
        return None
    direction = next((DIRECTIONS[a.split(".")[1]] - 1 for a in rest if a.startswith("Direction.")), -1)
    nums = []
    for a in rest:
        if re.fullmatch(r"\d+", a):
            nums.append(int(a))
        else:
            break
    if len(nums) == 2:
        x, y = nums
        return monster, x, y, x, y, direction, 1
    if len(nums) >= 4:
        x1, x2, y1, y2 = nums[:4]
        qty = nums[4] if len(nums) > 4 else 1
        return monster, x1, y1, x2, y2, direction, qty
    return None


def load_openmu(refs):
    classes = load_openmu_classes(refs)
    rows = []
    for key in classes:
        if not key.startswith("VersionSeasonSix."):
            continue
        map_ = resolve(classes, key, "map_number")
        if map_ is None:
            map_ = resolve(classes, key, "number")
        if map_ not in MAPS:
            continue
        for meth, section in (("CreateNpcSpawns", 0), ("CreateMonsterSpawns", 1)):
            for args in method_calls(classes, key, meth):
                p = parse_call(args)
                if p is None:
                    continue
                monster, x1, y1, x2, y2, d, qty = p
                single = (x1, y1) == (x2, y2) and qty == 1
                sec = section if not single or section == 0 else 2
                rows.append((sec, map_, monster, 0, x1, y1, x2, y2, d, qty))
    return rows


# ---------------------------------------------------------------------------------------------
# Votación
# ---------------------------------------------------------------------------------------------

def entries(rows):
    """{(mapa, monstruo, x1, y1, x2, y2, es_zona): (cantidad, filas)}; las casillas solas suman una por fila."""
    out = {}
    for row in rows:
        section, map_, monster, rng, x1, y1, x2, y2, d, qty = row
        if map_ not in MAPS:
            continue
        area = section == 1
        key = (map_, monster, x1, y1, x2, y2, area)
        count, kept = out.get(key, (0, []))
        out[key] = (count + qty, kept + [row])
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--refs", default=str(ROOT.parent / "referencias"))
    refs = pathlib.Path(parser.parse_args().refs)

    loaded = {"openmu": load_openmu(refs), "rebirth": load_rebirth(refs), "louis": load_louis(refs)}
    by_source = {s: entries(rows) for s, rows in loaded.items()}
    names = monster_names((refs / "datos-servidor-s6" / "rebirth" / "Monster.txt").read_text("latin-1"))

    kept = []
    report = defaultdict(list)
    for key in sorted(set().union(*by_source.values())):
        have = {s: by_source[s][key] for s in SOURCES if key in by_source[s]}
        map_, monster, x1, y1, x2, y2, area = key
        what = f"{names.get(monster, '?')} ({monster}) " + (f"zona ({x1},{y1})-({x2},{y2})" if area else f"en ({x1},{y1})")
        counts = {s: c for s, (c, _) in have.items()}
        if len(have) < 2:
            only = next(iter(have))
            report[map_].append(f"  solo {only}: {what} x{counts[only]} — fuera")
            continue
        votes = Counter(counts.values()).most_common()
        if votes[0][1] >= 2:
            count = votes[0][0]
        else:
            count = counts.get("openmu", counts.get("rebirth"))
        if len(set(counts.values())) > 1:
            report[map_].append(f"  cantidad distinta: {what} {counts} — se queda {count}")
        # La fila de referencia (rango y dirección): la de Rebirth, la de Louis o la de OpenMU.
        ref_rows = next(have[s][1] for s in ("rebirth", "louis", "openmu") if s in have)
        section = 1 if area else (0 if any(r[0] == 0 for s in have for r in have[s][1]) else 2)
        kept.append((map_, section, monster, ref_rows[0], count))

    write_setbase(kept, names)
    write_report(report, loaded, kept, refs)
    (OUT / "Monster.txt").write_bytes((refs / "datos-servidor-s6" / "rebirth" / "Monster.txt").read_bytes())
    print(f"{OUT / 'MonsterSetBase.txt'}: {len(kept)} entradas")


def monster_names(text):
    names = {}
    for line in text.splitlines():
        m = re.match(r'\s*(\d+)\s+\d+\s+"([^"]*)"', line)
        if m:
            names[int(m.group(1))] = m.group(2)
    return names


def write_setbase(kept, names):
    OUT.mkdir(parents=True, exist_ok=True)
    lines = [
        "// Monstruos y NPC de los mapas normales del S6: lo que dicen al menos dos de OpenMU, ProjectRebirthMu",
        "// y Louis S6 Ep3 (tools/gen_spawns.py; las diferencias en diferencias.txt). Formato de MonsterSetBase.txt.",
        "// Direccion: 0..7 (la que va en el paquete 0x13), -1 al azar.",
    ]
    for section, title, header in (
        (0, "NPC", "//Monster  Map  Range  X    Y    Dir"),
        (1, "ZONAS", "//Monster  Map  Range  X1   Y1   X2   Y2   Dir  Count"),
        (2, "MONSTRUOS SUELTOS", "//Monster  Map  Range  X    Y    Dir"),
    ):
        lines += ["", f"// {title}", str(section), header]
        for map_, sec, monster, row, count in kept:
            if sec != section:
                continue
            _, _, _, rng, x1, y1, x2, y2, d, _ = row
            name = names.get(monster, "?")
            if section == 1:
                lines.append(f"{monster:<10} {map_:<4} {rng:<6} {x1:<4} {y1:<4} {x2:<4} {y2:<4} {d:<4} {count:<5} // {name}")
            else:
                for _ in range(count):
                    lines.append(f"{monster:<10} {map_:<4} {rng:<6} {x1:<4} {y1:<4} {d:<4} // {name}")
        lines.append("end")
    (OUT / "MonsterSetBase.txt").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def write_report(report, loaded, kept, refs):
    lines = ["Diferencias entre OpenMU, ProjectRebirthMu y Louis (tools/gen_spawns.py).", ""]
    for s, rows in loaded.items():
        lines.append(f"{s}: {sum(1 for r in rows if r[1] in MAPS)} filas en estos mapas")
    lines.append("")
    for map_ in sorted(MAPS):
        n = sum(1 for k in kept if k[0] == map_)
        lines.append(f"Mapa {map_} {MAPS[map_]}: {n} entradas")
        lines += report.get(map_, [])
        lines.append("")
    a = monster_rows((refs / "datos-servidor-s6" / "rebirth" / "Monster.txt").read_text("latin-1"))
    b = monster_rows((refs / "datos-servidor-s6" / "louis" / "Monster.txt").read_text("latin-1"))
    lines.append("Monster.txt: filas de ProjectRebirthMu distintas en Louis")
    for k in sorted(set(a) | set(b)):
        if a.get(k) != b.get(k):
            lines.append(f"  {k}: rebirth {a.get(k)}\n       louis   {b.get(k)}")
    (OUT / "diferencias.txt").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def monster_rows(text):
    rows = {}
    for line in text.splitlines():
        line = line.split("//", 1)[0].strip()
        m = re.match(r'(\d+)\s+\d+\s+"[^"]*"\s+(.*)', line)
        if m:
            rows[int(m.group(1))] = " ".join(m.group(2).split())
    return rows


if __name__ == "__main__":
    main()
