"""Writes the monster definitions and the monsters and NPCs of the regular S6 maps from OpenMU.

    python tools/gen_spawns.py [--refs ../referencias]

Source: OpenMU (latest master) in `referencias/OpenMU/src/Persistence/Initialization`, the code its Season 6
initialization (`VersionSeasonSix.DataInitialization`) runs on a new database. The classes of
`VersionSeasonSix` and the map types of its `GameMapsInitializer` are followed up their inheritance into
`Version095d` and `Version075`, taking for each method the override that runs (and the base ones it calls with
`base.Method()`). The update plug-ins (`Updates/`) are not read: on a new database OpenMU marks them as
installed without running them (`Updates/NewDatabaseUpdates.cs`), because the initialization has their data.

- Monsters: every `this.Context.CreateNew<MonsterDefinition>()` block of the executed methods, also those built
  by a helper method (its parameters take the arguments of each call) or by a `foreach` over a tuple array.
- Spawns: the `CreateMonsterSpawn` calls of `CreateNpcSpawns` and `CreateMonsterSpawns`, and the
  `MonsterSpawnLayout` tables (`FieldMonsterSpawn`) those methods select from. Only `SpawnTrigger.Automatic` and
  `Wandering` ones (not the event ones).

Writes `crates/mu-server/datos/Monster.txt` (OpenMU's `MonsterDefinition` fields, see its header) and
`crates/mu-server/datos/MonsterSetBase.txt` (classic format, sections 0, 1 and 2). Whatever it cannot read
stops it with an error instead of being guessed.
"""
import argparse
import pathlib
import re
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "crates" / "mu-server" / "datos"

# Regular S6 maps the client has (no event maps: Blood Castle, Chaos Castle, Devil Square, Kalima,
# Illusion Temple, Doppelganger, Imperial Guardian, Kanturu 3, Raklion boss, Castle Siege, Land of Trials,
# Crywolf and Balgass's Refuge).
MAPS = {
    0: "Lorencia", 1: "Dungeon", 2: "Devias", 3: "Noria", 4: "Lost Tower", 6: "Arena", 7: "Atlans",
    8: "Tarkan", 10: "Icarus", 33: "Aida", 37: "Kanturu 1", 38: "Kanturu 2", 41: "Barracks of Balgass",
    51: "Elbeland", 56: "Swamp of Calmness", 57: "Raklion", 62: "Santa Town", 63: "Vulcanus",
    79: "Loren Market", 80: "Karutan 1", 81: "Karutan 2",
}

VERSIONS = ("Version075", "Version095d", "VersionSeasonSix")

# OpenMU Direction: Undefined 0, West 1 ... NorthWest 8; MonsterSetBase uses one less (-1 = random).
DIRECTIONS = {"Undefined": 0, "West": 1, "SouthWest": 2, "South": 3, "SouthEast": 4, "East": 5,
              "NorthEast": 6, "North": 7, "NorthWest": 8}
# OpenMU's MapInitializer spawns the `Automatic` ones when the map is created; the `Wandering` ones (merchants)
# are spawned by WanderingMerchantsPlugIn at one of their spawns at a time.
KEEP_TRIGGERS = {"Automatic", "Wandering"}

# The `Stats` attributes written to Monster.txt, by column.
STAT_COLUMNS = (
    ("Level", "Level"), ("MaxHealth", "MaximumHealth"), ("MinDamage", "MinimumPhysBaseDmg"),
    ("MaxDamage", "MaximumPhysBaseDmg"), ("Defense", "DefenseBase"), ("AttackRate", "AttackRatePvm"),
    ("DefenseRate", "DefenseRatePvm"),
)
RESISTANCE_COLUMNS = (
    ("IceRes", "IceResistance"), ("PoisonRes", "PoisonResistance"),
    ("LightningRes", "LightningResistance"), ("FireRes", "FireResistance"),
)


class GenError(Exception):
    pass


# ---------------------------------------------------------------------------------------------
# C# source model
# ---------------------------------------------------------------------------------------------

def matching(text, i, open_="{", close="}"):
    """Index of the bracket that closes the one at `i`."""
    depth = 0
    for j in range(i, len(text)):
        if text[j] == open_:
            depth += 1
        elif text[j] == close:
            depth -= 1
            if depth == 0:
                return j
    raise GenError("unclosed bracket")


def split_args(s):
    """Splits on the commas that are not inside brackets (not `<>`: `=>` would close one)."""
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def call_args(text, start):
    """The arguments of the call whose `(` is at or after `start`, and the index after its `)`."""
    i = text.index("(", start)
    j = matching(text, i, "(", ")")
    return split_args(text[i + 1:j]), j + 1


class Method:
    def __init__(self, cls, name, params, body):
        self.cls, self.name, self.params, self.body = cls, name, params, body


class CsClass:
    def __init__(self, key, base):
        self.key, self.base = key, base
        self.methods = {}
        self.consts = {}
        self.texts = []


METHOD_RE = re.compile(
    r"^    (?:(?:public|protected|private|internal|override|virtual|static|new|sealed|abstract|async)\s+)+"
    r"[\w<>\[\]?.]+\s+(\w+)\(([^)]*)\)\s*\n    \{", re.M)


def strip_comments(text):
    """The code without `//` and `/* */` comments (commented-out spawns do not run)."""
    out, i, n = [], 0, len(text)
    while i < n:
        char = re.match(r"'(\\.|[^'\\])'", text[i:i + 4])
        if char:
            out.append(char.group(0))
            i += len(char.group(0))
        elif text[i] == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j == -1 else j
        elif text.startswith("/*", i):
            i = text.index("*/", i) + 2
        else:
            out.append(text[i])
            i += 1
    return "".join(out)


def load_classes(init):
    classes = {}
    for version in VERSIONS:
        for f in sorted((init / version).rglob("*.cs")):
            text = strip_comments(f.read_text("utf-8-sig"))
            ns = re.search(r"^namespace MUnique\.OpenMU\.Persistence\.Initialization\.([\w.]+);", text, re.M)
            m = re.search(r"^(?:internal|public)[\w ]* class (\w+)(?: : ([\w.]+))?", text, re.M)
            if not ns or not m:
                continue
            key = f"{ns.group(1)}.{m.group(1)}"
            c = classes.setdefault(key, CsClass(key, None))
            if m.group(2):
                c.base = resolve_base(ns.group(1), m.group(2))
            c.texts.append(text)
            for mm in METHOD_RE.finditer(text):
                brace = mm.end() - 1
                params = [p.split()[-1] for p in split_args(mm.group(2))]
                m = Method(c, mm.group(1), params, text[brace + 1:matching(text, brace)])
                m.is_override = " override " in mm.group(0)
                c.methods[mm.group(1)] = m
            for cm in re.finditer(r"(?:const|static readonly) (?:short|byte|int) (\w+) = (\d+);", text):
                c.consts[cm.group(1)] = cm.group(2)
    return classes


def resolve_base(ns, base):
    """`Version095d.Maps.Lorencia`-like key of a base class name written in namespace `ns`."""
    if base.startswith("Initialization."):
        return base.removeprefix("Initialization.")  # the root namespace, not read
    if base.split(".")[0] in VERSIONS:
        return base
    return f"{ns}.{base}"


def chain(classes, key):
    out = []
    while key in classes:
        if classes[key] in out:
            raise GenError(f"inheritance cycle at {key}")
        out.append(classes[key])
        key = classes[key].base
    return out


def effective(classes, key, name):
    """The bodies of method `name` that run on an instance of `key`, in order: the override, and what it
    calls with `base.name()` (before it, as all these overrides do it first)."""
    for c in chain(classes, key):
        if name in c.methods:
            m = c.methods[name]
            before = effective(classes, c.base, name) if f"base.{name}(" in m.body and c.base else []
            return before + [m]
    return []


def lookup_const(classes, key, name):
    for c in chain(classes, key):
        if name in c.consts:
            return c.consts[name]
    return None


# ---------------------------------------------------------------------------------------------
# Expressions
# ---------------------------------------------------------------------------------------------

class Ctx:
    def __init__(self, names, skills, classes, cls_key, env):
        self.names, self.skills, self.classes, self.cls_key, self.env = names, skills, classes, cls_key, env


def substitute(expr, env):
    for k in sorted(env, key=len, reverse=True):
        expr = re.sub(rf"(?<![\w.]){re.escape(k)}(?!\w)", env[k], expr)
    return expr


def number(expr, ctx):
    e = substitute(expr.strip(), ctx.env)
    e = re.sub(r"^\((?:short|byte|int|float)\)\s*", "", e).replace("_", "")
    if re.fullmatch(r"-?\d+(\.\d+)?f?", e):
        v = float(e.rstrip("f"))
        return int(v) if v.is_integer() else v
    if re.fullmatch(r"\w+", e):
        c = lookup_const(ctx.classes, ctx.cls_key, e)
        if c is not None:
            return int(c)
    raise GenError(f"cannot evaluate number `{expr}` ({e})")


def millis(expr, ctx):
    e = substitute(expr.strip(), ctx.env)
    if e == "TimeSpan.Zero":
        return 0
    m = re.fullmatch(r"new TimeSpan\((.+?)\s*\*\s*TimeSpan\.TicksPer(Millisecond|Second)\)", e)
    if m:
        return number(m.group(1), ctx) * (1 if m.group(2) == "Millisecond" else 1000)
    m = re.fullmatch(r"new TimeSpan\((.+)\)", e)
    if m:
        return number(m.group(1), ctx) / 10_000  # ticks
    m = re.fullmatch(r"TimeSpan\.From(Milliseconds|Seconds)\((.+)\)", e)
    if m:
        return number(m.group(2), ctx) * (1 if m.group(1) == "Milliseconds" else 1000)
    raise GenError(f"cannot evaluate TimeSpan `{expr}`")


def designation(expr, ctx):
    e = substitute(expr.strip(), ctx.env)
    m = re.fullmatch(r"LocalizedString\.FromResource\(\(\) => (\w+)\.(\w+)\)", e)
    if not m or (m.group(1), m.group(2)) not in ctx.names:
        raise GenError(f"cannot resolve name `{expr}`")
    return ctx.names[(m.group(1), m.group(2))]


def skill(expr, ctx):
    e = substitute(expr.strip(), ctx.env)
    m = re.search(r"s\.Number == \(short\)(.+)\);?$", e)
    if not m:
        raise GenError(f"cannot read the skill `{expr}`")
    s = m.group(1).strip().rstrip(")")
    if s == "null":
        return None
    s = s.removeprefix("SkillNumber.")
    if s not in ctx.skills:
        raise GenError(f"unknown skill `{expr}`")
    return ctx.skills[s]


def load_resources(init):
    names = {}
    for f in (init / "Properties").glob("*Names.resx"):
        for data in ET.parse(f).getroot().iter("data"):
            value = data.find("value")
            names[(f.stem, data.get("name"))] = value.text if value is not None else ""
    return names


def load_skills(init):
    text = (init / "Skills" / "SkillNumber.cs").read_text("utf-8-sig")
    return {m.group(1): int(m.group(2)) for m in re.finditer(r"^\s+(\w+) = (\d+),", text, re.M)}


# ---------------------------------------------------------------------------------------------
# Monster definitions
# ---------------------------------------------------------------------------------------------

CREATE_RE = re.compile(r"var (\w+) = this\.Context\.CreateNew<MonsterDefinition>\(\);")


def definition_blocks(body):
    """(variable, start, text) of each creation block: up to the end of its `{}` block or the next creation."""
    out = []
    starts = list(CREATE_RE.finditer(body))
    for n, m in enumerate(starts):
        end = starts[n + 1].start() if n + 1 < len(starts) else len(body)
        depth = 0
        for j in range(m.end(), end):
            if body[j] == "{":
                depth += 1
            elif body[j] == "}":
                depth -= 1
                if depth < 0:
                    end = j
                    break
        out.append((m.group(1), m.start(), body[m.end():end]))
    return out


def environments(method, start, executed):
    """The variable bindings the block at `start` of `method` runs with: one per loop item or helper call."""
    before = method.body[:start]
    loop = None
    for m in re.finditer(r"foreach \(var (\w+) in (\w+)\)", before):
        loop = m
    if loop is not None:
        decl = re.search(rf"\(([^)]*)\)\[\] {loop.group(2)} =\s*\{{", method.body)
        if not decl:
            raise GenError(f"{method.cls.key}.{method.name}: cannot find the array `{loop.group(2)}`")
        fields = [p.split()[-1] for p in split_args(decl.group(1))]
        items = method.body[decl.end():matching(method.body, decl.end() - 1)]
        envs = []
        for item in re.finditer(r"\(([^()]*)\)", items):
            values = split_args(item.group(1))
            envs.append({f"{loop.group(1)}.{f}": v for f, v in zip(fields, values)})
        return envs
    if not method.params:
        return [{}]
    envs = []
    for m in executed:
        if m.cls is not method.cls:
            continue  # helpers are private methods, called from their own class
        for call in re.finditer(rf"this\.{method.name}\(", m.body):
            args, _ = call_args(m.body, call.start())
            if len(args) != len(method.params):
                raise GenError(f"{method.cls.key}.{method.name}: call with {len(args)} arguments")
            envs.append(dict(zip(method.params, args)))
    return envs


def read_definition(var, block, ctx):
    # `if (attackSkill is { } skillNumber)`: the pattern variable takes the value (null skips the `if`, and
    # `skill()` reads null as none).
    for m in re.finditer(r"if \((\w+) is \{ \} (\w+)\)", block):
        if m.group(1) in ctx.env:
            ctx.env[m.group(2)] = ctx.env[m.group(1)]
    props ={m.group(1): m.group(2) for m in re.finditer(rf"\b{var}\.(\w+)\s*=\s*([^;]*);", block)}
    if "Number" not in props:
        raise GenError(f"monster without number: {block[:200]}")
    d = {
        "number": number(props["Number"], ctx),
        "name": designation(props["Designation"], ctx),
        "kind": props.get("ObjectKind", "NpcObjectKind.Monster").split(".")[-1],
        "move_range": number(props.get("MoveRange", "0"), ctx),
        "attack_range": number(props.get("AttackRange", "0"), ctx),
        "view_range": number(props.get("ViewRange", "0"), ctx),
        "move_delay": millis(props.get("MoveDelay", "TimeSpan.Zero"), ctx),
        "attack_delay": millis(props.get("AttackDelay", "TimeSpan.Zero"), ctx),
        "respawn_delay": millis(props.get("RespawnDelay", "TimeSpan.Zero"), ctx),
        "attribute": number(props.get("Attribute", "0"), ctx),
        "skill": skill(props["AttackSkill"], ctx) if "AttackSkill" in props else None,
        "stats": {},
    }
    for m in re.finditer(r"\{\s*Stats\.(\w+),\s*([^}]*?)\s*\}", block):
        d["stats"][m.group(1)] = number(m.group(2), ctx)
    if "Stats." in block and f"{var}.AddAttributes(" not in block:
        raise GenError(f"monster {d['number']}: attributes not added")
    return d


def load_monsters(classes, roots, names, skills):
    """{number: definition} in creation order, and what changes definitions after their creation."""
    monsters = {}
    executed = []
    seen = set()
    for root in roots:
        # The overrides run (the initializers call Initialize, CreateMonsters...), and what they call of their
        # class, so a helper that only an update plug-in calls does not count.
        entries = {n for c in chain(classes, root) for n, m in c.methods.items() if m.is_override}
        todo = [m for n in sorted(entries) for m in effective(classes, root, n)]
        while todo:
            m = todo.pop(0)
            if id(m) in seen:
                continue
            seen.add(id(m))
            executed.append(m)
            for call in sorted(set(re.findall(r"\bthis\.(\w+)\(", m.body))):
                todo += effective(classes, root, call)
    later = []
    for m in executed:
        blocks = definition_blocks(m.body)
        for var, start, block in blocks:
            for env in environments(m, start, executed):
                ctx = Ctx(names, skills, classes, m.cls.key, env)
                d = read_definition(var, block, ctx)
                # OpenMU's GameConfiguration.Monsters has one per number (the NpcDictionary is built from it).
                if d["number"] in monsters:
                    raise GenError(f"monster {d['number']} defined twice ({m.cls.key}.{m.name})")
                d["source"] = f"{m.cls.key}.{m.name}"
                monsters[d["number"]] = d
        # Changes to definitions outside a creation block (they are reported, not applied).
        rest = m.body
        for var, start, block in reversed(blocks):
            rest = rest.replace(block, "")
        for line in rest.splitlines():
            if re.search(r"\b\w+\.(ObjectKind|MoveRange|ViewRange|AttackRange|MoveDelay|AttackDelay|RespawnDelay|"
                         r"Attribute|AttackSkill)\s*=(?!=)|\b\w+\.AddAttributes\(", line) and "spawn" not in line:
                later.append(f"{m.cls.key}.{m.name}: {line.strip()}")
    return monsters, later


# ---------------------------------------------------------------------------------------------
# Spawns
# ---------------------------------------------------------------------------------------------

def layout(classes, key, prop="MonsterSpawnLayout"):
    """The `FieldMonsterSpawn` list `prop` as seen from class `key`: (number, monster, x1, x2, y1, y2, qty)."""
    for c in chain(classes, key):
        for text in c.texts:
            m = re.search(rf"IReadOnlyList<FieldMonsterSpawn> {prop} \{{ get; \}} =\s*\[", text)
            if not m:
                continue
            body = text[m.end():matching(text, m.end() - 1, "[", "]")]
            out = []
            for item in re.finditer(r"new\(([^)]*)\)|\.\. ([\w.]+)\.(\w+)", body):
                if item.group(1):
                    out.append(tuple(int(v) for v in split_args(item.group(1))))
                else:
                    out += layout(classes, item.group(2), item.group(3))
            return out
    raise GenError(f"{key}: no {prop}")


def parse_call(args, ctx):
    """(monster, x1, y1, x2, y2, dir, count, trigger) of a `CreateMonsterSpawn(number, def, ...)` call."""
    m = re.fullmatch(r"this\.NpcDictionary\[(\w+)\]", args[1])
    if not m:
        raise GenError(f"cannot read the spawn `{', '.join(args)}`")
    monster = number(m.group(1), ctx)
    rest = args[2:]
    trigger = next((a.split(".")[1] for a in rest if a.startswith("SpawnTrigger.")), "Automatic")
    direction = next((DIRECTIONS[a.split(".")[1]] - 1 for a in rest if a.startswith("Direction.")), -1)
    nums = []
    for a in rest:
        if a.startswith(("Direction.", "SpawnTrigger.")):
            break
        nums.append(number(a, ctx))
    if len(nums) == 2:
        x, y = nums
        return monster, x, y, x, y, direction, 1, trigger
    if len(nums) >= 4:
        x1, x2, y1, y2 = nums[:4]
        qty = nums[4] if len(nums) > 4 else 1
        if len(nums) > 5:
            raise GenError(f"unexpected spawn arguments `{', '.join(args)}`")
        return monster, x1, y1, x2, y2, direction, qty, trigger
    raise GenError(f"cannot read the spawn `{', '.join(args)}`")


def map_number(classes, key):
    for c in chain(classes, key):
        for text in c.texts:
            m = re.search(r"override byte MapNumber => (\w+);", text)
            if m:
                return number(m.group(1), Ctx({}, {}, classes, c.key, {}))
    return None


def load_spawns(classes, map_types, monsters):
    """[(map, section, spawn number, monster, x1, y1, x2, y2, dir, count, trigger)] of the regular maps."""
    rows = []
    for key in map_types:
        map_ = map_number(classes, key)
        if map_ not in MAPS:
            continue
        for meth, npc in (("CreateNpcSpawns", True), ("CreateMonsterSpawns", False)):
            for m in effective(classes, key, meth):
                ctx = Ctx({}, {}, classes, m.cls.key, {})
                found = []
                body = m.body
                if "MonsterSpawnLayout" in body:
                    # The layout tables are selected as they are (FieldMonsterSpawn: X1, X2, Y1, Y2, Quantity).
                    select = re.search(r"MonsterSpawnLayout\.Select\(spawn => this\.CreateMonsterSpawn\(spawn\.Number, "
                                       r"this\.NpcDictionary\[spawn\.MonsterNumber\], spawn\.X1, spawn\.X2, spawn\.Y1, "
                                       r"spawn\.Y2, spawn\.Quantity\)\)", body)
                    if not select:
                        raise GenError(f"{m.cls.key}.{meth}: unexpected MonsterSpawnLayout use")
                    body = body.replace(select.group(0), "")
                    for n, monster, x1, x2, y1, y2, qty in layout(classes, m.cls.key):
                        found.append((n, monster, x1, y1, x2, y2, -1, qty, "Automatic"))
                for call in re.finditer(r"this\.CreateMonsterSpawn\(", body):
                    args, _ = call_args(body, call.start())
                    found.append((number(args[0], ctx), *parse_call(args, ctx)))
                if "CreateMonsterSpawn(" in body.replace("this.CreateMonsterSpawn(", "") or (
                        not found and "yield break" not in body):
                    raise GenError(f"{m.cls.key}.{meth}: spawns this generator does not understand")
                for n, monster, x1, y1, x2, y2, d, qty, trigger in found:
                    if trigger not in KEEP_TRIGGERS:
                        continue
                    single = (x1, y1) == (x2, y2) and qty == 1
                    if monster not in monsters:
                        raise GenError(f"{m.cls.key}.{meth}: monster {monster} has no definition")
                    # Section 0 (NPC): the NPC spawns and whatever is not a monster (traps, statues...).
                    is_npc = npc or monsters[monster]["kind"] != "Monster"
                    section = (0 if is_npc else 2) if single else 1
                    rows.append((map_, section, n, monster, x1, y1, x2, y2, d, qty, trigger))
    return rows


# ---------------------------------------------------------------------------------------------
# Output
# ---------------------------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--refs", default=str(ROOT.parent / "referencias"))
    refs = pathlib.Path(parser.parse_args().refs)
    init = refs / "OpenMU" / "src" / "Persistence" / "Initialization"

    classes = load_classes(init)
    maps_init = (init / "VersionSeasonSix" / "GameMapsInitializer.cs").read_text("utf-8-sig")
    map_types = []
    for t in re.findall(r"yield return typeof\(([\w.]+)\);", maps_init):
        map_types.append(t if t.split(".")[0] in VERSIONS else f"VersionSeasonSix.Maps.{t}")
    missing = [t for t in map_types if t not in classes]
    if missing:
        raise GenError(f"map classes not found: {missing}")
    roots = [k for k in classes if k.startswith("VersionSeasonSix.")]
    roots += [t for t in map_types if t not in roots]

    monsters, later = load_monsters(classes, roots, load_resources(init), load_skills(init))
    spawns = load_spawns(classes, map_types, monsters)

    OUT.mkdir(parents=True, exist_ok=True)
    write_monsters(monsters, later)
    write_setbase(spawns, monsters)
    print(f"{OUT / 'Monster.txt'}: {len(monsters)} monsters")
    print(f"{OUT / 'MonsterSetBase.txt'}: {len(spawns)} spawns, "
          f"{sum(r[9] for r in spawns)} monsters and NPCs")
    for line in later:
        print(f"  not applied: {line}")


def fmt(v):
    return str(int(v)) if float(v).is_integer() else str(v)


def write_monsters(monsters, later):
    known = {s for _, s in STAT_COLUMNS + RESISTANCE_COLUMNS}
    others = {}
    for d in monsters.values():
        for s in d["stats"]:
            if s not in known:
                others.setdefault(s, []).append(d["number"])
    columns = ("Number", '"Name"', "Kind") + tuple(c for c, _ in STAT_COLUMNS) + (
        "MoveRange", "AttackRange", "ViewRange", "MoveDelay", "AttackDelay", "RespawnDelay", "Attribute") + tuple(
        c for c, _ in RESISTANCE_COLUMNS) + ("Skill",)
    lines = [
        "// Monster and NPC definitions of OpenMU's Season 6 initialization (tools/gen_spawns.py). Not the classic",
        "// Monster.txt layout: the columns are the fields of OpenMU's MonsterDefinition, under OpenMU's names:",
        "//   Kind         ObjectKind (NpcObjectKind: Monster, PassiveNpc, Guard, Trap, Gate, Statue, SoccerBall,",
        "//                Destructible)",
        "//   Level .. DefenseRate, IceRes .. FireRes",
        "//                Stats.Level, MaximumHealth, MinimumPhysBaseDmg, MaximumPhysBaseDmg, DefenseBase,",
        "//                AttackRatePvm, DefenseRatePvm, IceResistance, PoisonResistance, LightningResistance,",
        "//                FireResistance. OpenMU gives NPCs and some monsters no attribute: 0 here, as the",
        "//                attribute system reads a missing attribute.",
        "//   MoveRange, AttackRange, ViewRange, Attribute",
        "//                the fields of the same name (C# default 0 when not set).",
        "//   MoveDelay, AttackDelay, RespawnDelay",
        "//                in milliseconds (0 when not set).",
        "//   Skill        AttackSkill, as its SkillNumber; - for none.",
        "// The classic columns without an OpenMU field are not here: Rate, MaxMana, MagicDefense, AttackType",
        "// (OpenMU uses the Skill), ItemRate, MoneyRate and MaxItemLevel (OpenMU uses drop item groups).",
    ]
    for s, numbers in sorted(others.items()):
        lines.append(f"// Not written: Stats.{s} of {', '.join(map(str, numbers))}.")
    if later:
        lines.append("// Not applied: OpenMU changes these definitions after creating them:")
        lines += [f"//   {line}" for line in later]
    widths = (8, 34, 13) + (11,) * 7 + (12,) * 7 + (12,) * 4 + (6,)
    lines.append(" ".join(c.ljust(w) for c, w in zip(("//" + columns[0],) + columns[1:], widths)).rstrip())
    for n in sorted(monsters):
        d = monsters[n]
        stats = [fmt(d["stats"].get(s, 0)) for _, s in STAT_COLUMNS]
        res = [fmt(d["stats"].get(s, 0)) for _, s in RESISTANCE_COLUMNS]
        values = [str(n), f'"{d["name"]}"', d["kind"], *stats, str(d["move_range"]), str(d["attack_range"]),
                  str(d["view_range"]), fmt(d["move_delay"]), fmt(d["attack_delay"]), fmt(d["respawn_delay"]),
                  str(d["attribute"]), *res, "-" if d["skill"] is None else str(d["skill"])]
        lines.append(" ".join(v.ljust(w) for v, w in zip(values, widths)) + f"// {d['source']}")
    (OUT / "Monster.txt").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


def write_setbase(spawns, monsters):
    lines = [
        "// Monsters and NPCs of the regular S6 maps, from OpenMU's Season 6 initialization (tools/gen_spawns.py).",
        "// MonsterSetBase.txt format; the comment has the OpenMU spawn number. Direction: 0..7 (the one sent in",
        "// packet 0x13), -1 random. Range: 0 (OpenMU has none).",
        "// `wandering`: OpenMU spawns that merchant at only one of its wandering spawns at a time, moving it every",
        "// 60 to 180 minutes (WanderingMerchantsPlugIn).",
    ]
    for section, title, header in (
        (0, "NPC", "//Monster  Map  Range  X    Y    Dir"),
        (1, "AREAS", "//Monster  Map  Range  X1   Y1   X2   Y2   Dir  Count"),
        (2, "LONE MONSTERS", "//Monster  Map  Range  X    Y    Dir"),
    ):
        lines += ["", f"// {title}", str(section), header]
        for map_, sec, n, monster, x1, y1, x2, y2, d, count, trigger in sorted(spawns, key=lambda r: r[0]):
            if sec != section:
                continue
            note = f"// {MAPS[map_]} {n}: {monsters[monster]['name']}" + (", wandering" if trigger == "Wandering" else "")
            if section == 1:
                lines.append(f"{monster:<10} {map_:<4} {0:<6} {x1:<4} {y1:<4} {x2:<4} {y2:<4} {d:<4} {count:<5} {note}")
            else:
                lines.append(f"{monster:<10} {map_:<4} {0:<6} {x1:<4} {y1:<4} {d:<4} {note}")
        lines.append("end")
    (OUT / "MonsterSetBase.txt").write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
