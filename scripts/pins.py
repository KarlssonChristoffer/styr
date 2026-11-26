#!/usr/bin/env python3
"""Hub pins: measures that must not get worse in a PR (report only for merging; a red pin must be explained in the PR). Usage:
python3 -I scripts/pins.py <repo> --head <sha> [--base origin/main] [--extra-base base.json --extra-head head.json] [--json] [--line]
Archives both commits (git archive into a temp folder `hub-pins-*`, removed afterwards), measures each and prints a markdown table; --line prints one short line for a commit status; exit 1 when a pin is red. Static pins: Rust tests (#[test]), app tests (it/test in *.test.ts), dead code (scripts/garden.py of the HEAD checkout, run on both trees), `hub_lines` (lines in crates/hub-core/src/hub.rs + hub/: a ceiling that may only shrink, so moving `impl Hub` parts in to shrink the ring shows as red). Measured pins come from JSON you give (--extra-*): idle_cpu_pct and idle_rss_mb (scripts/idle-matning.py), wire_bytes (thread and list size to the phone), build_s (code-health.py --build-time). Red: tests fall, dead or hub_lines rise, idle_cpu_pct +20 %, idle_rss_mb +10 %, wire_bytes +10 %, build_s +15 %. Brittle pins (idle, build) need the same machine and binary kind on both sides, else leave them out."""
import os, re, sys, json, shutil, tempfile, subprocess
A = sys.argv[1:]
def opt(n, d=None): return A[A.index(n) + 1] if n in A else d
R = os.path.abspath(next(a for a in A if not a.startswith("--") and a != opt("--head") and a != opt("--base") and a != opt("--extra-base") and a != opt("--extra-head")))
HEAD, BASE = opt("--head"), opt("--base", "origin/main")
if not HEAD: sys.exit("--head <sha> needed")
HERE = os.path.dirname(os.path.abspath(__file__))
def tree(ref, dest):
    import tarfile
    os.makedirs(dest)
    p = subprocess.Popen(["git", "-C", R, "archive", "--format=tar", ref], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    with tarfile.open(fileobj=p.stdout, mode="r|") as t:
        for m in t:
            try: t.extract(m, dest, **({"filter": "data"} if hasattr(tarfile, "data_filter") else {}))
            except (OSError, tarfile.TarError): pass  # a symlink or odd entry Windows refuses: not source we count
    if p.wait(): raise SystemExit(f"git archive {ref} failed")
def walk(d, exts):
    for r, ds, fs in os.walk(d):
        ds[:] = [x for x in ds if x not in ("node_modules", "target", ".git", "bindings")]
        for f in fs:
            if f.endswith(exts): yield os.path.join(r, f)
def rd(p):
    try: return open(p, encoding="utf-8", errors="ignore").read()
    except OSError: return ""
def measure(d):
    rs = {p: rd(p) for p in walk(d, (".rs",))}
    app = {p: rd(p) for p in walk(os.path.join(d, "app/src"), (".ts", ".svelte"))}
    m = {"tests_rust": sum(len(re.findall(r"#\[(?:tokio::)?test", s)) for s in rs.values()),
         "tests_app": sum(len(re.findall(r"^\s*(?:it|test)(?:\.each)?\(", s, re.M)) for p, s in app.items() if p.endswith(".test.ts"))}
    m["hub_lines"] = sum(rs[p].count("\n") + 1 for p in rs if re.search(r"crates/hub-core/src/hub(\.rs|/)", p.replace("\\", "/")))
    g = subprocess.run([sys.executable, "-I", os.path.join(HERE, "garden.py"), d, "--json"], capture_output=True, text=True)
    try: m["dead"] = json.loads(g.stdout)["dead"]
    except Exception: pass
    return m
tmp = tempfile.mkdtemp(prefix="hub-pins-")
try:
    b, h = os.path.join(tmp, "base"), os.path.join(tmp, "head")
    tree(BASE, b); tree(HEAD, h)
    mb, mh = measure(b), measure(h)
finally:
    shutil.rmtree(tmp, ignore_errors=True)
for k, f in (("--extra-base", mb), ("--extra-head", mh)):
    if opt(k): f.update(json.loads(rd(opt(k))))
# key: (worse when, tolerance as a fraction; None = any change)
RULES = {"tests_rust": ("down", 0), "tests_app": ("down", 0), "dead": ("up", 0), "hub_lines": ("up", 0),
         "idle_cpu_pct": ("up", 0.20), "idle_rss_mb": ("up", 0.10), "wire_bytes": ("up", 0.10), "merge_no_green_pct": ("up", 0.10), "build_s": ("up", 0.15)}
rows, red = [], []
for k, (way, tol) in RULES.items():
    if k not in mb or k not in mh: continue
    a, c = mb[k], mh[k]
    bad = (c < a) if way == "down" else (c > a * (1 + tol) if a else c > 0)
    rows.append((k, a, c, bad))
    if bad: red.append(f"{k} {a}→{c}")
if "--json" in A: print(json.dumps({"base": mb, "head": mh, "red": red}))
elif "--line" in A: print("ok: " + ", ".join(f"{k} {c}" for k, a, c, _ in rows) + ("" if "dead" in mh else " (weeds not measured)") if not red else "red: " + ", ".join(red))
else:
    print("| pin | base | head | |\n|---|---|---|---|")
    for k, a, c, bad in rows: print(f"| {k} | {a} | {c} | {'🔴' if bad else '✓'} |")
sys.exit(1 if red else 0)
