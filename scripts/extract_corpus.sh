#!/usr/bin/env bash
set -euo pipefail
SRC=${1:-$HOME/repos/tree-sitter-q/test/corpus}
DST=crates/parser/tests/data/corpus
mkdir -p "$DST"
rm -f "$DST"/*.q

python3 - "$SRC" "$DST" <<'PY'
import re, sys, pathlib
src_dir, dst_dir = sys.argv[1], sys.argv[2]
header = re.compile(r"^=+$")
sep    = re.compile(r"^-+$")
for path in sorted(pathlib.Path(src_dir).glob("*.txt")):
    text = path.read_text()
    lines = text.splitlines()
    i, n = 0, len(lines)
    idx = 0
    while i < n:
        if header.match(lines[i]):
            j = i + 1
            while j < n and not header.match(lines[j]):
                j += 1
            if j >= n:
                break
            name = "_".join(lines[i+1:j]).strip()
            name = re.sub(r"[^A-Za-z0-9_]", "_", name) or f"case{idx}"
            i = j + 1
            buf = []
            while i < n and not sep.match(lines[i]):
                buf.append(lines[i])
                i += 1
            slug = f"{path.stem}__{name}__{idx}.q"
            (pathlib.Path(dst_dir)/slug).write_text("\n".join(buf).rstrip() + "\n")
            idx += 1
            while i < n and not header.match(lines[i]):
                i += 1
        else:
            i += 1
PY

count=$(ls "$DST"/*.q 2>/dev/null | wc -l | tr -d ' ')
echo "extracted $count corpus inputs to $DST"
