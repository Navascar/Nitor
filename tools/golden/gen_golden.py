"""Знімає «золоті» DMX-кадри зі старого рушія Nitor 1.x (Python).

Запуск (один раз; результат комітиться):
  python tools/golden/gen_golden.py <тека старого репозиторію Nitor 1.x>

Читає tools/golden/scenarios.json і пише crates/nitor-core/tests/golden/golden.json.
Rust-тест tests/golden.rs відтворює ті самі сценарії й порівнює байти.
"""
import json
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, sys.argv[1])
from dmxctl.engine import Engine  # noqa: E402

spec = json.loads((HERE / "scenarios.json").read_text("utf-8"))
OUT = HERE.parent.parent / "crates" / "nitor-core" / "tests" / "golden" / "golden.json"


def make(fixtures, tmp):
    cfg = Path(tmp) / "config.json"
    cfg.write_text(json.dumps({
        "profiles": spec["profiles"], "device_profiles": [],
        "fixtures": [{"name": f["id"], **f} for f in fixtures],
    }), "utf-8")
    eng = Engine(cfg)
    eng._t0 = 0.0
    return eng


def frame(eng, t):
    dmx = eng.render(t)
    return {"t": t, "dmx": dmx.hex(), "live": eng.live}


results = []
with tempfile.TemporaryDirectory() as tmp:
    for case in spec["cases"]:
        eng = make(case["fixtures"], tmp)
        eng.master = case.get("master", 255)
        eng.blackout = case.get("blackout", False)
        if "strobe" in case:
            eng.strobe = case["strobe"]
        eng.raw = {int(k) - 1: v for k, v in case.get("raw", {}).items()}
        results.append({"key": case["name"], "frames": [frame(eng, t) for t in case["times"]]})

    ef = spec["effects"]
    for kind in ef["types"]:
        for vi, var in enumerate(ef["variants"]):
            eng = make(ef["fixtures"], tmp)
            eng.master = ef["master"]
            eng.effect = {"type": kind, **var}
            results.append({"key": f"effect_{kind}_{vi}", "frames": [frame(eng, t) for t in ef["times"]]})

    for fd in spec["fades"]:
        eng = make(fd["fixtures"], tmp)
        eng.scenes = {"s": {"fixtures": fd["scene"], "effect": None}}
        eng.cmd_recall_scene({"name": "s", "fade": fd["dur"]})
        eng._fade["start"] = 0.0
        results.append({"key": fd["name"], "frames": [frame(eng, t) for t in fd["times"]]})

OUT.write_text(json.dumps({"results": results}, ensure_ascii=False, indent=1), "utf-8")
print(f"{len(results)} сценаріїв → {OUT}")
