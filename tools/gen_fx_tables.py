"""Generates the effects engine tables from the MuMain and Main 5.2 code (read only).

    python tools/gen_fx_tables.py <path to MuMain/src/source> [<path to Main 5.2 source>]

Writes to crates/mu-client/src/fx/tables/:
- ids.rs      BITMAP_*, MODEL_*, AT_SKILL_*, PLAYER_*... constants with their exact values
              (enums of Core/Globals/_TextureIndex.h and _enum.h, with the active macros of
              Defined_Global.h), so the translated code uses the same numbers. At the end, the
              buff ids (`eBuffState`, checked against Main 5.2).
- bitmaps.rs  (id, file under Data/, repeat) of each LoadBitmap in Engine/Object/ZzzOpenData.cpp.
- models.rs   MODELS: (id, folder, .bmd file name, texture folder) of each model loaded once for
              the whole game: the literal AccessModel calls of MuMain's ZzzOpenData.cpp, plus
              (evaluated with a small interpreter) its loops and the Main 5.2 loaders of skills,
              events, NPC/monster extra parts and the Gaion swords.
              MAP_MODELS: the models Main 5.2 loads per map (`CMapManager::Load`).
"""

import os
import re
import sys
from collections import ChainMap

SRC = sys.argv[1] if len(sys.argv) > 1 else r"..\referencias\MuMain\src\source"
# Main 5.2 is the primary reference; by default it sits next to MuMain (../referencias/Main5.2).
SRC52 = (
    sys.argv[2]
    if len(sys.argv) > 2
    else os.path.join(SRC, "..", "..", "..", "Main5.2", "Source Main 5.2", "source")
)
OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "mu-client", "src", "fx", "tables")

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import cexpr  # noqa: E402
import cparse  # noqa: E402


def read(rel):
    with open(os.path.join(SRC, rel), encoding="utf-8", errors="replace") as f:
        return f.read()


def strip_comments(text):
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


# --- Macros -------------------------------------------------------------------------------

defined = set()
numeric = {}
for rel in ["Core/Globals/Defined_Global.h", "Core/Globals/_define.h"]:
    try:
        text = strip_comments(read(rel))
    except FileNotFoundError:
        continue
    for m in re.finditer(r"^\s*#\s*define\s+(\w+)(?:[ \t]+([^\n]+))?", text, flags=re.M):
        name, value = m.group(1), (m.group(2) or "").strip()
        defined.add(name)
        if value:
            numeric[name] = value
    # `constexpr int NAME = value;` also counts as a constant.
    for m in re.finditer(r"constexpr\s+(?:unsigned\s+)?(?:int|short|long|BYTE|WORD|DWORD|size_t)\s+(\w+)\s*=\s*([^;]+);", text):
        numeric.setdefault(m.group(1), m.group(2).strip())


def active_lines(text):
    """Drops what is outside #ifdef/#ifndef/#if defined according to the known macros."""
    out, stack = [], []
    for line in text.split("\n"):
        s = line.strip()
        m = re.match(r"#\s*(ifdef|ifndef|if|elif|else|endif)\b\s*(.*)", s)
        if m:
            kind, cond = m.group(1), m.group(2).strip()
            if kind in ("ifdef", "ifndef", "if"):
                if kind == "ifdef":
                    val = cond in defined
                elif kind == "ifndef":
                    val = cond not in defined
                else:
                    d = re.match(r"defined\s*\(?\s*(\w+)\s*\)?", cond)
                    val = (d.group(1) in defined) if d else cond.strip() not in ("0",)
                parent = all(stack)
                stack.append(parent and val)
            elif kind == "else":
                cur = stack.pop()
                stack.append(all(stack) and not cur)
            elif kind == "elif":
                stack.pop()
                stack.append(False)
            elif kind == "endif":
                stack.pop()
            continue
        if all(stack):
            out.append(line)
    return "\n".join(out)


# --- Enums ---------------------------------------------------------------------------------

values = {}


def eval_expr(expr):
    e = expr.strip()
    e = re.sub(r"\b(0x[0-9A-Fa-f]+)\b", lambda m: str(int(m.group(1), 16)), e)

    def name(m):
        n = m.group(0)
        if n in values:
            return str(values[n])
        if n in numeric:
            return "(" + str(eval_expr(numeric[n])) + ")"
        raise KeyError(n)

    e = re.sub(r"\b[A-Za-z_]\w*\b", name, e)
    e = e.replace("/", "//")
    return int(eval(e, {"__builtins__": {}}))


def parse_enums(text, origin):
    body = strip_comments(active_lines(text))
    count = 0
    for m in re.finditer(r"\benum\b[^{;]*\{(.*?)\}", body, flags=re.S):
        nxt = 0
        for item in m.group(1).split(","):
            item = item.strip()
            if not item:
                continue
            if "=" in item:
                n, e = item.split("=", 1)
                n = n.strip()
                try:
                    v = eval_expr(e)
                except Exception:
                    # Value we cannot compute (e.g. a sizeof): skip the rest of the enum.
                    print(f"  {origin}: could not evaluate {n} = {e.strip()}", file=sys.stderr)
                    break
            else:
                n, v = item, nxt
            if not re.match(r"^[A-Za-z_]\w*$", n):
                continue
            values.setdefault(n, v)
            nxt = v + 1
            count += 1
    return count


for rel in ["Core/Globals/_TextureIndex.h", "Core/Globals/_enum.h"]:
    print(f"{rel}: {parse_enums(read(rel), rel)} values", file=sys.stderr)

# --- LoadBitmap / AccessModel -----------------------------------------------------------------

opendata = strip_comments(active_lines(read("Engine/Object/ZzzOpenData.cpp")))

bitmaps = {}
for m in re.finditer(
    r'LoadBitmap\(\s*L"([^"]+)"\s*,\s*([A-Z_0-9]+(?:\s*[+-]\s*\d+)?)\s*(?:,\s*(GL_\w+))?\s*(?:,\s*(GL_\w+))?',
    opendata,
):
    path, idexpr, filt, wrap = m.groups()
    try:
        i = eval_expr(idexpr)
    except Exception:
        print(f"  LoadBitmap: unknown id {idexpr}", file=sys.stderr)
        continue
    path = path.replace("\\\\", "/").replace("\\", "/")
    bitmaps[i] = (path, wrap == "GL_REPEAT", idexpr.replace(" ", ""))  # the last load wins

texdirs = {}
for m in re.finditer(r'OpenTexture\(\s*([A-Z_0-9]+(?:\s*[+-]\s*\w+)?)\s*,\s*L"([^"]+)"', opendata):
    try:
        texdirs[eval_expr(m.group(1))] = m.group(2).replace("\\\\", "/").replace("\\", "/").rstrip("/")
    except Exception:
        pass

models = {}
for m in re.finditer(
    r'AccessModel\(\s*([A-Z_0-9]+(?:\s*[+-]\s*\d+)?)\s*,\s*L"([^"]+)"\s*,\s*L"([^"]+)"\s*(?:,\s*(-?\d+))?\s*\)',
    opendata,
):
    idexpr, d, name, idx = m.groups()
    try:
        i = eval_expr(idexpr)
    except Exception:
        print(f"  AccessModel: unknown id {idexpr}", file=sys.stderr)
        continue
    d = d.replace("\\\\", "/").replace("\\", "/").rstrip("/")
    if d.startswith("Data/"):
        d = d[5:]
    # AccessModel(..., "Name", n) → Name0n.bmd; without a number → Name.bmd
    # LoadData.cpp GetModelFileName: only -1 means no number (0 gives Name00.bmd).
    fname = f"{name}{int(idx):02d}.bmd" if idx is not None and int(idx) != -1 else f"{name}.bmd"
    models[i] = (d, fname, idexpr.replace(" ", ""))

# --- Loads the literal regex above does not see ------------------------------------------------
#
# Loops (`for (int i = 0; i < 3; i++) AccessModel(MODEL_SNOW1 + i, ..., i + 1)`), the per-map loads of
# `CMapManager::Load` and the loaders outside ZzzOpenData.cpp are evaluated by walking the statement
# tree of cparse.py: blocks, if/else, switch/case (with fall-through), counted `for` loops, int and
# string variables and `sprintf`. A load whose arguments or enclosing condition cannot be evaluated
# is printed to stderr and left out. Model ids are MuMain's numbers (`values`), like ids.rs.


def mask_inactive(text):
    """MuMain source lines with the inactive #if parts (and every directive) blanked, so the line
    numbers stay those of the file (same rules as active_lines)."""
    out, stack = [], []
    for line in text.split("\n"):
        m = re.match(r"\s*#\s*(ifdef|ifndef|if|elif|else|endif)\b\s*(.*)", line)
        if m:
            kind, cond = m.group(1), m.group(2).strip()
            cond = re.sub(r"//.*", "", cond).strip()
            if kind in ("ifdef", "ifndef", "if"):
                if kind == "ifdef":
                    val = cond in defined
                elif kind == "ifndef":
                    val = cond not in defined
                else:
                    d = re.match(r"defined\s*\(?\s*(\w+)\s*\)?", cond)
                    val = (d.group(1) in defined) if d else cond.strip() not in ("0",)
                stack.append(all(stack) and val)
            elif kind == "else":
                cur = stack.pop()
                stack.append(all(stack) and not cur)
            elif kind == "elif":
                stack.pop()
                stack.append(False)
            elif kind == "endif":
                stack.pop()
            out.append("")
            continue
        out.append(line if all(stack) else "")
    return out


class Source:
    """One source file: `label` is how the comments cite it."""

    def __init__(self, root, rel, label, mumain):
        path = os.path.join(root, rel)
        self.label = label
        self.ok = os.path.exists(path)
        if not self.ok:
            print(f"  {label}: not found ({path})", file=sys.stderr)
            self.lines = []
        elif mumain:
            with open(path, encoding="utf-8", errors="replace") as f:
                self.lines = mask_inactive(f.read())
        else:
            # cparse applies the Main 5.2 macros (cparse.DEFINED) itself.
            self.lines = cparse.read_source(path)
        self._bodies = {}

    def function_line(self, name):
        """Line (1-based) of the definition of `name` (`Class::Method` or a free function)."""
        pat = re.compile(r"^[A-Za-z_][\w:<>\*&\s]*?\b" + re.escape(name) + r"\s*\(")
        for n, line in enumerate(self.lines, 1):
            code = re.sub(r"//.*", "", line).rstrip()
            if pat.match(line) and not code.endswith(";"):
                return n
        return None

    def body(self, name):
        if name not in self._bodies:
            n = self.function_line(name)
            if n is None:
                print(f"  {self.label}: function {name} not found", file=sys.stderr)
                self._bodies[name] = None
            else:
                try:
                    self._bodies[name] = cparse.function_body(self.lines, n)
                except (SyntaxError, IndexError) as e:
                    print(f"  {self.label}: cannot parse {name}: {e}", file=sys.stderr)
                    self._bodies[name] = None
        return self._bodies[name]

    def functions_with(self, call):
        """Names of the functions whose body contains `call(` (by the nearest definition above)."""
        names = []
        for n, line in enumerate(self.lines, 1):
            if re.search(r"\b" + call + r"\s*\(", re.sub(r"//.*", "", line)):
                for k in range(n - 1, 0, -1):
                    m = re.match(r"^[A-Za-z_][\w:<>\*&\s]*?\b([\w:~]+)\s*\(", self.lines[k - 1])
                    code = re.sub(r"//.*", "", self.lines[k - 1]).rstrip()
                    if m and not code.endswith(";") and m.group(1) not in ("if", "for", "while", "switch"):
                        if m.group(1) not in names:
                            names.append(m.group(1))
                        break
        return names


# Values the interpreter can use: MuMain's enums and numeric macros, and the maps (`ENUM_WORLD`,
# Main 5.2 MapManager.h:6).
VARS = dict(values)
for _n in numeric:
    try:
        VARS.setdefault(_n, eval_expr(numeric[_n]))
    except Exception:
        pass


def eval_in(expr, table):
    e = re.sub(r"\b[A-Za-z_]\w*\b", lambda m: str(table[m.group(0)]), expr)
    return int(eval(e.replace("/", "//"), {"__builtins__": {}}))


WORLDS = {}
_mh = os.path.join(SRC52, "MapManager.h")
if os.path.exists(_mh):
    _text = "\n".join(s for _, s in cparse.active_text(cparse.read_source(_mh)))
    _m = re.search(r"enum\s+ENUM_WORLD\s*\{(.*?)\}", _text, flags=re.S)
    _nxt = 0
    for _item in _m.group(1).split(","):
        _item = _item.strip()
        if not _item:
            continue
        _name, _, _e = _item.partition("=")
        _name = _name.strip()
        _v = eval_in(_e, ChainMap(WORLDS, VARS)) if _e.strip() else _nxt
        WORLDS[_name] = _v
        _nxt = _v + 1
    for _n, _v in WORLDS.items():
        VARS.setdefault(_n, _v)
        if _n in cexpr.WD and cexpr.WD[_n] != _v:
            print(f"  cexpr.WD[{_n}] = {cexpr.WD[_n]}, MapManager.h gives {_v}: using {_v}", file=sys.stderr)
else:
    print(f"  Main 5.2 MapManager.h not found under {SRC52}: no MAP_MODELS", file=sys.stderr)
NUM_WD = WORLDS.get("NUM_WD", 0)

# Map checks the per-map code uses that cexpr.WORLD_PREDICATES lacks (Main 5.2 MapManager.cpp
# `InHellas`, `IsEmpireGuardian1..4`; maps of MapManager.h:23-24,30). As cexpr does for the other
# `In*()` checks, they test the current map: in Main 5.2 their `iMap = -1` default makes them return
# false, which would leave out the Kundun models of Kalima (MapManager.cpp:988-1014).
WORLD_PREDICATES = dict(cexpr.WORLD_PREDICATES)
WORLD_PREDICATES.update({
    "InHellas": lambda w: WORLDS["WD_24HELLAS"] <= w <= WORLDS["WD_24HELLAS_END"] or w == WORLDS["WD_24HELLAS_7"],
    "InHiddenHellas": lambda w: w == WORLDS["WD_24HELLAS_7"],
    "IsEmpireGuardian1": lambda w: w == 69,
    "IsEmpireGuardian2": lambda w: w == 70,
    "IsEmpireGuardian3": lambda w: w == 71,
    "IsEmpireGuardian4": lambda w: w in (72, 73, 74),
    "IsEmpireGuardian": lambda w: 69 <= w <= 74,
})
# cexpr's `WD_24HELLAS_END` is 28; the enum gives 29 (24 + HELLAS_NUM - 2): InHellas above uses
# the enum. The other predicates do not depend on it.
cexpr.WORLD_PREDICATES.update(WORLD_PREDICATES)


class Break(Exception):
    pass


class Continue(Exception):
    pass


class Return(Exception):
    pass


INT_TYPES = {"int", "short", "long", "BYTE", "WORD", "DWORD", "bool", "unsigned", "float", "auto", "size_t"}
STR_TYPES = {"char", "wchar_t"}


def unescape(tok):
    s = tok[1:-1]
    return re.sub(r"\\(.)", lambda m: {"n": "\n", "t": "\t", "0": ""}.get(m.group(1), m.group(1)), s)


def clean(toks):
    """Token strings without the `L` of wide strings, `::` prefixes and `this->` (→ gMapManager.)."""
    names = [t for t, _ in toks] if toks and isinstance(toks[0], tuple) else list(toks)
    out = []
    i = 0
    while i < len(names):
        t = names[i]
        if t == "L" and i + 1 < len(names) and names[i + 1].startswith('"'):
            i += 1
            continue
        if t == "::" and (not out or not re.match(r"\w", out[-1])):
            i += 1
            continue
        if t == "this" and i + 1 < len(names) and names[i + 1] == "->":
            out += ["gMapManager", "."]
            i += 2
            continue
        out.append(t)
        i += 1
    return out


def split_args(names):
    args, cur, depth = [], [], 0
    for t in names:
        if t in ("(", "[", "{"):
            depth += 1
        elif t in (")", "]", "}"):
            depth -= 1
        if t == "," and depth == 0:
            args.append(cur)
            cur = []
            continue
        cur.append(t)
    if cur:
        args.append(cur)
    return args


def call_args(names, k):
    """Arguments of the call whose `(` is at names[k]."""
    depth = 0
    for j in range(k, len(names)):
        if names[j] == "(":
            depth += 1
        elif names[j] == ")":
            depth -= 1
            if depth == 0:
                return split_args(names[k + 1:j])
    raise cexpr.Unknown("unbalanced call")


def norm_dir(d):
    d = d.replace("\\", "/").rstrip("/")
    return d[5:] if d.startswith("Data/") else d


def model_name(i, expr_names):
    """`MODEL_FISH01+3` for an id whose expression starts with MODEL_FISH01."""
    for n in expr_names:
        if n in values and re.match(r"^(MODEL|MODAL)_", n):
            k = i - values[n]
            return n if k == 0 else f"{n}{k:+d}"
    for n, v in values.items():
        if v == i and n.startswith("MODEL_"):
            return n
    return str(i)


class Ctx:
    """One run of a loader: its loads and texture folders."""

    def __init__(self, world=None, type_value=None, calls=None):
        self.world, self.type = world, type_value
        self.vars = {}
        self.strs = {}
        self.loop_vars = set()
        self.loads = []  # (id, folder, file, src label, line, name)
        self.textures = []  # (id, folder, explicit)
        self.skipped = []  # (src label, line, text, reason)
        self.unknown = 0  # > 0 inside a branch whose condition cannot be evaluated
        self.why = []  # why, for each of those branches
        self.calls = calls or {}
        self.src = None
        self.visited = set()  # (src label, line) of every AccessModel reached


def ev(ctx, names):
    env = cexpr.Env(type_value=ctx.type, world=ctx.world)
    env.vars = ChainMap(ctx.vars, VARS)
    v = cexpr.evaluate(clean(names), env)
    if isinstance(v, cexpr.Sym):
        raise cexpr.Unknown(f"unknown name {v}")
    return v


def ev_str(ctx, names):
    names = clean(names)
    if names and all(t.startswith('"') for t in names):
        return "".join(unescape(t) for t in names)
    if len(names) == 1 and isinstance(ctx.strs.get(names[0]), str):
        return ctx.strs[names[0]]
    if len(names) == 4 and names[1] == "[" and names[3] == "]" and isinstance(ctx.strs.get(names[0]), list):
        return ctx.strs[names[0]][int(ev(ctx, [names[2]]))]
    raise cexpr.Unknown("string " + " ".join(names))


def c_format(fmt, args):
    """sprintf with %s / %d / %0Nd."""
    out, k = "", 0
    for m in re.finditer(r"%(0?\d*)([sdi])|%%|[^%]+", fmt):
        if m.group(0) == "%%":
            out += "%"
        elif m.group(2):
            out += ("%" + m.group(1) + ("s" if m.group(2) == "s" else "d")) % args[k]
            k += 1
        else:
            out += m.group(0)
    return out


def run(ctx, node):
    kind = node.kind
    if kind == "block":
        for x in node.items:
            run(ctx, x)
    elif kind == "stmt":
        stmt(ctx, node)
    elif kind == "if":
        try:
            cond = bool(ev(ctx, node.cond))
        except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError, IndexError):
            unknown_run(ctx, node.then)
            if node.other is not None:
                unknown_run(ctx, node.other)
            return
        if cond:
            run(ctx, node.then)
        elif node.other is not None:
            run(ctx, node.other)
    elif kind == "switch":
        try:
            value = ev(ctx, node.expr)
        except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError):
            unknown_run(ctx, cparse.Node("block", node.line, items=node.items))
            return
        start = None
        for k, x in enumerate(node.items):
            if x.kind == "case":
                try:
                    if ev(ctx, x.expr) == value:
                        start = k
                        break
                except (cexpr.Unknown, cexpr.WorldDependent, KeyError, TypeError):
                    pass
        if start is None:
            start = next((k for k, x in enumerate(node.items) if x.kind == "default"), None)
        if start is None:
            return
        try:
            for x in node.items[start:]:
                run(ctx, x)
        except Break:
            pass
    elif kind == "for":
        loop(ctx, node)
    elif kind == "while":
        unknown_run(ctx, node.body)
    elif kind == "break":
        raise Break()
    elif kind == "return":
        raise Return()
    # case / default / empty: nothing (a `case` reached in order is a fall-through).


def unknown_run(ctx, node, why="condition not evaluated"):
    """Walks a branch that may or may not run: its loads are reported, not kept."""
    ctx.unknown += 1
    ctx.why.append(why)
    try:
        run(ctx, node)
    except (Break, Continue, Return):
        pass
    finally:
        ctx.unknown -= 1
        ctx.why.pop()


def loop(ctx, node):
    head = clean(node.head)
    parts, cur = [], []
    for t in head:
        if t == ";":
            parts.append(cur)
            cur = []
        else:
            cur.append(t)
    parts.append(cur)
    try:
        if len(parts) != 3:
            raise cexpr.Unknown("for head")
        init, cond, step = parts
        while init and init[0] in INT_TYPES:
            init = init[1:]
        if len(init) < 3 or init[1] != "=":
            raise cexpr.Unknown("for init")
        var = init[0]
        start = ev(ctx, init[2:])
        if step in ([var, "++"], ["++", var]):
            inc = 1
        elif step in ([var, "--"], ["--", var]):
            inc = -1
        elif len(step) >= 3 and step[0] == var and step[1] in ("+=", "-="):
            inc = ev(ctx, step[2:]) * (1 if step[1] == "+=" else -1)
        else:
            raise cexpr.Unknown("for step")
        ctx.vars[var] = start
        ev(ctx, cond)  # the bound must be known before the body runs
    except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError) as e:
        unknown_run(ctx, node.body, f"loop at line {node.line} not evaluated ({e})")
        return
    was_loop = var in ctx.loop_vars
    ctx.loop_vars.add(var)
    count = 0
    try:
        while bool(ev(ctx, cond)):
            count += 1
            if count > 100000:
                raise cexpr.Unknown("endless loop")
            try:
                run(ctx, node.body)
            except Continue:
                pass
            ctx.vars[var] = ctx.vars[var] + inc
    except Break:
        pass
    finally:
        if not was_loop:
            ctx.loop_vars.discard(var)


def stmt(ctx, node):
    names = clean(node.toks)
    if not names:
        return
    if "AccessModel" in names:
        k = names.index("AccessModel")
        if k + 1 < len(names) and names[k + 1] == "(":
            access(ctx, node, names, k)
            return
    if "OpenTexture" in names:
        k = names.index("OpenTexture")
        if k + 1 < len(names) and names[k + 1] == "(":
            if ctx.unknown:
                return
            try:
                args = call_args(names, k + 1)
                i = int(ev(ctx, args[0]))
                folder = norm_dir(ev_str(ctx, args[1]))
            except (cexpr.Unknown, cexpr.WorldDependent, KeyError, TypeError, IndexError, ValueError):
                return
            explicit = not (len(args[0]) == 1 and args[0][0] in ctx.loop_vars)
            ctx.textures.append((i, folder, explicit))
            return
    # Calls into other loaders (`g_TrapCanon.Open_TrapCanon()`).
    for j in range(1, len(names) - 1):
        if names[j - 1] in (".", "->") and names[j + 1] == "(" and (names[j - 2] if j >= 2 else "", names[j]) in ctx.calls:
            src, func = ctx.calls[(names[j - 2], names[j])]
            body = src.body(func)
            if body is not None:
                saved = ctx.src
                ctx.src = src
                try:
                    run(ctx, body)
                except Return:
                    pass
                finally:
                    ctx.src = saved
            return
    # sprintf(dst, fmt, args...)
    if names[0] in ("sprintf", "swprintf", "mu_swprintf", "wsprintf") and len(names) > 3 and names[1] == "(":
        args = call_args(names, 1)
        dst = args[0][0] if len(args[0]) == 1 else None
        try:
            fmt = ev_str(ctx, args[1])
            vals = []
            for a in args[2:]:
                try:
                    vals.append(ev_str(ctx, a))
                except cexpr.Unknown:
                    vals.append(int(ev(ctx, a)))
            if dst:
                ctx.strs[dst] = c_format(fmt, vals)
        except (cexpr.Unknown, cexpr.WorldDependent, KeyError, TypeError, IndexError, ValueError):
            if dst:
                ctx.strs[dst] = None
        return
    # Declarations and assignments of int and string variables.
    t = list(names)
    while t and t[0] in ("const", "static"):
        t = t[1:]
    if t and t[0] in STR_TYPES:
        t = t[1:]
        while t and t[0] == "*":
            t = t[1:]
        if t and re.match(r"^[A-Za-z_]\w*$", t[0]):
            name = t[0]
            if "=" in t:
                eq = t.index("=")
                strs = [unescape(x) for x in t[eq + 1:] if x.startswith('"')]
                dims = t[1:eq].count("[")
                if dims >= 2 or (dims == 1 and len(strs) > 1):
                    ctx.strs[name] = strs  # char a[][50] = { "x", "y" }
                else:
                    ctx.strs[name] = strs[0] if len(strs) == 1 else None
            else:
                ctx.strs[name] = None
        return
    if t and t[0] in INT_TYPES:
        while t and t[0] in INT_TYPES:
            t = t[1:]
        if len(t) >= 3 and t[1] == "=":
            try:
                ctx.vars[t[0]] = ev(ctx, t[2:])
            except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError):
                ctx.vars.pop(t[0], None)
        elif t:
            ctx.vars.pop(t[0], None)
        return
    if len(t) >= 3 and t[1] == "=" and re.match(r"^[A-Za-z_]\w*$", t[0]) and t[0] not in VARS:
        try:
            ctx.vars[t[0]] = ev(ctx, t[2:])
        except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError):
            ctx.vars.pop(t[0], None)
        return
    if len(t) == 2 and t[0] in ctx.vars and t[1] in ("++", "--"):
        ctx.vars[t[0]] += 1 if t[1] == "++" else -1
    elif len(t) == 2 and t[1] in ctx.vars and t[0] in ("++", "--"):
        ctx.vars[t[1]] += 1 if t[0] == "++" else -1


def access(ctx, node, names, k):
    ctx.visited.add((ctx.src.label, node.line))
    text = " ".join(names[k:])
    try:
        args = call_args(names, k + 1)
        i = int(ev(ctx, args[0]))
        folder = norm_dir(ev_str(ctx, args[1]))
        name = ev_str(ctx, args[2])
        idx = int(ev(ctx, args[3])) if len(args) > 3 else -1
    except (cexpr.Unknown, cexpr.WorldDependent, cexpr.RandomDependent, KeyError, TypeError, IndexError, ValueError) as e:
        why = ctx.why[-1] if ctx.unknown else f"arguments not evaluated ({e})"
        ctx.skipped.append((ctx.src, node.line, text, why))
        return
    if ctx.unknown:
        ctx.skipped.append((ctx.src, node.line, text, ctx.why[-1]))
        return
    # LoadData.cpp:21-29 (MuMain LoadData.cpp:39-51): -1 → Name.bmd, else Name0N.bmd / NameNN.bmd.
    fname = f"{name}.bmd" if idx == -1 else f"{name}{idx:02d}.bmd"
    ctx.loads.append((i, folder, fname, ctx.src.label, node.line, model_name(i, args[0])))


def texture_dirs(ctx):
    """Last OpenTexture folder per id; one whose id is a bare loop counter only if no other."""
    explicit, by_loop = {}, {}
    for i, folder, exp in ctx.textures:
        (explicit if exp else by_loop)[i] = folder
    return {**by_loop, **explicit}


def run_function(src, func, ctx):
    body = src.body(func)
    if body is None:
        return
    ctx.src = src
    try:
        run(ctx, body)
    except (Return, Break):
        pass


def case_labels(node, into):
    if node is None:
        return
    if node.kind == "case":
        try:
            into.add(int(cexpr.evaluate(clean(node.expr), _label_env())))
        except (cexpr.Unknown, KeyError, TypeError, ValueError):
            pass
    for x in getattr(node, "items", None) or []:
        case_labels(x, into)
    for attr in ("then", "other", "body"):
        case_labels(getattr(node, attr, None), into)


def _label_env():
    env = cexpr.Env()
    env.vars = VARS
    return env


def report(ctx):
    for src, line, text, why in ctx.skipped:
        label = src.label if src else "?"
        print(f"  left out: {label}:{line} `{text}`: {why}", file=sys.stderr)


mu = lambda rel: Source(SRC, rel, "MuMain " + os.path.basename(rel), True)  # noqa: E731
m52 = lambda rel: Source(SRC52, rel, os.path.basename(rel), False)  # noqa: E731

# 1. MuMain ZzzOpenData.cpp, every function with an AccessModel or OpenTexture (the literal calls
#    are already in `models`; this adds those in loops).
mu_open = mu("Engine/Object/ZzzOpenData.cpp")
ctx_mu = Ctx()
for _f in mu_open.functions_with("AccessModel") + [
    f for f in mu_open.functions_with("OpenTexture") if f not in mu_open.functions_with("AccessModel")
]:
    run_function(mu_open, _f, ctx_mu)
report(ctx_mu)

# 2. Main 5.2 loaders of models used by effects, loaded once (not per map):
#    - OpenSkills (ZzzOpenData.cpp:3973), with the event models it opens through
#      `g_NewYearsDayEvent->LoadModel()` (Event.cpp:222) and `g_XmasEvent->LoadXmasEvent()` (Event.cpp:40);
#    - the extra parts OpenNpc (ZzzOpenData.cpp:1865) and OpenMonsterModel (ZzzOpenData.cpp:2367) load
#      with an NPC or a monster (snowman parts, ice giant parts), not the NPC/monster model itself
#      (that one is in mu-data's monster tables);
#    - the Gaion swords of GMEmpireGuardian4::CreateMonster (GMEmpireGuardian4.cpp:95-99, monster 504).
#    Not read: OpenPlayers/OpenItems and what they call (ChangeRingManager, MonkSystem,
#    LoadXmasEventItem): items and player parts, which the client resolves on its own; the logo and
#    character scenes.
open52 = m52("ZzzOpenData.cpp")
event52 = m52("Event.cpp")
kan2nd52 = m52("GM_Kanturu_2nd.cpp")
CALLS52 = {
    ("g_NewYearsDayEvent", "LoadModel"): (event52, "CNewYearsDayEvent::LoadModel"),
    ("g_XmasEvent", "LoadXmasEvent"): (event52, "CXmasEvent::LoadXmasEvent"),
    ("g_TrapCanon", "Open_TrapCanon"): (kan2nd52, "CTrapCanon::Open_TrapCanon"),
}
global52 = []  # (id, folder, file, src, line, name, note, texture dirs)
skipped52 = []
visited52 = set()
if open52.ok:
    _c = Ctx(calls=CALLS52)
    run_function(open52, "OpenSkills", _c)
    _td = texture_dirs(_c)
    global52 += [(*x, "", _td) for x in _c.loads]
    report(_c)
    visited52 |= _c.visited
    for func, primary, what, src in [
        ("OpenNpc", lambda t: t, lambda t: "with " + model_name(t, []), open52),
        ("OpenMonsterModel", lambda t: VARS["MODEL_MONSTER01"] + t, lambda t: f"with MODEL_MONSTER01+{t}", open52),
        ("GMEmpireGuardian4::CreateMonster", lambda t: None, lambda t: f"monster {t}", m52("GMEmpireGuardian4.cpp")),
    ]:
        labels = set()
        case_labels(src.body(func), labels)
        for t in sorted(labels):
            _c = Ctx(type_value=t, world=None, calls=CALLS52)
            run_function(src, func, _c)
            visited52 |= _c.visited
            _td = texture_dirs(_c)
            for x in _c.loads:
                if x[0] != primary(t):
                    global52.append((*x, f", {what(t)}", _td))
            # Unevaluated loads of the NPC/monster itself are expected (its number is `Type`).
            for s in _c.skipped:
                if "condition not evaluated" not in s[3]:
                    skipped52.append(s)
    for s in set(skipped52):
        print(f"  left out: {s[0].label}:{s[1]} `{s[2]}`: {s[3]}", file=sys.stderr)

# New MODELS rows: Main 5.2 first (primary), then MuMain's loops; the existing rows stay as they are.
new_models = {}  # id -> (folder, file, texture folder, comment)
_mu_td = texture_dirs(ctx_mu)
for i, d, f, src, line, name in ctx_mu.loads:
    if i not in models:
        new_models[i] = (d, f, _mu_td.get(i, d), f"{name} ({src}:{line})")
_from52 = {}
for i, d, f, src, line, name, note, td in global52:
    _from52[i] = (d, f, td.get(i, d), f"{name} ({src}:{line}{note})")
for i, row in _from52.items():
    if i in models:
        if (models[i][0].lower(), models[i][1].lower()) != (row[0].lower(), row[1].lower()):
            print(f"  MODELS {i} ({models[i][2]}): MuMain {models[i][0]}/{models[i][1]}, Main 5.2 {row[0]}/{row[1]}"
                  " (existing row kept)", file=sys.stderr)
        continue
    old = new_models.get(i)
    if old and (old[0].lower(), old[1].lower()) != (row[0].lower(), row[1].lower()):
        print(f"  MODELS {i}: MuMain {old[0]}/{old[1]}, Main 5.2 {row[0]}/{row[1]}: using Main 5.2", file=sys.stderr)
    new_models[i] = row
# The literal regex above writes `Name.bmd` for `AccessModel(..., "Name", 0)`, but AccessModel makes
# `Name00.bmd` (LoadData.cpp:26-27). Those rows are kept (MODELS only grows); report them.
for i, d, f, src, line, name in ctx_mu.loads:
    if i in models and models[i][1].lower() != f.lower():
        print(f"  MODELS {i} ({models[i][2]}): existing row has {models[i][1]}, AccessModel opens {f}", file=sys.stderr)

# 3. Per-map loads: Main 5.2 CMapManager::Load (MapManager.cpp:42) for each WorldActive.
#    The map's own numbered objects (ids below MAX_WORLD_OBJECTS: `Object%d` of each map, Lorencia's
#    named ones) are left out: the map loader draws them (mu-data objects.rs).
map_models = {}  # (world, id) -> (folder, file, texture folder, comment)
_world_objects = 0
mm52 = m52("MapManager.cpp")
if mm52.ok and NUM_WD:
    _globals = {i: (r[0].lower(), r[1].lower()) for i, r in list(models.items()) + list(new_models.items())}
    for w in range(NUM_WD):
        _c = Ctx(world=w, calls=CALLS52)
        run_function(mm52, "CMapManager::Load", _c)
        visited52 |= _c.visited
        _td = texture_dirs(_c)
        for i, d, f, src, line, name in _c.loads:
            if VARS["MODEL_WORLD_OBJECT"] <= i < VARS["MAX_WORLD_OBJECTS"]:
                _world_objects += 1
                continue
            if _globals.get(i) == (d.lower(), f.lower()):
                continue
            if i in _globals:
                print(f"  map {w}: {name} loads {d}/{f} instead of the global {'/'.join(_globals[i])}", file=sys.stderr)
            map_models[(w, i)] = (d, f, _td.get(i, d), f"{name}, {src}:{line}")
        for s in _c.skipped:
            # `AccessModel(i, DirName, "Object", i + 1)`: the numbered objects, left out on purpose.
            if "DirName" in s[2]:
                continue
            skipped52.append((w, s))
    for s in sorted({(x[1][0].label, x[1][1], x[1][2], x[1][3]) for x in skipped52 if isinstance(x[0], int)}):
        _ws = sorted({x[0] for x in skipped52 if isinstance(x[0], int) and (x[1][0].label, x[1][1]) == s[:2]})
        print(f"  left out: {s[0]}:{s[1]} `{s[2]}`: {s[3]} (maps {_ws[0]}..{_ws[-1]})", file=sys.stderr)
    print(f"  per-map numbered objects left out: {_world_objects} loads", file=sys.stderr)

    # MuMain moved most per-map parts to Data/Monsters/*.json; its LoadMapModels is compared only.
    _mm = mu("World/MapInfra/MapManager.cpp")
    for w in range(NUM_WD):
        _c = Ctx(world=w)
        run_function(_mm, "CMapManager::LoadMapModels", _c)
        for i, d, f, src, line, name in _c.loads:
            mine = map_models.get((w, i))
            g = _globals.get(i)
            if (mine and (mine[0].lower(), mine[1].lower()) != (d.lower(), f.lower())) or (
                not mine and g != (d.lower(), f.lower())
            ):
                print(f"  map {w}: MuMain loads {name} = {d}/{f} ({src}:{line}), Main 5.2 "
                      f"{(mine[0] + '/' + mine[1]) if mine else 'not per map'}", file=sys.stderr)

# Every active AccessModel line of the Main 5.2 files read that no run reached, by function: the
# loaders deliberately not read (items, players, scenes) and anything the interpreter missed.
_unreached = {}
for src in [open52, event52, kan2nd52, mm52]:
    if not src.ok:
        continue
    active = {n for n, _ in cparse.active_text(src.lines)}
    func = "?"
    for n, line in enumerate(src.lines, 1):
        m = re.match(r"^[A-Za-z_][\w:<>\*&\s]*?\b([\w:~]+)\s*\(", line)
        if m and not re.sub(r"//.*", "", line).rstrip().endswith(";") and m.group(1) not in ("if", "for"):
            func = m.group(1)
        if n in active and re.search(r"\bAccessModel\s*\(", line) and (src.label, n) not in visited52:
            _unreached.setdefault(f"{src.label} {func}", []).append(n)
for k, ns in _unreached.items():
    print(f"  not read: {k}: {len(ns)} AccessModel (lines {ns[0]}..{ns[-1]})", file=sys.stderr)

# --- Buff ids (eBuffState) --------------------------------------------------------------------


def enum_members(text, name):
    m = re.search(r"\benum\s+" + name + r"\s*\{(.*?)\}", text, flags=re.S)
    if not m:
        return []
    out, nxt = [], 0
    for item in m.group(1).split(","):
        item = item.strip()
        if not item:
            continue
        n, _, e = item.partition("=")
        n = n.strip()
        v = eval_in(e, ChainMap(dict(out), VARS)) if e.strip() else nxt
        out.append((n, v))
        nxt = v + 1
    return out


buffs = enum_members(strip_comments(active_lines(read("Core/Globals/_enum.h"))), "eBuffState")
buff_notes = []
_e52 = os.path.join(SRC52, "_enum.h")
if os.path.exists(_e52):
    buffs52 = dict(enum_members("\n".join(s for _, s in cparse.active_text(cparse.read_source(_e52))), "eBuffState"))
    _mu_b = dict(buffs)
    _by52 = {v: n for n, v in buffs52.items()}
    for n, v in buffs:
        if n in buffs52 and buffs52[n] != v:
            buff_notes.append(f"{n}: Main 5.2 has {buffs52[n]}, MuMain {v} (MuMain's value below)")
        elif n not in buffs52:
            other = _by52.get(v)
            buff_notes.append(f"{n} = {v}: Main 5.2 calls it {other}" if other and other not in _mu_b
                              else f"{n} = {v}: not in Main 5.2")
    for n, v in buffs52.items():
        if n not in _mu_b and v not in set(_mu_b.values()):
            buff_notes.append(f"{n} = {v}: only in Main 5.2")
    for note in buff_notes:
        print(f"  eBuffState: {note}", file=sys.stderr)

# --- Output ---------------------------------------------------------------------------------

os.makedirs(OUT, exist_ok=True)
HEADER = "// Generated by tools/gen_fx_tables.py from MuMain. Do not edit by hand.\n"
WANT = re.compile(r"^(BITMAP_|MODEL_|AT_SKILL_|PLAYER_|MONSTER01_|SKILL_|ITEM_)")

with open(os.path.join(OUT, "ids.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "#![allow(dead_code, non_upper_case_globals)]\n\n")
    for n, v in sorted(values.items(), key=lambda kv: (kv[1], kv[0])):
        if WANT.match(n):
            f.write(f"pub const {n}: i32 = {v};\n")
    if buffs:
        f.write("\n// Buff ids: `eBuffState` (Core/Globals/_enum.h), checked against Main 5.2 (_enum.h:2042).\n")
        for note in buff_notes:
            f.write(f"// {note}\n")
        for n, v in buffs:
            if not WANT.match(n):
                f.write(f"pub const {n}: i32 = {v};\n")

with open(os.path.join(OUT, "bitmaps.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "\n/// (id, file under Data/ as the original requests it, repeat the texture).\n")
    f.write("pub const BITMAPS: &[(i32, &str, bool)] = &[\n")
    for i, (path, rep, expr) in sorted(bitmaps.items()):
        f.write(f'    ({i}, "{path}", {"true" if rep else "false"}), // {expr}\n')
    f.write("];\n")

with open(os.path.join(OUT, "models.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "\n/// (id, folder under Data/, .bmd file, folder of its textures under Data/).\n")
    f.write("pub const MODELS: &[(i32, &str, &str, &str)] = &[\n")
    for i in sorted(set(models) | set(new_models)):
        if i in models:
            d, fname, expr = models[i]
            td = texdirs.get(i, d)
            f.write(f'    ({i}, "{d}", "{fname}", "{td}"), // {expr}\n')
        else:
            d, fname, td, comment = new_models[i]
            f.write(f'    ({i}, "{d}", "{fname}", "{td}"), // {comment}\n')
    f.write("];\n")
    f.write(
        "\n/// Models Main 5.2 loads when a map is entered (`CMapManager::Load`, MapManager.cpp:42):\n"
        "/// (WorldActive with Lorencia = 0, id, folder under Data/, .bmd file, folder of its textures\n"
        "/// under Data/). Only loads that MODELS lacks or that replace its file on that map; the map's\n"
        "/// numbered objects (`Object%d`, below MAX_WORLD_OBJECTS) are not listed.\n"
    )
    f.write("pub const MAP_MODELS: &[(i32, i32, &str, &str, &str)] = &[\n")
    for (w, i), (d, fname, td, comment) in sorted(map_models.items()):
        f.write(f'    ({w}, {i}, "{d}", "{fname}", "{td}"), // {comment}\n')
    f.write("];\n")

print(
    f"ids: {sum(1 for n in values if WANT.match(n))} + {len(buffs)} buffs, bitmaps: {len(bitmaps)}, "
    f"models: {len(models)} + {len(new_models)}, map models: {len(map_models)}",
    file=sys.stderr,
)
