"""Generates the monster and NPC table from the Main 5.2 code (read only).

    python tools/gen_monsters.py [path to "Source Main 5.2/source"] [path to MuMain/src/bin/Data]

Walks (with a minimal C interpreter, `cparse.py`/`cexpr.py`):
- `CreateMonster` (ZzzCharacter.cpp:12577): first each map's creators in order, then the big
  `switch(Type)`. For each server monster number it gets the model, scale, weapons, `Level`,
  `BlendMesh`... and, if it changes per map or with `rand()`, one entry per case.
- `OpenMonsterModel` (ZzzOpenData.cpp:2367): `PlaySpeed` and `Loop` of each action, `BoneHead`,
  `StreamMesh` and the sounds of each `MonsterNN.bmd` model.
- `OpenNpc` (ZzzOpenData.cpp:1865): file, texture folder and `PlaySpeed` of each `MODEL_*` model.
- `CreateCharacterPointer` (ZzzCharacter.cpp:11540): weapon bones (`LinkBone`).

Names (the original reads them from `NpcName_<language>.txt`, which is not in our Data) come from MuMain's
`Monsters/Monsters.json` (Spanish if present, else English). That file is also used to compare model,
scale and weapons against MuMain's conversion and report differences (Main 5.2 wins).

Writes crates/mu-data/src/monsters/table.rs.
"""

import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(__file__))
from cparse import read_source, function_body, text_of  # noqa: E402
from cexpr import Env, Sym, Unknown, WorldDependent, RandomDependent, evaluate, MONSTER_ACTIONS  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
SRC = ARGS[0] if len(ARGS) > 0 else os.path.join(HERE, "..", "..", "referencias", "Main5.2", "Source Main 5.2", "source")
MUMAIN_DATA = ARGS[1] if len(ARGS) > 1 else os.path.join(HERE, "..", "..", "referencias", "MuMain", "src", "bin", "Data")
OUT = os.path.join(HERE, "..", "crates", "mu-data", "src", "monsters", "table.rs")

WORLDS = list(range(0, 82))


def src(name):
    return read_source(os.path.join(SRC, name))


# --- Interpreter ----------------------------------------------------------------------------------

class Break(Exception):
    pass


class Return(Exception):
    def __init__(self, value):
        self.value = value


class State:
    """What ends up in `c` / `c->Object` (only what the code sets explicitly)."""

    def __init__(self, file):
        self.file = file
        self.fields = {}
        self.notes = []
        self.created = False
        self.case_line = None


ALIASES = [
    (r"\b(?:c|pCharacter|pChar|pMonster)->Object\.", "O."),
    (r"\b(?:o|pObject|pObj)->", "O."),
    (r"\b(?:c|pCharacter|pChar|pMonster)->", "C."),
]


def compact(toks):
    s = "".join(t for t, _ in toks)
    for pat, rep in ALIASES:
        s = re.sub(pat, rep, s)
    return s


def split_args(toks):
    """Arguments of the call `f(...)` whose tokens are `toks` (starting at the name)."""
    i = [t for t, _ in toks].index("(")
    depth, args, cur = 0, [], []
    for t in toks[i:]:
        if t[0] == "(":
            depth += 1
            if depth == 1:
                continue
        elif t[0] == ")":
            depth -= 1
            if depth == 0:
                if cur:
                    args.append(cur)
                break
        if t[0] == "," and depth == 1:
            args.append(cur)
            cur = []
            continue
        cur.append(t)
    return args


class Interp:
    def __init__(self, env, st, functions):
        self.env, self.st, self.functions = env, st, functions

    def run(self, node):
        k = node.kind
        if k == "block":
            for x in node.items:
                self.run(x)
        elif k in ("case", "default", "empty"):
            pass
        elif k == "break":
            raise Break()
        elif k == "return":
            raise Return(text_of(node.expr))
        elif k == "if":
            try:
                v = evaluate(node.cond, self.env)
            except Unknown as e:
                self.st.notes.append(f"{self.st.file}:{node.line} condición sin evaluar `{text_of(node.cond)}` ({e}): se toma la rama else")
                v = False
            if v:
                self.run(node.then)
            elif node.other is not None:
                self.run(node.other)
        elif k == "switch":
            self.switch(node)
        elif k == "for":
            self.for_loop(node)
        elif k == "while":
            self.st.notes.append(f"{self.st.file}:{node.line} bucle sin traducir")
        elif k == "stmt":
            try:
                self.stmt(node)
            except Unknown as e:
                self.st.notes.append(f"{self.st.file}:{node.line} `{text_of(node.toks)}` ({e})")

    def switch(self, node):
        value = evaluate(node.expr, self.env)
        start = None
        for i, x in enumerate(node.items):
            if x.kind == "case" and evaluate(x.expr, self.env) == value:
                start = i
                break
        if start is None:
            start = next((i for i, x in enumerate(node.items) if x.kind == "default"), None)
        if start is None:
            return
        self.st.case_line = node.items[start].line
        try:
            for x in node.items[start:]:
                self.run(x)
        except Break:
            pass

    def for_loop(self, node):
        parts, cur = [], []
        for t in node.head:
            if t[0] == ";":
                parts.append(cur)
                cur = []
            else:
                cur.append(t)
        parts.append(cur)
        init, cond, step = parts
        init = [t for t in init if t[0] not in ("int",)]
        var = init[0][0]
        try:
            self.env.vars[var] = evaluate(init[2:], self.env)
            n = 0
            while evaluate(cond, self.env) and n < 64:
                self.run(node.body)
                self.env.vars[var] += 1 if "++" in [t for t, _ in step] else -1
                n += 1
        except Unknown as e:
            self.st.notes.append(f"{self.st.file}:{node.line} bucle sin evaluar `{text_of(node.head)}` ({e})")

    def ev(self, toks):
        return evaluate(toks, self.env)

    def set(self, key, value, line):
        self.st.fields[key] = (value, line)

    def stmt(self, node):
        toks = node.toks
        s = compact(toks)
        line = node.line
        names = [t for t, _ in toks]
        st = self.st
        # Pointer declarations that do not matter.
        if re.match(r"^(OBJECT|BMD|CHARACTER)\*\w+=&?(C\.Object|&?\w+)$", s) or re.match(r"^O=&C\.Object$", s):
            return
        if re.match(r"^[oO]=&", s) or s.startswith("OBJECT*") and "CreateCharacter" not in s:
            return
        # `o->Position[2] = RequestTerrainHeight(o->Position[0],o->Position[1]) + k`: k above the ground.
        m = re.match(r"^O\.Position\[2\]=RequestTerrainHeight\(O\.Position\[0\],O\.Position\[1\]\)\+(.+)$", s)
        if m:
            self.set("height", evaluate([m.group(1)], self.env), line)
            return
        if "CreateCharacter" in names:
            i = names.index("CreateCharacter")
            args = split_args(toks[i:])
            st.fields["model"] = (self.ev(args[1]), line)
            st.created = True
            if len(args) > 4:
                st.fields["rotation"] = (self.ev(args[4]), line)
            # Offset tile (`PositionX+1`...).
            for k, name in ((2, "tile_dx"), (3, "tile_dy")):
                a = [t for t, _ in args[k]]
                if len(a) == 3 and a[1] in "+-":
                    st.fields[name] = ((1 if a[1] == "+" else -1) * int(a[2]), line)
            return
        m = re.match(r"^(?:CHARACTER\*)?(\w+)=g_(\w+)\.CreateMonster\(", s)
        if m:
            fn = self.functions[m.group(2)]
            try:
                self.run(fn)
            except Return:
                pass
            except Break:
                pass
            self.env.vars[m.group(1)] = 1 if st.created else 0
            return
        if names[0] in ("strcpy", "::") or s.startswith("::strcpy") or s.startswith("wsprintf"):
            return
        if names[0] == "OpenMonsterModel" or names[0] == "OpenNpc":
            st.fields["open"] = (self.ev(split_args(toks)[0]), line)
            return
        if names[0] == "SetCharacterScale":
            # In-game `SetCharacterScale` (ZzzCharacter.cpp:11772-11801), by class (0 = Dark Wizard, the one
            # from `CreateCharacterPointer`) and `Skin`.
            cls = repr(st.fields.get("class", (Sym("CLASS_WIZARD"),))[0])
            skin = st.fields.get("skin", (0,))[0]
            table0 = {"CLASS_WIZARD": 0.9, "CLASS_KNIGHT": 0.9, "CLASS_ELF": 0.88, "CLASS_DARK": 0.95,
                      "CLASS_DARK_LORD": 0.92, "CLASS_SUMMONER": 0.9}
            table1 = {"CLASS_WIZARD": 0.93, "CLASS_KNIGHT": 0.93, "CLASS_ELF": 0.86, "CLASS_DARK": 0.95,
                      "CLASS_DARK_LORD": 0.92, "CLASS_SUMMONER": 0.9}
            self.set("scale", (table0 if skin == 0 else table1)[cls], line)
            return
        if names[0] in self.functions and len(names) > 1 and names[1] == "(":
            # `MakeElfHelper(c)` and similar: run their code on the same character.
            self.run(self.functions[names[0]])
            return
        if names[0] == "SetAction":
            self.set("start_action", self.ev(split_args(toks)[1]), line)
            return
        if names[0] == "Vector" and s.endswith("BoundingBoxMax)"):
            args = split_args(toks)
            self.set("bbox_max", [self.ev(a) for a in args[:3]], line)
            return
        # Local variables: `int Level = 9;`, `randType = rand()%2;`.
        m = re.match(r"^(?:int|float|BYTE|bool)?([A-Za-z_]\w*)=(.+)$", s)
        if m and m.group(1) not in ("C", "O") and "." not in m.group(1):
            j = names.index("=")
            try:
                self.env.vars[m.group(1)] = self.ev(toks[j + 1:])
            except Unknown:
                st.notes.append(f"{st.file}:{line} `{text_of(toks)}`")
            return
        m = re.match(r"^(?:int|float)([A-Za-z_]\w*)$", s)
        if m:
            return
        # `Models[X].Actions[i].PlaySpeed = v` and similar: they change the model for everyone.
        m = re.match(r"^Models\[(.+?)\]\.Actions\[(\w+)\]\.PlaySpeed=(.+)$", s)
        if m:
            j = names.index("=")
            act = m.group(2)
            act = self.env.vars[act] if act in self.env.vars else MONSTER_ACTIONS.get(act, None)
            if act is None:
                act = int(m.group(2))
            speeds = dict(st.fields.get("model_speeds", ({}, line))[0])
            speeds[act] = self.ev(toks[j + 1:])
            self.set("model_speeds", speeds, line)
            return
        m = re.match(r"^Models\[(.+?)\]\.Meshs\[(\d+)\]\.NoneBlendMesh=(\w+)$", s)
        if m:
            nb = dict(st.fields.get("none_blend", ({}, line))[0])
            nb[int(m.group(2))] = m.group(3) == "true"
            self.set("none_blend", nb, line)
            return
        m = re.match(r"^Models\[(.+?)\]\.StreamMesh=(-?\d+)$", s)
        if m:
            self.set("stream_mesh", int(m.group(2)), line)
            return
        # Fields of `c` and `c->Object`.
        m = re.match(r"^([CO])\.([\w\[\]\.]+?)(=|\+=|-=|\*=)(.+)$", s)
        if m:
            who, field, op = m.group(1), m.group(2), m.group(3)
            j = next(i for i, t in enumerate(names) if t == op)
            try:
                value = self.ev(toks[j + 1:])
            except Unknown:
                value = None
            self.field(who, field, op, value, text_of(toks[j + 1:]), line)
            return
        st.notes.append(f"{st.file}:{line} `{text_of(toks)}`")

    def field(self, who, field, op, value, text, line):
        st = self.st
        key = f"{who}.{field}"
        simple = {
            "O.Scale": "scale", "C.Level": "level", "O.HiddenMesh": "hidden_mesh", "O.BlendMesh": "blend_mesh",
            "O.BlendMeshLight": "blend_mesh_light", "C.m_bFixForm": "fix_form", "C.HideShadow": "hide_shadow",
            "O.EnableShadow": "enable_shadow", "O.m_bRenderShadow": "render_shadow", "O.SubType": "sub_type",
            "O.AlphaTarget": "alpha_target", "C.MoveSpeed": "move_speed", "C.Blood": "blood",
            "O.m_fEdgeScale": "edge_scale", "O.LifeTime": "life_time", "O.Visible": "visible", "O.Kind": "kind",
            "O.LightEnable": "light_enable", "O.Live": "live", "O.m_iAnimation": "animation",
            "O.AnimationFrame": "start_frame", "O.PriorAnimationFrame": "prior_frame", "C.PK": "pk",
            "O.Gravity": "gravity", "O.m_bpcroom": "pc_room", "C.Class": "class", "C.Skin": "skin",
            "C.Helper.Type": "helper", "O.CurrentAction": "start_action",
        }
        if key in simple and op == "=":
            if value is None:
                st.notes.append(f"{st.file}:{line} `{key} = {text}`")
                return
            self.set(simple[key], value, line)
            return
        m = re.match(r"^C\.Weapon\[(\d)\]\.(Type|Level|Option1)$", key)
        if m and op == "=" and value is not None:
            w = dict(st.fields.get(f"weapon{m.group(1)}", ({}, line))[0])
            w[m.group(2)] = value
            self.set(f"weapon{m.group(1)}", w, line)
            return
        m = re.match(r"^C\.BodyPart\[BODYPART_(\w+)\]\.(Type|Level)$", key)
        if m and op == "=" and value is not None:
            parts = {k: dict(v) for k, v in st.fields.get("body", ({}, line))[0].items()}
            parts.setdefault(m.group(1), {})[m.group(2)] = value
            self.set("body", parts, line)
            return
        if key == "C.Wing.Type" and op == "=":
            self.set("wing", value, line)
            return
        m = re.match(r"^O\.(Angle|Position)\[(\d)\]$", key)
        if m and value is not None:
            name = "angle" if m.group(1) == "Angle" else "position"
            d = dict(st.fields.get(name, ({}, line))[0])
            d[int(m.group(2))] = (op, value)
            self.set(name, d, line)
            return
        st.notes.append(f"{st.file}:{line} `{key} {op} {text}`")


# --- Creators -------------------------------------------------------------------------------------

# (file, signature line, name for `g_<name>.CreateMonster`) in `CreateMonster` order
# (ZzzCharacter.cpp:12584-12692; the `TheMapProcess()` ones in `MapProcess::Make` order, w_MapProcess.cpp:35-86).
CREATORS = [
    ("w_CursedTemple.cpp", 163, None),
    ("GMHellas.cpp", 779, None),
    ("GMBattleCastle.cpp", 1251, None),
    ("GMHuntingGround.cpp", 192, None),
    ("GMCryingWolf2nd.cpp", 124, None),
    ("GMCrywolf1st.cpp", 574, None),
    ("GMAida.cpp", 335, None),
    ("GM_kanturu_1st.cpp", 404, None),
    ("GM_Kanturu_2nd.cpp", 53, None),
    ("GM_Kanturu_3rd.cpp", 661, None),
    ("GM3rdChangeUp.cpp", 398, None),
    ("Event.cpp", 251, None),
    ("GMNewTown.cpp", 993, None),
    ("GMSwampOfQuiet.cpp", 284, None),
    ("Event.cpp", 441, None),
    ("GM_Raklion.cpp", 91, None),
    ("GMSantaTown.cpp", 59, None),
    ("GM_PK_Field.cpp", 44, None),
    ("GMDuelArena.cpp", 58, None),
    ("GMDoppelGanger1.cpp", 57, None),
    ("GMDoppelGanger2.cpp", 56, None),
    ("GMDoppelGanger3.cpp", 53, None),
    ("GMDoppelGanger4.cpp", 54, None),
    ("GMEmpireGuardian1.cpp", 74, "EmpireGuardian1"),
    ("GMEmpireGuardian2.cpp", 71, None),
    ("GMEmpireGuardian3.cpp", 72, None),
    ("GMEmpireGuardian4.cpp", 78, None),
    ("GMUnitedMarketPlace.cpp", 64, None),
    ("GMKarutan1.cpp", 365, None),
    # The `switch(Type)` of `CreateMonster` (last).
    ("ZzzCharacter.cpp", 12577, None),
]

bodies = []
functions = {}
# Numbers of each creator (its `case`s); `None` = all (the final `switch`, with `default:`).
handled = []
for file, line, alias in CREATORS:
    body = function_body(src(file), line)
    if file == "ZzzCharacter.cpp":
        # Only the final `switch(Type)` (the earlier creators are already in the list).
        body = next(x for x in body.items if x.kind == "switch")
    bodies.append((file, body))
    if alias:
        functions[alias] = body
# `MakeElfHelper` (ZzzCharacter.cpp:12065), called by `case 257`.
functions["MakeElfHelper"] = function_body(src("ZzzCharacter.cpp"), 12065)

case_numbers = set()


def collect_cases(node, into):
    if node.kind == "case":
        try:
            into.add(evaluate(node.expr, Env()))
        except Unknown:
            pass
    for x in getattr(node, "items", []) or []:
        collect_cases(x, into)
    for attr in ("then", "other", "body"):
        x = getattr(node, attr, None)
        if x is not None:
            collect_cases(x, into)


for file, b in bodies:
    mine = set()
    collect_cases(b, mine)
    case_numbers.update(mine)
    handled.append(None if file == "ZzzCharacter.cpp" else mine)
# `C09SummerEvent::CreateMonster` has no `switch`: `if(iType == 463)` (Event.cpp:445).
case_numbers.add(463)
handled[[i for i, c in enumerate(CREATORS) if c[:2] == ("Event.cpp", 441)][0]] = {463}
# `GMEmpireGuardian2..4::CreateMonster` call `g_EmpireGuardian1.CreateMonster` first.
eg1 = handled[[i for i, c in enumerate(CREATORS) if c[0] == "GMEmpireGuardian1.cpp"][0]]
for i, c in enumerate(CREATORS):
    if c[0] in ("GMEmpireGuardian2.cpp", "GMEmpireGuardian3.cpp", "GMEmpireGuardian4.cpp"):
        handled[i] = handled[i] | eg1


def run_creator(file, body, number, world, rand):
    env = Env(type_value=number, world=world, rand=rand)
    st = State(file)
    it = Interp(env, st, functions)
    try:
        it.run(body)
    except (Return, Break):
        pass
    return st if st.created else None, env


def create(number, world, rand):
    for (file, body), mine in zip(bodies, handled):
        if mine is not None and number not in mine:
            continue
        st, env = run_creator(file, body, number, world, rand)
        if st is not None:
            return st
    return None


def signature(st):
    if st is None:
        return None
    return repr(sorted((k, repr(v[0])) for k, v in st.fields.items()))


def resolve(number):
    """[(maps or None = any, rand variants: [State])]"""
    def variants(world):
        try:
            return [create(number, world, None)]
        except RandomDependent:
            out, seen = [], set()
            for r in range(4):
                st = create(number, world, [r])
                sig = signature(st)
                if sig not in seen:
                    seen.add(sig)
                    out.append(st)
            return out

    try:
        return [(None, variants(None))]
    except WorldDependent:
        pass
    groups = {}
    for w in WORLDS:
        vs = variants(w)
        sig = tuple(signature(s) for s in vs)
        groups.setdefault(sig, (vs, []))[1].append(w)
    out = sorted(groups.values(), key=lambda g: -len(g[1]))
    # Outside its creator's maps, a number the `switch` lacks falls into `default:` (the bull): the server
    # never sends it there, so that entry is useless and the creator's one applies everywhere.
    def from_default(vs):
        return all(s is not None and s.file == "ZzzCharacter.cpp" and s.case_line == DEFAULT_LINE for s in vs)
    if len(out) > 1:
        out = [g for g in out if not from_default(g[0])] or out
    # The one with most maps, without a list (applies to the rest).
    return [(None if i == 0 else maps, vs) for i, (vs, maps) in enumerate(out)]


DEFAULT_LINE = next(x.line for x in bodies[-1][1].items if x.kind == "default")


monsters = {}
for n in sorted(case_numbers):
    monsters[n] = resolve(n)
# The switch's `default:`: any number not in any `case` (ZzzCharacter.cpp:13467).
default_def = resolve(100000)

if __name__ == "__main__" and "--debug" in sys.argv:
    for n, groups in monsters.items():
        for maps, vs in groups:
            for st in vs:
                if st is None:
                    print(n, maps, None)
                    continue
                print(n, maps, st.file, st.case_line, {k: v[0] for k, v in st.fields.items()})
                for note in st.notes:
                    print("    ", note)
    sys.exit(0)


# --- OpenMonsterModel -----------------------------------------------------------------------------

class ModelState(State):
    def __init__(self, file):
        super().__init__(file)
        self.speeds = {}  # action -> PlaySpeed
        self.speed_lines = {}
        self.hold = set()  # actions with `Loop = true` (they hold the last frame)
        self.bone_head = {}  # model index -> bone
        self.stream_mesh = {}
        self.sounds = {}  # model index -> [slots]
        self.waves = {}  # slot SOUND_MONSTER+n -> file


class ModelInterp(Interp):
    """`OpenMonsterModel(Type)`: `b` is `Models[MODEL_MONSTER01+Type]`."""

    def stmt(self, node):
        toks = node.toks
        s = "".join(t for t, _ in toks)
        names = [t for t, _ in toks]
        st, line = self.st, node.line
        t = self.env.type
        if names[0] in ("g_ErrorReport", "gLoadData", "LoadBitmap") or s.startswith("BMD*") or s.startswith("intIndex="):
            return
        m = re.match(r"^(?:int|bool)(\w+)=(.+)$", s)
        if m:
            self.env.vars[m.group(1)] = self.ev(toks[names.index("=") + 1:])
            return
        m = re.match(r"^b->Actions\[(.+?)\]\.(PlaySpeed|Loop)(=|\*=)(.+)$", s)
        if m:
            act = evaluate([x for x in re.findall(r"\w+|[+\-]", m.group(1))], self.env)
            j = names.index(m.group(3))
            v = self.ev(toks[j + 1:])
            if m.group(2) == "Loop":
                (st.hold.add if v else st.hold.discard)(act)
            elif m.group(3) == "=":
                st.speeds[act] = v
                st.speed_lines[act] = line
            else:
                if act not in st.speeds:
                    raise Unknown(f"*= sin valor en la acción {act}")
                st.speeds[act] = st.speeds[act] * v
                st.speed_lines[act] = line
            return
        m = re.match(r"^Models\[MODEL_MONSTER01\+(\w+)\]\.(BoneHead|StreamMesh)=(-?\d+)$", s)
        if m:
            idx = t if m.group(1) == "Type" else int(m.group(1))
            (st.bone_head if m.group(2) == "BoneHead" else st.stream_mesh)[idx] = int(m.group(3))
            return
        m = re.match(r"^b->StreamMesh=(-?\d+)$", s)
        if m:
            st.stream_mesh[t] = int(m.group(1))
            return
        if names[0] == "LoadWaveFile":
            args = split_args(toks)
            slot = self.ev(args[0])
            if isinstance(slot, Sym) and slot.name == "SOUND_MONSTER":
                st.waves[slot.offset] = args[1][0][0].strip('"').replace("\\\\", "/")
            else:
                st.notes.append(f"ZzzOpenData.cpp:{line} `{text_of(toks)}`")
            return
        if names[0] == "SetMonsterSound":
            args = split_args(toks)
            model = self.ev(args[0])
            idx = model.offset if isinstance(model, Sym) else None
            vals = [self.ev(a) for a in args[1:]]
            st.sounds[idx] = vals + [-1] * (10 - len(vals))
            return
        st.notes.append(f"ZzzOpenData.cpp:{line} `{text_of(toks)}`")


open_data = src("ZzzOpenData.cpp")
omm_body = function_body(open_data, 2367)

used_models = set()
for groups in list(monsters.values()) + [default_def]:
    for _, vs in groups:
        for st in vs:
            if st is None:
                continue
            for key in ("model", "open"):
                v = st.fields.get(key, (None,))[0]
                if isinstance(v, Sym) and v.name == "MODEL_MONSTER01":
                    used_models.add(v.offset)
monster_models = {}
waves = {}
for t in range(0, max(used_models) + 1):
    st = ModelState("ZzzOpenData.cpp")
    it = ModelInterp(Env(type_value=t, world=0), st, {})
    try:
        it.run(omm_body)
    except (Return, Break):
        pass
    monster_models[t] = st
    waves.update(st.waves)


# --- OpenNpc --------------------------------------------------------------------------------------

class NpcState(State):
    def __init__(self, file):
        super().__init__(file)
        self.access = []  # (slot, folder, file, line)
        self.textures = {}  # slot -> folder
        self.speeds = {}
        self.texture_patch = None


class NpcInterp(Interp):
    def stmt(self, node):
        toks = node.toks
        s = "".join(t for t, _ in toks)
        names = [t for t, _ in toks]
        st, line = self.st, node.line
        if s.startswith("BMD*") or names[0] in ("LoadWaveFile", "SetTexture", "itoa"):
            return
        if names[0] == "gLoadData" and names[2] == "AccessModel":
            args = split_args(toks)
            slot = self.ev(args[0])
            folder = args[1][0][0].strip('"').replace("\\\\", "/").removeprefix("Data/").rstrip("/")
            name = args[2][0][0].strip('"')
            i = self.ev(args[3]) if len(args) > 3 else -1
            file = f"{name}.bmd" if i == -1 else f"{name}{i:02d}.bmd"
            st.access.append((slot, folder, file, line))
            return
        if names[0] == "gLoadData" and names[2] == "OpenTexture":
            args = split_args(toks)
            slot = self.ev(args[0])
            st.textures[slot] = args[1][0][0].strip('"').replace("\\\\", "/").rstrip("/")
            return
        m = re.match(r"^Models\[Type\]\.Actions\[(\w+)\]\.PlaySpeed=(.+)$", s)
        if m:
            st.speeds[MONSTER_ACTIONS[m.group(1)]] = (self.ev(toks[names.index("=") + 1:]), line)
            return
        if s.startswith("Models[Type].Textures->FileName") or re.match(r"^(int|char)_", s):
            st.texture_patch = line
            return
        st.notes.append(f"ZzzOpenData.cpp:{line} `{text_of(toks)}`")


npc_body = function_body(open_data, 1865)
npc_switches = [x for x in npc_body.items if x.kind == "switch"]
npc_slots = []
for x in npc_switches[0].items:
    if x.kind == "case":
        npc_slots.append((evaluate(x.expr, Env()), x.line))
npc_models = {}
for slot, case_line in npc_slots:
    st = NpcState("ZzzOpenData.cpp")
    env = Env(type_value=slot, world=0)
    it = NpcInterp(env, st, {})
    for sw in npc_switches:
        try:
            it.run(sw)
        except (Return, Break):
            pass
    st.case_line = case_line
    npc_models[slot] = st

# The timed quest NPCs (566-568): Main 5.2 creates them (ZzzCharacter.cpp:14390-14414) and sets Tercia's
# `PlaySpeed` (ZzzOpenData.cpp:2324), but has no `AccessModel` for them (left unfinished). Files as in
# MuMain (Data/Monsters/Models.json): `tersia.bmd`, `bena.bmd` and, for Zyro, Moss's model.
MISSING_ACCESS = {
    "MODEL_TIME_LIMIT_QUEST_NPC_TERSIA": ("Npc", "tersia.bmd"),
    "MODEL_TIME_LIMIT_QUEST_NPC_BENA": ("Npc", "bena.bmd"),
    "MODEL_TIME_LIMIT_QUEST_NPC_ZAIRO": "MODEL_GAMBLE_NPC_MOSS",
}
for name, where in MISSING_ACCESS.items():
    slot = Sym(name)
    assert slot not in npc_models, f"{name} already has AccessModel in Main 5.2: remove it from MISSING_ACCESS"
    st = NpcState("ZzzOpenData.cpp")
    it = NpcInterp(Env(type_value=slot, world=0), st, {})
    for sw in npc_switches:
        try:
            it.run(sw)
        except (Return, Break):
            pass
    if isinstance(where, str):
        base = npc_models[Sym(where)]
        _, folder, file, _ = next(a for a in base.access if a[0] == Sym(where))
        st.textures[slot] = base.textures.get(Sym(where), "Npc")
        st.src = f"MuMain Models.json ({where}, ZzzOpenData.cpp:{base.case_line})"
    else:
        folder, file = where
        st.src = "MuMain Models.json"
    st.access.append((slot, folder, file, None))
    st.case_line = None
    npc_models[slot] = st


# --- LinkBone (CreateCharacterPointer) ------------------------------------------------------------

ccp = function_body(src("ZzzCharacter.cpp"), 11263)
link_switch = [x for x in ccp.items if x.kind == "switch"][-1]
link_bones = {}
labels = []
for x in link_switch.items:
    if x.kind == "case":
        labels.append(evaluate(x.expr, Env()))
    elif x.kind == "default":
        labels = []
    elif x.kind == "stmt":
        s = "".join(t for t, _ in x.toks)
        m = re.match(r"^c->Weapon\[(\d)\]\.LinkBone=(\d+)$", s)
        if m:
            for lab in labels:
                link_bones.setdefault(lab, [33, 42])[int(m.group(1))] = int(m.group(2))
    elif x.kind == "break":
        labels = []


# --- Names and comparison with MuMain -------------------------------------------------------------

names = {}
mumain = {}
# Maps where each number appears (MuMain's `numbers[].maps`): only for picking test maps.
seen_in = {}
try:
    with open(os.path.join(MUMAIN_DATA, "Monsters", "Monsters.json"), encoding="utf-8") as f:
        mj = json.load(f)
    for mdef in mj["monsters"]:
        for num in mdef["numbers"]:
            n = num["number"]
            names.setdefault(n, (mdef["name"].get("es") or mdef["name"]["en"], mdef["name"]["en"]))
            mumain.setdefault(n, (mdef, num))
            seen_in.setdefault(n, []).extend(m for m in num.get("maps", []) if 0 <= m < 256)
except FileNotFoundError:
    print("warning: no MuMain Monsters.json: no names")

GROUPS = ["MODEL_SWORD", "MODEL_AXE", "MODEL_MACE", "MODEL_SPEAR", "MODEL_BOW", "MODEL_STAFF", "MODEL_SHIELD",
          "MODEL_HELM", "MODEL_ARMOR", "MODEL_PANTS", "MODEL_GLOVES", "MODEL_BOOTS", "MODEL_WING", "MODEL_HELPER",
          "MODEL_POTION", "MODEL_ETC"]


def item_of(v):
    if isinstance(v, Sym) and v.name in GROUPS:
        return GROUPS.index(v.name), v.offset
    return None


warnings = []


def compare(n, st):
    if n not in mumain or st is None:
        return
    mdef, num = mumain[n]
    scale = num.get("scale", mdef.get("scale"))
    ours = st.fields.get("scale", (0.9,))[0]
    if isinstance(scale, (int, float)) and abs(scale - ours) > 1e-4:
        warnings.append(f"{n}: scale Main 5.2 {ours} / MuMain {scale}")
    weapons = (num.get("equipment") or mdef.get("equipment") or {}).get("weapons") or []
    for i in range(2):
        w = st.fields.get(f"weapon{i}", ({},))[0].get("Type", -1)
        ours = item_of(w)
        theirs = weapons[i] if i < len(weapons) else None
        theirs = (theirs["group"], theirs["number"]) if theirs else None
        if ours != theirs:
            warnings.append(f"{n}: weapon {i} Main 5.2 {ours} / MuMain {theirs}")


for n, groups in monsters.items():
    compare(n, groups[0][1][0])


# --- Rust -----------------------------------------------------------------------------------------

def rs_f32(v):
    # Products (`PlaySpeed *= 1.2f`) are computed in `float` in the original: 7 digits are enough.
    s = format(float(v), ".7g")
    return s if "." in s or "e" in s else s + ".0"


def rs_str(s):
    return json.dumps(s, ensure_ascii=False)


def rs_model(v):
    if isinstance(v, Sym):
        if v.name == "MODEL_MONSTER01":
            return f"Model::Monster({v.offset})"
        if v.name == "MODEL_PLAYER":
            return "Model::Player"
        return f"Model::Npc({rs_str(repr(v))})" if v in npc_models else f"Model::Other({rs_str(repr(v))})"
    if isinstance(v, int):
        # `CreateCharacter(Key, 39, ...)`: the model of the map's object 39 (traps).
        return f"Model::Object({v})"
    raise ValueError(v)


def rs_item(d, level_key="Level"):
    t = d.get("Type", -1)
    it = item_of(t)
    if it is None:
        return "None"
    lvl = d.get(level_key, 0)
    opt = d.get("Option1", 0)
    return f"Some(Item {{ group: {it[0]}, index: {it[1]}, level: {int(lvl)}, option: {int(opt)} }})"


def rs_def(n, maps, st):
    f = {k: v[0] for k, v in st.fields.items()}
    name, name_en = names.get(n, ("", ""))
    parts = [f"number: {n}"]
    if maps:
        parts.append(f"maps: &{list(maps)}")
    parts.append(f"model: {rs_model(f['model'])}")
    if "scale" in f:
        parts.append(f"scale: {rs_f32(f['scale'])}")
    w = [rs_item(f.get(f"weapon{i}", {})) for i in range(2)]
    if w != ["None", "None"]:
        parts.append(f"weapons: [{w[0]}, {w[1]}]")
    if "body" in f:
        order = ["HELM", "ARMOR", "PANTS", "GLOVES", "BOOTS"]
        b = f["body"]
        items = []
        for k in order:
            d = b.get(k, {})
            t = d.get("Type", -1)
            if isinstance(t, Sym) and t.name.startswith("MODEL_MERCHANT"):
                items.append(f"Part::Npc({rs_str(repr(t))})")
            elif item_of(t):
                items.append(f"Part::Item({rs_item(d)[5:-1]})")
            else:
                items.append("Part::None")
        parts.append(f"body: [{', '.join(items)}]")
    if "wing" in f and item_of(f["wing"]):
        g, i = item_of(f["wing"])
        parts.append(f"wing: Some(Item {{ group: {g}, index: {i}, level: 0, option: 0 }})")
    if "helper" in f and item_of(f["helper"]):
        g, i = item_of(f["helper"])
        parts.append(f"helper: Some(Item {{ group: {g}, index: {i}, level: 0, option: 0 }})")
    ints = {"level": "level", "hidden_mesh": "hidden_mesh", "blend_mesh": "blend_mesh", "move_speed": "move_speed",
            "skin": "skin"}
    for k, rk in ints.items():
        if k in f:
            parts.append(f"{rk}: {int(f[k])}")
    floats = {"blend_mesh_light": "blend_mesh_light", "alpha_target": "alpha_target", "edge_scale": "edge_scale"}
    for k, rk in floats.items():
        if k in f:
            parts.append(f"{rk}: {rs_f32(f[k])}")
    bools = {"fix_form": "fix_form", "hide_shadow": "hide_shadow", "render_shadow": "render_shadow",
             "blood": "blood", "visible": "visible", "live": "live", "light_enable": "light_enable",
             "pc_room": "pc_room"}
    for k, rk in bools.items():
        if k in f:
            parts.append(f"{rk}: {'true' if f[k] else 'false'}")
    if "sub_type" in f:
        v = f["sub_type"]
        parts.append(f"sub_type: SubType::{'Model(' + rs_str(repr(v)) + ')' if isinstance(v, Sym) else 'Value(' + str(int(v)) + ')'}")
    if "kind" in f:
        parts.append(f"kind: Some(Kind::{ {'KIND_NPC': 'Npc', 'KIND_MONSTER': 'Monster', 'KIND_TRAP': 'Trap'}[repr(f['kind'])] })")
    if "class" in f:
        parts.append(f"class: Some({rs_str(repr(f['class']))})")
    if "life_time" in f:
        parts.append(f"life_time: Some({int(f['life_time'])})")
    if "start_action" in f and isinstance(f["start_action"], int):
        parts.append(f"start_action: Some({f['start_action']})")
    elif "start_action" in f:
        parts.append(f"start_player_action: Some({rs_str(repr(f['start_action']))})")
    if "start_frame" in f:
        parts.append(f"start_frame: Some({rs_f32(f['start_frame'])})")
    if "height" in f:
        parts.append(f"height: Some({rs_f32(f['height'])})")
    if "position" in f:
        for axis, (op, v) in f["position"].items():
            if axis == 2 and op in ("+=", "-="):
                parts.append(f"z_offset: {rs_f32(v if op == '+=' else -v)}")
    if "angle" in f:
        a = ["None"] * 3
        for axis, (op, v) in f["angle"].items():
            if op == "=":
                a[axis] = f"Some({rs_f32(v)})"
        parts.append(f"angle: [{', '.join(a)}]")
    if "rotation" in f:
        parts.append(f"rotation: Some({rs_f32(f['rotation'])})")
    if "tile_dx" in f or "tile_dy" in f:
        parts.append(f"tile_offset: ({f.get('tile_dx', 0)}, {f.get('tile_dy', 0)})")
    if "bbox_max" in f:
        parts.append(f"bbox_max: Some([{', '.join(rs_f32(x) for x in f['bbox_max'])}])")
    if "model_speeds" in f:
        sp = ", ".join(f"({a}, {rs_f32(v)})" for a, v in sorted(f["model_speeds"].items()))
        parts.append(f"model_speeds: &[{sp}]")
    if "none_blend" in f:
        nb = ", ".join(f"({m}, {'true' if v else 'false'})" for m, v in sorted(f["none_blend"].items()))
        parts.append(f"none_blend: &[{nb}]")
    if "stream_mesh" in f:
        parts.append(f"stream_mesh: Some({int(f['stream_mesh'])})")
    if st.notes:
        parts.append("notes: &[" + ", ".join(rs_str(x) for x in st.notes) + "]")
    parts.append(f"src: {rs_str(f'{st.file}:{st.case_line}')}")
    seen = sorted(set(seen_in.get(n, [])))
    if seen:
        parts.append(f"seen_in: &{seen}")
    if name:
        parts.append(f"name: {rs_str(name)}")
    if name_en and name_en != name:
        parts.append(f"name_en: {rs_str(name_en)}")
    return "    Def { " + ", ".join(parts) + ", ..BASE },"


out = []
out.append("// Generated by tools/gen_monsters.py from Main 5.2 (names: MuMain's Monsters.json). Do not edit by hand.")
out.append("")
out.append("use super::*;")
out.append("")
out.append("/// Each server monster number with its look (`CreateMonster`, ZzzCharacter.cpp:12577). A number may")
out.append("/// appear several times: first the entries for specific maps (`maps`), last the one for all other maps;")
out.append("/// several with the same `maps` are `rand()` variants (one is picked at random).")
out.append("pub static DEFS: &[Def] = &[")
count = 0
for n, groups in monsters.items():
    # Map-specific entries before the general one.
    for maps, vs in sorted(groups, key=lambda g: g[0] is None):
        for st in vs:
            if st is None:
                continue
            out.append(rs_def(n, maps, st))
            count += 1
out.append("];")
out.append("")
out.append("/// The `switch`'s `default:` (ZzzCharacter.cpp:13467): any number that is not in any `case`.")
st = default_def[0][1][0]
out.append("pub static DEFAULT_DEF: Def = " + rs_def(0, None, st).strip().rstrip(",").replace("number: 0", "number: u16::MAX") + ";")
out.append("")

# Monster models.
out.append("/// `OpenMonsterModel(Type)` (ZzzOpenData.cpp:2367) of each `Monster{Type+1:02}.bmd`: `PlaySpeed` of each")
out.append("/// action (`None` = the original does not set it), actions with `Loop` (they hold their last frame),")
out.append("/// `BoneHead`, `StreamMesh`, sounds (`Sounds[0..10]`, `SOUND_MONSTER` slots) and weapon `LinkBone`")
out.append("/// (`CreateCharacterPointer`, ZzzCharacter.cpp:11540-11676).")
out.append("pub static MONSTER_MODELS: &[MonsterModel] = &[")
for t, st in monster_models.items():
    speeds = ", ".join(f"Some({rs_f32(st.speeds[a])})" if a in st.speeds else "None" for a in range(12))
    hold = sum(1 << a for a in st.hold)
    sounds = st.sounds.get(t, [-1] * 10)
    lb = link_bones.get(Sym("MODEL_MONSTER01", t), [33, 42])
    lines_ = sorted(set(st.speed_lines.values()))
    out.append(
        f"    MonsterModel {{ speeds: [{speeds}], hold: {hold:#06x}, bone_head: {st.bone_head.get(t, -1)}, "
        f"stream_mesh: {st.stream_mesh.get(t, -1)}, sounds: [{', '.join(str(int(x)) for x in sounds)}], "
        f"link_bones: [{lb[0]}, {lb[1]}] }}, // {t}" + (f" ZzzOpenData.cpp:{','.join(map(str, lines_))}" if lines_ else "")
    )
out.append("];")
out.append("")
out.append("/// `LoadWaveFile(SOUND_MONSTER + n, ...)` of `OpenMonsterModel`: (n, file under the client folder).")
out.append("pub static MONSTER_WAVES: &[(u16, &str)] = &[")
for k in sorted(waves):
    out.append(f"    ({k}, {rs_str(waves[k])}),")
out.append("];")
out.append("")

# NPC.
out.append("/// `OpenNpc` models (ZzzOpenData.cpp:1865): file, texture folder, parts and the `PlaySpeed`s that")
out.append("/// change (every action starts at 0.25, ZzzOpenData.cpp:2268).")
out.append("pub static NPC_MODELS: &[NpcModel] = &[")
for slot, st in npc_models.items():
    main = [a for a in st.access if a[0] == slot]
    if not main:
        out.append(f"    // {slot!r}: no active AccessModel (ZzzOpenData.cpp:{st.case_line})")
        continue
    _, folder, file, line = main[0]
    tex = st.textures.get(slot, "Npc")
    parts = [a for a in st.access if a[0] != slot]
    prs = ", ".join(f"({rs_str(repr(p[0]))}, {rs_str(p[1])}, {rs_str(p[2])}, {rs_str(st.textures.get(p[0], 'Npc'))})" for p in parts)
    sp = ", ".join(f"({a}, {rs_f32(v)})" for a, (v, _) in sorted(st.speeds.items()))
    extra = f", texture_patch: {'true' if st.texture_patch else 'false'}"
    where = getattr(st, "src", None) or f"ZzzOpenData.cpp:{line}"
    out.append(
        f"    NpcModel {{ name: {rs_str(repr(slot))}, dir: {rs_str(folder)}, file: {rs_str(file)}, textures: {rs_str(tex)}, "
        f"parts: &[{prs}], speeds: &[{sp}]{extra}, src: {rs_str(where)} }},"
    )
out.append("];")
out.append("")
out.append("/// Weapon `LinkBone` of the models that are not `MonsterNN` (`CreateCharacterPointer`).")
out.append("pub static OTHER_LINK_BONES: &[(&str, [u8; 2])] = &[")
for k, v in link_bones.items():
    if not (isinstance(k, Sym) and k.name == "MODEL_MONSTER01"):
        out.append(f"    ({rs_str(repr(k))}, [{v[0]}, {v[1]}]),")
out.append("];")

os.makedirs(os.path.dirname(OUT), exist_ok=True)
with open(OUT, "w", encoding="utf-8", newline="\n") as f:
    f.write("\n".join(out) + "\n")
print(f"{OUT}: {count} entries for {len(monsters)} numbers, {len(monster_models)} MonsterNN models, {len(npc_models)} NPCs")
for w in warnings:
    print("difference with MuMain:", w)
