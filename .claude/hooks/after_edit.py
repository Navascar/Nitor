"""Після правки файлу: rustfmt для *.rs (стиль — з rustfmt.toml у корені)."""
import json
import os
import shutil
import subprocess
import sys

try:
    data = json.load(sys.stdin)
except ValueError:
    sys.exit(0)
path = (data.get("tool_input") or {}).get("file_path") or ""
if not path.endswith(".rs") or not os.path.exists(path):
    sys.exit(0)
rustfmt = shutil.which("rustfmt")
if not rustfmt:
    sys.exit(0)
root = os.environ.get("CLAUDE_PROJECT_DIR", ".")
r = subprocess.run([rustfmt, "--edition", "2024", "--config-path", os.path.join(root, "rustfmt.toml"), path],
                   capture_output=True, text=True)
if r.returncode != 0:
    # синтаксична помилка — показуємо, щоб її виправили одразу
    print(r.stderr.strip()[-1500:], file=sys.stderr)
    sys.exit(2)
