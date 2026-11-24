"""Evaluates simple C expressions (those in the Main 5.2 table `switch`es).

`MODEL_*`, `BITMAP_*`... names stay as symbols with an offset (`Sym`). Map-dependent terms
(`gMapManager.WorldActive`, `InDevilSquare()`...) use `Env.world`; if it is `None`, `WorldDependent` is
raised so the caller re-evaluates map by map. `rand()` uses `Env.rand` (or raises `RandomDependent`).
Anything it cannot evaluate raises `Unknown`.
"""

import re


class Unknown(Exception):
    pass


class WorldDependent(Exception):
    pass


class RandomDependent(Exception):
    pass


class Sym:
    def __init__(self, name, offset=0):
        self.name, self.offset = name, offset

    def __add__(self, k):
        if isinstance(k, bool) or not isinstance(k, int):
            raise Unknown(f"{self} + {k}")
        return Sym(self.name, self.offset + k)

    __radd__ = __add__

    def __sub__(self, k):
        return self + (-k)

    def __eq__(self, o):
        return isinstance(o, Sym) and (self.name, self.offset) == (o.name, o.offset)

    def __hash__(self):
        return hash((self.name, self.offset))

    def __repr__(self):
        return self.name if self.offset == 0 else f"{self.name}+{self.offset}"


# Maps (`ENUM_WORLD`, MapManager.h:6).
WD = {
    "WD_0LORENCIA": 0, "WD_1DUNGEON": 1, "WD_2DEVIAS": 2, "WD_3NORIA": 3, "WD_4LOSTTOWER": 4, "WD_6STADIUM": 6,
    "WD_7ATLANSE": 7, "WD_8TARKAN": 8, "WD_9DEVILSQUARE": 9, "WD_10HEAVEN": 10, "WD_11BLOODCASTLE1": 11,
    "WD_11BLOODCASTLE_END": 17, "WD_18CHAOS_CASTLE": 18, "WD_18CHAOS_CASTLE_END": 23, "WD_24HELLAS": 24,
    "WD_24HELLAS_END": 28, "WD_30BATTLECASTLE": 30, "WD_31HUNTING_GROUND": 31, "WD_33AIDA": 33,
    "WD_34CRYWOLF_1ST": 34, "WD_35CRYWOLF_2ND": 35, "WD_24HELLAS_7": 36, "WD_37KANTURU_1ST": 37,
    "WD_38KANTURU_2ND": 38, "WD_39KANTURU_3RD": 39, "WD_40AREA_FOR_GM": 40, "WD_41CHANGEUP3RD_1ST": 41,
    "WD_42CHANGEUP3RD_2ND": 42, "WD_45CURSEDTEMPLE_LV1": 45, "WD_51HOME_6TH_CHAR": 51,
    "WD_52BLOODCASTLE_MASTER_LEVEL": 52, "WD_53CAOSCASTLE_MASTER_LEVEL": 53, "WD_56MAP_SWAMP_OF_QUIET": 56,
    "WD_57ICECITY": 57, "WD_58ICECITY_BOSS": 58, "WD_62SANTA_TOWN": 62, "WD_63PK_FIELD": 63,
    "WD_64DUELARENA": 64, "WD_65DOPPLEGANGER1": 65, "WD_68DOPPLEGANGER4": 68, "WD_69EMPIREGUARDIAN1": 69,
    "WD_72EMPIREGUARDIAN4": 72, "WD_79UNITEDMARKETPLACE": 79,
}

# Map checks (MapManager.cpp, GM*.cpp). `InBattleCastle()` and `InChaosCastle()` without an argument always
# return `false` in Main 5.2 (`iMap = -1`, MapManager.cpp): a bug from the refactor of the leaked code; here
# they test the current map (what the S6 client did).
WORLD_PREDICATES = {
    "InDevilSquare": lambda w: w == 9,
    "InBattleCastle": lambda w: w == 30,
    "InChaosCastle": lambda w: 18 <= w <= 23 or w == 53,
    "InBloodCastle": lambda w: 11 <= w <= 17 or w == 52,
    "IsPKField": lambda w: w == 63,
    "IsCursedTemple": lambda w: 45 <= w <= 50,
    "IsCyrWolf1st": lambda w: w == 34,
    "IsCyringWolf2nd": lambda w: w == 35,
    "IsInKanturu3rd": lambda w: w == 39,
    "IsBalgasBarrackMap": lambda w: w == 41,
    "IsBalgasRefugeMap": lambda w: w == 42,
}


class Env:
    def __init__(self, type_value=None, world=None, rand=None):
        self.type = type_value
        self.world = world
        self.rand = rand  # list of values, one per rand() call encountered
        self.rand_used = 0
        self.vars = {}
        self.world_used = False


PREC = [["||"], ["&&"], ["==", "!="], ["<", ">", "<=", ">="], ["+", "-"], ["*", "/", "%"]]


def evaluate(toks, env):
    toks = [t for t, _ in toks] if toks and isinstance(toks[0], tuple) else list(toks)
    p = _P(toks, env)
    v = p.expr(0)
    if p.i != len(toks):
        raise Unknown("sobra: " + " ".join(toks[p.i:]))
    return v


def num(s):
    s = s.rstrip("f")
    if s.startswith("0x"):
        return int(s, 16)
    return float(s) if "." in s else int(s)


class _P:
    def __init__(self, t, env):
        self.t, self.i, self.env = t, 0, env

    def peek(self, k=0):
        return self.t[self.i + k] if self.i + k < len(self.t) else None

    def take(self, want=None):
        tok = self.peek()
        if want is not None and tok != want:
            raise Unknown(f"se esperaba {want} y hay {tok}")
        self.i += 1
        return tok

    def expr(self, level):
        if level == len(PREC):
            return self.unary()
        left = self.expr(level + 1)
        while self.peek() in PREC[level]:
            op = self.take()
            right = self.expr(level + 1)
            left = binop(op, left, right)
        return left

    def unary(self):
        tok = self.peek()
        if tok == "!":
            self.take()
            return not truth(self.unary())
        if tok == "-":
            self.take()
            v = self.unary()
            if isinstance(v, Sym):
                raise Unknown("-símbolo")
            return -v
        if tok == "(":
            # `(float)x` cast or parentheses.
            if self.peek(1) in ("float", "int", "BYTE", "short") and self.peek(2) == ")":
                cast = self.peek(1)
                self.i += 3
                v = self.unary()
                return float(v) if cast == "float" else int(v)
            self.take("(")
            v = self.expr(0)
            self.take(")")
            return v
        return self.primary()

    def call_args(self):
        self.take("(")
        depth, args, cur = 1, [], []
        while True:
            tok = self.take()
            if tok == "(":
                depth += 1
            elif tok == ")":
                depth -= 1
                if depth == 0:
                    if cur:
                        args.append(cur)
                    return args
            if tok == "," and depth == 1:
                args.append(cur)
                cur = []
                continue
            cur.append(tok)

    def primary(self):
        tok = self.take()
        env = self.env
        if tok is None:
            raise Unknown("fin")
        if re.match(r"^(\d|\.\d)", tok):
            return num(tok)
        if tok in ("true", "TRUE"):
            return True
        if tok in ("false", "FALSE"):
            return False
        if tok == "NULL":
            return 0
        if tok == "int" and self.peek() == "(":
            self.take("(")
            v = self.expr(0)
            self.take(")")
            return int(v)
        if tok in ("Type", "iType"):
            if env.type is None:
                raise Unknown("Type")
            return env.type
        if tok == "rand" and self.peek() == "(":
            self.call_args()
            if env.rand is None:
                raise RandomDependent()
            v = env.rand[env.rand_used] if env.rand_used < len(env.rand) else 0
            env.rand_used += 1
            return v
        if tok == "gMapManager" and self.peek() == ".":
            self.take(".")
            name = self.take()
            if name == "WorldActive":
                return self.world()
            if name in WORLD_PREDICATES:
                self.call_args()
                return WORLD_PREDICATES[name](self.world())
            raise Unknown("gMapManager." + name)
        if tok in WORLD_PREDICATES and self.peek() == "(":
            self.call_args()
            return WORLD_PREDICATES[tok](self.world())
        if tok in WD:
            return WD[tok]
        if tok in MONSTER_ACTIONS:
            return MONSTER_ACTIONS[tok]
        if tok in env.vars:
            return env.vars[tok]
        if re.match(r"^(MODEL|MODAL|BITMAP|SOUND|KIND|CLASS|PVP|KANTURU2ND|PLAYER)_\w+$", tok):
            return Sym(tok)
        raise Unknown(tok)

    def world(self):
        self.env.world_used = True
        if self.env.world is None:
            raise WorldDependent()
        return self.env.world


# Monster actions (`MONSTER01_*`, _define.h:483-494).
MONSTER_ACTIONS = {
    "MONSTER01_STOP1": 0, "MONSTER01_STOP2": 1, "MONSTER01_WALK": 2, "MONSTER01_ATTACK1": 3,
    "MONSTER01_ATTACK2": 4, "MONSTER01_SHOCK": 5, "MONSTER01_DIE": 6, "MONSTER01_APEAR": 7,
    "MONSTER01_ATTACK3": 8, "MONSTER01_ATTACK4": 9, "MONSTER01_RUN": 10, "MONSTER01_ATTACK5": 11,
}


def truth(v):
    if isinstance(v, Sym):
        raise Unknown("verdad de un símbolo")
    return bool(v)


def binop(op, a, b):
    if op == "||":
        return truth(a) or truth(b)
    if op == "&&":
        return truth(a) and truth(b)
    if op == "==":
        return a == b
    if op == "!=":
        return a != b
    if isinstance(a, Sym) and isinstance(b, Sym) and op == "-" and a.name == b.name:
        return a.offset - b.offset
    if isinstance(a, Sym) or isinstance(b, Sym):
        if op == "+":
            return a + b
        if op == "-" and isinstance(a, Sym):
            return a - b
        raise Unknown(f"{a} {op} {b}")
    if op == "<":
        return a < b
    if op == ">":
        return a > b
    if op == "<=":
        return a <= b
    if op == ">=":
        return a >= b
    if op == "+":
        return a + b
    if op == "-":
        return a - b
    if op == "*":
        return a * b
    if op == "/":
        if isinstance(a, int) and isinstance(b, int):
            return int(a / b)
        return a / b
    if op == "%":
        return a % b
    raise Unknown(op)
