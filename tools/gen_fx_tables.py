"""Genera las tablas del motor de efectos a partir del código de MuMain (solo lectura).

    python tools/gen_fx_tables.py <ruta a MuMain/src/source>

Escribe en crates/mu-client/src/fx/tables/:
- ids.rs      constantes BITMAP_*, MODEL_*, AT_SKILL_*, PLAYER_*... con sus valores exactos
              (enums de Core/Globals/_TextureIndex.h y _enum.h, con las macros activas de
              Defined_Global.h), para que el código traducido use los mismos números.
- bitmaps.rs  (id, archivo en Data/, repetir) de cada LoadBitmap de Engine/Object/ZzzOpenData.cpp.
- models.rs   (id, carpeta, nombre de archivo .bmd, carpeta de texturas) de cada AccessModel.
"""

import os
import re
import sys

SRC = sys.argv[1] if len(sys.argv) > 1 else r"..\referencias\MuMain\src\source"
OUT = os.path.join(os.path.dirname(__file__), "..", "crates", "mu-client", "src", "fx", "tables")


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
    # `constexpr int NOMBRE = valor;` también cuenta como constante.
    for m in re.finditer(r"constexpr\s+(?:unsigned\s+)?(?:int|short|long|BYTE|WORD|DWORD|size_t)\s+(\w+)\s*=\s*([^;]+);", text):
        numeric.setdefault(m.group(1), m.group(2).strip())


def active_lines(text):
    """Quita lo que queda fuera de #ifdef/#ifndef/#if defined según las macros conocidas."""
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
                    # Valor que no sabemos calcular (p. ej. un sizeof): se salta el resto del enum.
                    print(f"  {origin}: no se pudo evaluar {n} = {e.strip()}", file=sys.stderr)
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
    print(f"{rel}: {parse_enums(read(rel), rel)} valores", file=sys.stderr)

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
        print(f"  LoadBitmap: id desconocido {idexpr}", file=sys.stderr)
        continue
    path = path.replace("\\\\", "/").replace("\\", "/")
    bitmaps[i] = (path, wrap == "GL_REPEAT", idexpr.replace(" ", ""))  # la última carga gana

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
        print(f"  AccessModel: id desconocido {idexpr}", file=sys.stderr)
        continue
    d = d.replace("\\\\", "/").replace("\\", "/").rstrip("/")
    if d.startswith("Data/"):
        d = d[5:]
    # AccessModel(..., "Name", n) → Name0n.bmd; sin número → Name.bmd
    fname = f"{name}{int(idx):02d}.bmd" if idx is not None and int(idx) > 0 else f"{name}.bmd"
    models[i] = (d, fname, idexpr.replace(" ", ""))

# --- Salida ---------------------------------------------------------------------------------

os.makedirs(OUT, exist_ok=True)
HEADER = "// Generado por tools/gen_fx_tables.py a partir de MuMain. No editar a mano.\n"
WANT = re.compile(r"^(BITMAP_|MODEL_|AT_SKILL_|PLAYER_|MONSTER01_|SKILL_|ITEM_)")

with open(os.path.join(OUT, "ids.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "#![allow(dead_code, non_upper_case_globals)]\n\n")
    for n, v in sorted(values.items(), key=lambda kv: (kv[1], kv[0])):
        if WANT.match(n):
            f.write(f"pub const {n}: i32 = {v};\n")

with open(os.path.join(OUT, "bitmaps.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "\n/// (id, archivo bajo Data/ tal como lo pide el original, repetir la textura).\n")
    f.write("pub const BITMAPS: &[(i32, &str, bool)] = &[\n")
    for i, (path, rep, expr) in sorted(bitmaps.items()):
        f.write(f'    ({i}, "{path}", {"true" if rep else "false"}), // {expr}\n')
    f.write("];\n")

with open(os.path.join(OUT, "models.rs"), "w", encoding="utf-8") as f:
    f.write(HEADER + "\n/// (id, carpeta bajo Data/, archivo .bmd, carpeta de sus texturas bajo Data/).\n")
    f.write("pub const MODELS: &[(i32, &str, &str, &str)] = &[\n")
    for i, (d, fname, expr) in sorted(models.items()):
        td = texdirs.get(i, d)
        f.write(f'    ({i}, "{d}", "{fname}", "{td}"), // {expr}\n')
    f.write("];\n")

print(
    f"ids: {sum(1 for n in values if WANT.match(n))}, bitmaps: {len(bitmaps)}, models: {len(models)}",
    file=sys.stderr,
)
