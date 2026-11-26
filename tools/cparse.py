"""Mini C++ reader for the table generators (only what the Main 5.2 code uses).

Strips comments and whatever is outside active #ifdefs, tokenizes with line numbers and builds a
statement tree: blocks, if/else, switch/case, for and plain statements (up to the `;`). It does not
understand C++: just enough to walk the table `switch`es.
"""

import re

# Active Main 5.2 macros: Defined_Global.h, Winmain.h and the project ones (Main.vcxproj).
DEFINED = {
    "PJH_NEW_SERVER_SELECT_MAP", "ASG_ADD_GENS_SYSTEM", "ASG_ADD_INFLUENCE_GROUND_EFFECT", "ASG_ADD_GENS_MARK",
    "PBG_MOD_STRIFE_GENSMARKRENDER", "PBG_ADD_GENSRANKING", "KJH_PBG_ADD_INGAMESHOP_SYSTEM",
    "PBG_ADD_INGAMESHOP_UI_MAINFRAME", "PBG_ADD_INGAMESHOP_UI_ITEMSHOP", "PBG_ADD_NAMETOPMSGBOX",
    "KJH_ADD_INGAMESHOP_UI_SYSTEM", "KJH_ADD_PERIOD_ITEM_SYSTEM", "PBG_ADD_INGAMESHOPMSGBOX", "PBG_ADD_ITEMRESIZE",
    "KJH_MOD_SHOP_SCRIPT_DOWNLOAD", "PBG_ADD_CHARACTERCARD", "PJH_ADD_PANDA_PET", "PJH_ADD_PANDA_CHANGERING",
    "NEW_PROTOCOL_SYSTEM", "FOR_WORK", "ENABLE_EDIT", "CSK_LH_DEBUG_CONSOLE", "CSK_DEBUG_MAP_ATTRIBUTE",
    "CSK_DEBUG_RENDER_BOUNDINGBOX", "CSK_DEBUG_MAP_PATHFINDING", "WIN32", "_WINDOWS", "_LANGUAGE_FOREIGN",
    "_LANGUAGE_ENG", "NDEBUG",
    # Off in Main 5.2, but the code is there and the S6 server (OpenMU) spawns these
    # monsters and NPCs: Karutan (MuMain enables them too, Defined_Global.h:33-34), the timed quest NPCs
    # (566-568) and David (579).
    "ASG_ADD_MAP_KARUTAN", "ASG_ADD_KARUTAN_MONSTERS", "ASG_ADD_KARUTAN_NPC", "ASG_ADD_TIME_LIMIT_QUEST_NPC",
    "LEM_ADD_LUCKYITEM",
}


def read_source(path):
    with open(path, encoding="cp1252", errors="replace") as f:
        return f.read().split("\n")


def active_text(lines):
    """[(line number, text)] without comments and without what is outside active #ifdefs."""
    out = []
    stack = []
    in_block = False
    for n, raw in enumerate(lines, 1):
        line = raw
        # /* */ comments (may span lines) and //.
        res = ""
        i = 0
        while i < len(line):
            if in_block:
                j = line.find("*/", i)
                if j < 0:
                    i = len(line)
                    break
                in_block = False
                i = j + 2
                continue
            if line.startswith("/*", i):
                in_block = True
                i += 2
                continue
            if line.startswith("//", i):
                break
            if line[i] == '"':
                j = i + 1
                while j < len(line) and line[j] != '"':
                    j += 2 if line[j] == "\\" else 1
                res += line[i:j + 1]
                i = j + 1
                continue
            res += line[i]
            i += 1
        s = res.strip()
        m = re.match(r"#\s*(ifdef|ifndef|if|elif|else|endif)\b\s*(.*)", s)
        if m:
            kind, cond = m.group(1), m.group(2).strip()
            if kind == "ifdef":
                stack.append(cond in DEFINED)
            elif kind == "ifndef":
                stack.append(cond not in DEFINED)
            elif kind == "if":
                mm = re.match(r"defined\s*\(?\s*(\w+)", cond)
                stack.append(bool(mm and mm.group(1) in DEFINED) or cond == "1")
            elif kind == "elif":
                stack[-1] = False
            elif kind == "else":
                stack[-1] = not stack[-1]
            else:
                stack.pop()
            continue
        if s.startswith("#"):
            continue
        if all(stack) and s:
            out.append((n, res))
    return out


TOKEN = re.compile(
    r'\s*(?:(?P<str>"(?:\\.|[^"\\])*")|(?P<chr>\'(?:\\.|[^\'\\])*\')|(?P<num>\d+\.\d*f?|\.\d+f?|\d+f?|0x[0-9a-fA-F]+)'
    r"|(?P<id>[A-Za-z_]\w*)|(?P<op>->|::|==|!=|<=|>=|&&|\|\||\+=|-=|\*=|/=|\+\+|--|<<|>>|[{}()\[\];:,.<>=!+\-*/%&|^~?]))"
)


def tokenize(text_lines):
    toks = []
    for n, line in text_lines:
        pos = 0
        while pos < len(line):
            m = TOKEN.match(line, pos)
            if not m or m.end() == pos:
                if line[pos:].strip() == "":
                    break
                pos += 1
                continue
            kind = m.lastgroup
            toks.append((m.group(kind), n))
            pos = m.end()
    return toks


class Node:
    def __init__(self, kind, line, **kw):
        self.kind = kind
        self.line = line
        self.__dict__.update(kw)

    def __repr__(self):
        return f"<{self.kind}@{self.line}>"


class Parser:
    def __init__(self, toks):
        self.t = toks
        self.i = 0

    def peek(self, k=0):
        j = self.i + k
        return self.t[j][0] if j < len(self.t) else None

    def line(self):
        return self.t[self.i][1] if self.i < len(self.t) else -1

    def take(self, want=None):
        tok = self.t[self.i]
        if want is not None and tok[0] != want:
            raise SyntaxError(f"expected {want!r}, got {tok[0]!r} at line {tok[1]}")
        self.i += 1
        return tok[0]

    def group(self, open_, close):
        """Tokens between balanced parentheses (or brackets), without the delimiters."""
        self.take(open_)
        depth, out = 1, []
        while True:
            tok = self.t[self.i]
            self.i += 1
            if tok[0] == open_:
                depth += 1
            elif tok[0] == close:
                depth -= 1
                if depth == 0:
                    return out
            out.append(tok)

    def block(self):
        line = self.line()
        self.take("{")
        items = []
        while self.peek() != "}":
            items.append(self.statement())
        self.take("}")
        return Node("block", line, items=items)

    def statement(self):
        tok = self.peek()
        line = self.line()
        if tok == "{":
            return self.block()
        if tok == ";":
            self.take()
            return Node("empty", line)
        if tok == "if":
            self.take()
            cond = self.group("(", ")")
            then = self.statement()
            other = None
            if self.peek() == "else":
                self.take()
                other = self.statement()
            return Node("if", line, cond=cond, then=then, other=other)
        if tok == "switch":
            self.take()
            expr = self.group("(", ")")
            body = self.block()
            return Node("switch", line, expr=expr, items=body.items)
        if tok == "case":
            self.take()
            expr = []
            while not (self.peek() == ":" and self.peek(1) != ":"):
                expr.append(self.t[self.i])
                self.i += 1
            self.take(":")
            return Node("case", line, expr=expr)
        if tok == "default" and self.peek(1) == ":":
            self.take()
            self.take(":")
            return Node("default", line)
        if tok == "for" or tok == "while":
            self.take()
            head = self.group("(", ")")
            body = self.statement()
            return Node(tok, line, head=head, body=body)
        if tok == "do":
            self.take()
            body = self.statement()
            self.take("while")
            head = self.group("(", ")")
            self.take(";")
            return Node("while", line, head=head, body=body)
        if tok == "break":
            self.take()
            self.take(";")
            return Node("break", line)
        if tok == "return":
            self.take()
            expr = []
            while self.peek() != ";":
                expr.append(self.t[self.i])
                self.i += 1
            self.take(";")
            return Node("return", line, expr=expr)
        # Plain statement up to the `;` (with balanced parentheses and braces).
        toks = []
        depth = 0
        while True:
            t = self.t[self.i]
            if t[0] in "([{" and len(t[0]) == 1:
                depth += 1
            elif t[0] in ")]}" and len(t[0]) == 1:
                depth -= 1
            if t[0] == ";" and depth == 0:
                self.i += 1
                break
            toks.append(t)
            self.i += 1
        return Node("stmt", line, toks=toks)


def function_body(lines, start_line):
    """Tree of the body of the function starting at `start_line` (1-based, its signature line)."""
    text = active_text(lines)
    toks = tokenize([(n, s) for n, s in text if n >= start_line])
    p = Parser(toks)
    while p.peek() != "{":
        p.i += 1
    return p.block()


def text_of(toks):
    out = ""
    prev = None
    for t, _ in toks:
        if prev is not None and (re.match(r"\w", t) and re.match(r"\w", prev[-1])):
            out += " "
        out += t
        prev = t
    return out
