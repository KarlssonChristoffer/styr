#!/usr/bin/env python3
"""Adapter check: which engine adapters Hub pins (config.rs `@agentclientprotocol/<pkg>@X`) and whether npm has a newer minor/major.
Hub's model list for Claude Code and Codex is what the pinned adapter offers in session/new, so an old pin hides new models (Fable 5.2 needed claude-agent-acp 0.104).
Usage: python3 -I scripts/adapters.py <repo> [--out-dir dir] [--json]; writes <out-dir>/adapters-<date>.md when given; exit 1 when a pin is behind (patch releases float, a pin like @0.104 already takes 0.104.x)."""
import re, sys, json, datetime, urllib.request
A = sys.argv[1:]
def opt(n, d=None): return A[A.index(n) + 1] if n in A else d
R = next(a for a in A if not a.startswith("--") and a != opt("--out-dir"))
src = open(f"{R}/crates/hub-core/src/config.rs", encoding="utf-8").read()
pins = dict(re.findall(r"@agentclientprotocol/([\w-]+)@([\d.]+)", src))
def key(v): return tuple(int(x) for x in re.findall(r"\d+", v.split("-")[0]))
rows = []
for pkg, pin in sorted(pins.items()):
    try:
        with urllib.request.urlopen(f"https://registry.npmjs.org/@agentclientprotocol%2f{pkg}", timeout=20) as r:
            latest = json.load(r)["dist-tags"]["latest"]
    except Exception as e:
        rows.append({"pkg": pkg, "pin": pin, "latest": None, "behind": None, "error": str(e)[:80]}); continue
    p = key(pin)
    rows.append({"pkg": pkg, "pin": pin, "latest": latest, "behind": key(latest)[:len(p)] > p})
red = any(r["behind"] for r in rows)
if "--json" in A: print(json.dumps(rows, indent=1))
else:
    for r in rows:
        s = "BEHIND: bump the pin in config.rs, connections/tests.rs and README.md" if r["behind"] else ("could not ask npm" if r["behind"] is None else "ok")
        print(f"{r['pkg']}: pinned {r['pin']}, npm latest {r['latest']} - {s}")
out = opt("--out-dir")
if out:
    with open(f"{out}/adapters-{datetime.date.today()}.md", "w", encoding="utf-8") as f:
        f.write("| adapter | pinned | npm latest | status |\n|---|---|---|---|\n" + "".join(f"| {r['pkg']} | {r['pin']} | {r['latest']} | {'BEHIND' if r['behind'] else 'unknown' if r['behind'] is None else 'ok'} |\n" for r in rows))
sys.exit(1 if red else 0)
