"""Lists the types (case BITMAP_/MODEL_ ...) of the MuMain particle and joint creation switches and
splits them into chunks of similar size (by lines of code). For planning the port.

    python tools/list_fx_cases.py <MuMain/src/source> <particle_chunks> <joint_chunks>
"""
import json
import re
import sys

SRC = sys.argv[1]
NP = int(sys.argv[2]) if len(sys.argv) > 2 else 6
NJ = int(sys.argv[3]) if len(sys.argv) > 3 else 3


def cases(path, start, end):
    lines = open(f"{SRC}/{path}", encoding="utf-8", errors="replace").read().split("\n")
    out = []  # (type, line)
    depth_line = None
    for i in range(start - 1, min(end, len(lines))):
        for m in re.finditer(r"^\s{8,16}case\s+((?:BITMAP|MODEL)_\w+(?:\s*\+\s*\d+)?)\s*:", lines[i]):
            t = m.group(1).replace(" ", "")
            if t not in [x[0] for x in out]:
                out.append((t, i + 1))
    return out


def chunk(items, end, n):
    # size of each type ≈ lines up to the next one
    sizes = []
    for k, (t, ln) in enumerate(items):
        nxt = items[k + 1][1] if k + 1 < len(items) else end
        sizes.append(max(1, nxt - ln))
    total = sum(sizes)
    target = total / n
    groups, cur, acc = [], [], 0
    for (t, ln), s in zip(items, sizes):
        cur.append(t)
        acc += s
        if acc >= target and len(groups) < n - 1:
            groups.append((cur, acc))
            cur, acc = [], 0
    if cur:
        groups.append((cur, acc))
    return groups


p = cases("Render/Effects/ZzzEffectParticle.cpp", 97, 3850)
j = cases("Render/Effects/ZzzEffectJoint.cpp", 265, 3000)
print(json.dumps({
    "particles": [{"types": g, "create_lines": s} for g, s in chunk(p, 3850, NP)],
    "joints": [{"types": g, "create_lines": s} for g, s in chunk(j, 3000, NJ)],
    "particle_count": len(p),
    "joint_count": len(j),
}, indent=1))
