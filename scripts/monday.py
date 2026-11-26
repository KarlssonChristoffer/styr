#!/usr/bin/env python3
"""Hub Monday pins: the measures too slow or noisy for every pull request (report only). Usage:
python3 -I scripts/monday.py <repo> --out-dir <halsorutin folder> [--hubd <hubd release binary>] [--repo owner/name] [--skip idle,wire,build,merges]
Measures on this machine (tincan): idle CPU % and RSS MB (scripts/idle-matning.py on --hubd), wire bytes (the `wire` test of hub-core's `it` program with HUB_WIRE_OUT: the bytes the phone is sent for a fixed set of threads), build seconds (scripts/code-health.py --build-time: test build + app build; the run of the tests is left out, it is noise). Also counts, with `gh`, the share of last week's merged pull requests whose head never had `hub/ci · tincan` = success (`merge_no_green_pct`, `merges_7d`): the merge rule says wait for green, this says how often that happened. Writes <out-dir>/pins-<date>.json ({"merge_no_green_pct","merges_7d","idle_cpu_pct","idle_rss_mb","wire_bytes","build_s","date","head"}; a pin that could not be measured is left out and named under "skipped"), then compares it with the newest earlier pins-*.json in the same folder by calling scripts/pins.py (same commit on both sides, the measured pins as --extra-*), so the thresholds live in one place: idle CPU +20 %, RSS +10 %, wire +10 %, build +15 %. Prints pins.py's table; exit 1 when a pin is red. With no earlier file it only records. Same machine and binary kind every week, or the compare means nothing. Needs git, Python 3, cargo; the box is small, so the three steps run one after the other, build first (the wire test compiles the same test program, which would leave the build step a warm cache). Build seconds depend on the cache `target/` holds: keep it the same every week (or clean it before) or the +15 % compares cache states."""
import os, sys, json, glob, shutil, tempfile, subprocess, datetime
A = sys.argv[1:]
def opt(n, d=None): return A[A.index(n) + 1] if n in A else d
R = os.path.abspath(A[0])
OUT = opt("--out-dir")
if not OUT: sys.exit("--out-dir <folder> needed")
HUBD = opt("--hubd")
SKIP = set((opt("--skip", "") or "").split(","))
HERE = os.path.dirname(os.path.abspath(__file__))
def run(cmd, cwd=R, env=None, timeout=3600):
    try: return subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired) as e: return e
def idle():
    if not HUBD: return {}, "idle: no --hubd given"
    p = run([sys.executable, "-I", os.path.join(HERE, "idle-matning.py"), HUBD], timeout=300)
    try: d = json.loads(p.stdout)
    except (AttributeError, ValueError): return {}, "idle: no answer"
    if "cpu_pct" not in d: return {}, "idle: " + str(d.get("status", "failed"))
    return {"idle_cpu_pct": d["cpu_pct"], "idle_rss_mb": d["rss_mb"]}, None
def wire():
    tmp = tempfile.mkdtemp(prefix="hub-wire-")
    try:
        f = os.path.join(tmp, "wire.json")
        p = run(["cargo", "test", "-p", "hub-core", "--test", "it", "wire::", "--", "--test-threads=1"], env=dict(os.environ, HUB_WIRE_OUT=f))
        try: return {"wire_bytes": json.load(open(f))["wire_bytes"]}, None
        except (OSError, ValueError, KeyError): return {}, "wire: the test did not write a number" + ("" if getattr(p, "returncode", 0) == 0 else " (it failed)")
    finally: shutil.rmtree(tmp, ignore_errors=True)
def build():
    p = run([sys.executable, "-I", os.path.join(HERE, "code-health.py"), R, "--build-time", "--no-test-run"], timeout=7200)
    try: b = json.loads(p.stdout)["build_time"]
    except (AttributeError, ValueError, KeyError): return {}, "build: no answer"
    parts = [b.get("rust_test_build_s"), b.get("app_build_s")]
    if any(x is None for x in parts): return {}, "build: cargo or npx missing"
    return {"build_s": round(sum(parts), 1)}, None
def merges():
    """Share of the pull requests merged in the last 7 days whose head never had `hub/ci · tincan` = success (the merge rule says wait for it). Docs-only PRs are counted too: they are few."""
    o = None if opt("--repo") else run(["gh", "repo", "view", "--json", "nameWithOwner", "--jq", ".nameWithOwner"])
    repo = opt("--repo") or (getattr(o, "stdout", "") or "").strip()
    if not repo: return {}, "merges: gh could not name the repo"
    since = (datetime.datetime.now(datetime.timezone.utc) - datetime.timedelta(days=7)).strftime("%Y-%m-%dT%H:%M:%SZ")
    q = run(["gh", "api", f"repos/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100", "--jq", f'.[]|select(.merged_at != null and .merged_at >= "{since}")|.head.sha'])
    shas = (getattr(q, "stdout", "") or "").split()
    if not shas: return {}, "merges: no merged pull requests found (or gh failed)"
    green = 0
    for sha in shas:
        r = run(["gh", "api", f"repos/{repo}/commits/{sha}/statuses", "--jq", '[.[]|select(.context=="hub/ci · tincan")]|first|.state // "none"'])
        green += (getattr(r, "stdout", "") or "").strip() == "success"
    return {"merges_7d": len(shas), "merge_no_green_pct": round(100 * (len(shas) - green) / len(shas))}, None
head = (run(["git", "rev-parse", "HEAD"]).stdout or "").strip()
cur, skipped = {"date": datetime.date.today().isoformat(), "head": head}, []
for name, step in (("build", build), ("idle", idle), ("wire", wire), ("merges", merges)):
    if name in SKIP: skipped.append(f"{name}: skipped by --skip"); continue
    got, why = step()
    cur.update(got)
    if why: skipped.append(why)
if skipped: cur["skipped"] = skipped
os.makedirs(OUT, exist_ok=True)
mine = os.path.join(OUT, f"pins-{cur['date']}.json")
earlier = sorted(p for p in glob.glob(os.path.join(OUT, "pins-*.json")) if p != mine)
json.dump(cur, open(mine, "w"), indent=1)
print(json.dumps(cur))
if not earlier: print("first week: recorded, nothing to compare with"); sys.exit(0)
p = run([sys.executable, "-I", os.path.join(HERE, "pins.py"), R, "--head", head or "HEAD", "--base", head or "HEAD", "--extra-base", earlier[-1], "--extra-head", mine], timeout=600)
print(getattr(p, "stdout", "") or str(p))
sys.exit(getattr(p, "returncode", 1))
