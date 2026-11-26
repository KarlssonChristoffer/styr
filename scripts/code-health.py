#!/usr/bin/env python3
"""Hub code-health measurements (report only, never blocks). Usage: python3 -I scripts/code-health.py <repo> [--coverage] [--audit] [--pr-labels] > health.json; compare two runs to see the trend. Grades and background: docs of the first report in the project files (kodhalsa-2026-11-06).
--coverage adds "coverage" (Rust via cargo-llvm-cov, app via vitest v8; slow: runs the tests). --coverage-app is the same for the app only (fast, the weekly routine uses it). --build-time adds "build_time" (seconds; --no-test-run leaves out the run of the integration tests; whatever cache the machine has, so compare like with like). --table prints the A-E table (markdown) instead of the JSON. "grades" is always in the JSON: per area A-E from the goals in kodhalsa-malplan (A = all measured goals met, B >= 75 %, C 50, D 25, else E); a goal that was not measured (flag missing) counts as unmeasured, not as failed. "structure" is always in the JSON: dependency cycle, ranked modules, Arc<Hub>, untyped events, spawn, HTTP clients (the structure figures of hallbarhet-2026-11-09). --pr-labels adds "pr_labels": merged PRs and `bug` PRs per `area:` label over 7 days (gh, network). --audit adds "security" (cargo audit, npm audit; needs network). A missing tool or a failed step gives {"status": ...} and never fails the script; a floor on coverage is informative only until Christoffer decides otherwise."""
import os, re, sys, json, subprocess, collections, tempfile, shutil
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
FLAGS = {a for a in sys.argv[1:] if a.startswith("--")}
R = ARGS[0]
def files(exts, roots):
    for r in roots:
        for d, ds, fs in os.walk(os.path.join(R, r)):
            ds[:] = [x for x in ds if x not in ("node_modules", "target", ".git", "bindings")]
            for f in fs:
                if f.endswith(exts): yield os.path.join(d, f)
def rd(p):
    try: return open(p, encoding="utf-8", errors="ignore").read()
    except: return ""
def split_test(src):
    i = src.find("#[cfg(test)]")
    return (src, "") if i < 0 else (src[:i], src[i:])
def area(p):
    p = os.path.relpath(p, R)
    if p.startswith("crates/hub-core/tests"): return "tests"
    m = re.match(r"crates/hub-core/src/([^/.]+)", p)
    if p.startswith("crates/hub-core/src/"): return "core/" + m.group(1)
    if p.startswith("crates/"): return p.split("/")[1]
    if p.startswith("app/src-tauri"): return "tauri-skal"
    if p.startswith("app/src/lib/components"): return "ui/components"
    if p.startswith("app/src"): return "ui/lib"
    if p.startswith("ios"): return "ios"
    return p.split("/")[0]
out = {}
rs = list(files((".rs",), ["crates", "app/src-tauri/src"]))
svel = list(files((".svelte", ".ts"), ["app/src"]))
sw = list(files((".swift",), ["ios"]))
# file sizes
sizes = {}
for p in rs + svel + sw: sizes[os.path.relpath(p, R)] = rd(p).count("\n") + 1
out["files"] = {"rs": len(rs), "svelte_ts": len(svel), "swift": len(sw)}
out["loc"] = {k: sum(v for p, v in sizes.items() if p.endswith(e)) for k, e in [("rs", ".rs"), ("svelte", ".svelte"), ("ts", ".ts"), ("swift", ".swift")]}
# per-area production stats for rust
ar = collections.defaultdict(lambda: collections.Counter())
fn_lens = []
fn_re = re.compile(r"^(\s*)(?:pub(?:\([a-z]+\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+(\w+)", re.M)
for p in rs:
    src = rd(p); prod, tst = split_test(src)
    a = area(p)
    is_test_file = "/tests/" in p or os.path.basename(p) in ("tests.rs", "test.rs") or p.endswith("_tests.rs")
    c = ar[a]
    c["files"] += 1
    c["loc"] += src.count("\n")
    c["test_loc"] += (src.count("\n") if is_test_file else tst.count("\n"))
    if not is_test_file:
        c["unwrap"] += len(re.findall(r"\.unwrap\(\)", prod))
        c["expect"] += len(re.findall(r"\.expect\(", prod))
        c["panic"] += len(re.findall(r"\bpanic!|unreachable!", prod))
        c["lock_unwrap"] += len(re.findall(r"lock\(\)\.unwrap\(\)", prod))
        c["let_underscore"] += len(re.findall(r"\blet _ =", prod))
        c["unsafe"] += len(re.findall(r"\bunsafe\b", prod))
        c["tests_fn"] += len(re.findall(r"#\[(?:tokio::)?test", tst))
        if not re.match(r"\s*(//!|#!\[)", src) and src.strip(): c["no_header"] += 1
        # function length (brace matching, rough)
        lines = prod.split("\n")
        for m in fn_re.finditer(prod):
            s = prod.count("\n", 0, m.start())
            depth = 0; started = False; e = s
            for i in range(s, min(len(lines), s + 1500)):
                depth += lines[i].count("{") - lines[i].count("}")
                if "{" in lines[i]: started = True
                if started and depth <= 0: e = i; break
                if not started and lines[i].rstrip().endswith(";"): e = i; break
            fn_lens.append((e - s + 1, m.group(2), os.path.relpath(p, R)))
    else:
        c["tests_fn"] += len(re.findall(r"#\[(?:tokio::)?test", src))
out["rust_areas"] = {a: dict(c) for a, c in ar.items()}
fn_lens.sort(reverse=True)
out["fn_total"] = len(fn_lens); out["fn_top"] = fn_lens[:15]
# svelte/ts
ui = collections.defaultdict(collections.Counter)
for p in svel:
    s = rd(p); a = area(p); c = ui[a]
    c["files"] += 1; c["loc"] += s.count("\n")
    if p.endswith(".svelte"):
        c["svelte"] += 1
        if not re.match(r"\s*<!--", s): c["no_header"] += 1
    else:
        if not re.match(r"\s*(//|/\*)", s) and not p.endswith(".test.ts"): c["no_header"] += 1   # a test file is named after the module it tests
    c["catch_quiet"] += len(re.findall(r"\.catch\(\s*\(\s*\w*\s*\)\s*=>\s*(\{\s*\}|undefined|null)\s*\)", s))
    c["setInterval"] += s.count("setInterval")
    c["any"] += len(re.findall(r":\s*any\b|\bas any\b", s))
    c["ignores"] += len(re.findall(r"@ts-ignore|@ts-expect-error|svelte-ignore", s))
    c["browser_controls"] += len(re.findall(r"<select\b|<input[^>]*type=\"(?:checkbox|radio|range)\"", s))
    c["tests"] += len(re.findall(r"\b(?:it|test)\(", s)) if re.search(r"\.(test|spec)\.", p) else 0
out["ui_areas"] = {a: dict(c) for a, c in ui.items()}
# swift
swc = collections.Counter()
for p in sw:
    s = rd(p); swc["files"] += 1; swc["loc"] += s.count("\n")
    swc["force_unwrap_try!"] += len(re.findall(r"try!|as!", s))
    swc["tests"] += len(re.findall(r"func test", s))
    if not re.match(r"\s*//", s): swc["no_header"] += 1
out["swift"] = dict(swc)
# docs
docs = os.path.join(R, "docs")
out["docs_lines"] = {f: rd(os.path.join(docs, f)).count("\n") for f in os.listdir(docs) if f.endswith(".md")}
arch = os.path.join(docs, "architecture")
if os.path.isdir(arch):
    out["docs_lines"].update({"architecture/" + f: rd(os.path.join(arch, f)).count("\n") for f in os.listdir(arch) if f.endswith(".md")})
# churn
def git(*a): return subprocess.run(["git", "-C", R, *a], capture_output=True, text=True).stdout
log = git("log", "--since=2026-10-28", "--name-only", "--format=@@%H")
ch = collections.Counter(); commits = 0
for l in log.split("\n"):
    if l.startswith("@@"): commits += 1
    elif l.strip(): ch[l.strip()] += 1
out["commits"] = commits
out["churn_top"] = [(n, f) for f, n in ch.most_common(25)]
# fix-commits per area
fixes = collections.Counter(); allc = collections.Counter()
cur = None
for l in git("log", "--since=2026-10-28", "--name-only", "--format=@@%s").split("\n"):
    if l.startswith("@@"): cur = l[2:]; isfix = bool(re.match(r"(fix|bug|hotfix|revert)\b|.*\bfix(es|ed)?\b", cur, re.I)); seen = set()
    elif l.strip():
        a = area(os.path.join(R, l.strip()))
        if a in seen: continue
        seen.add(a); allc[a] += 1
        if isfix: fixes[a] += 1
out["fix_share"] = {a: (fixes[a], allc[a]) for a in allc if allc[a] >= 15}
# structure (is the next change dearer than the last?): dependency cycle, ranked modules, Hub as a bag, untyped events, owner-less infrastructure
def structure():
    core = os.path.join(R, "crates/hub-core/src")
    if not os.path.isdir(core): return {"status": "saknas: hub-core/src"}
    mods = set()
    for e in os.listdir(core):
        if e.endswith(".rs"): mods.add(e[:-3])
        elif os.path.isdir(os.path.join(core, e)): mods.add(e)
    mods.discard("lib")
    edges = collections.defaultdict(set); loc = collections.Counter()
    hub_impl = arc_hub = spawn = http = json_new = json_read = 0; spawn_files = set()
    for d, ds, fs in os.walk(core):
        for f in fs:
            if not f.endswith(".rs"): continue
            path = os.path.join(d, f); rel = os.path.relpath(path, core)
            frm = rel.split(os.sep)[0].removesuffix(".rs")
            loc[frm] += rd(path).count("\n")
            if f == "tests.rs" or f.endswith("_tests.rs"): continue
            src = split_test(rd(path))[0]
            src = "\n".join(l.split("//")[0] for l in src.split("\n"))
            for m in re.finditer(r"(?<![\w$])crate::(\{(?:[^{}]|\{[^{}]*\})*\}|\w+)", src):
                g = m.group(1)
                names = re.findall(r"(?:^\{|,)\s*(\w+)", g) if g.startswith("{") else [g]
                for n in names:
                    if n in mods and n != frm: edges[frm].add(n)
            if re.search(r"impl\s+(?:\w+\s+for\s+)?Hub\b", src): hub_impl += 1
            arc_hub += len(re.findall(r"Arc<Hub>", src))
            n = len(re.findall(r"tokio::spawn\(", src)); spawn += n
            if n: spawn_files.add(rel)
            http += len(re.findall(r"Client::new\(\)|Client::builder\(\)", src))
            json_new += len(re.findall(r"\bjson!\(", src)); json_read += len(re.findall(r'\bdata\["', src))
    idx = {}; low = {}; stack = []; on = set(); comps = []; counter = [0]
    sys.setrecursionlimit(10000)
    def sc(v):  # Tarjan; ~200 nodes, recursion is fine
        idx[v] = low[v] = counter[0]; counter[0] += 1; stack.append(v); on.add(v)
        for w in edges.get(v, ()):
            if w not in idx: sc(w); low[v] = min(low[v], low[w])
            elif w in on: low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            c = []
            while True:
                w = stack.pop(); on.discard(w); c.append(w)
                if w == v: break
            comps.append(c)
    for v in sorted(mods):
        if v not in idx: sc(v)
    big = max(comps, key=len)
    ranked = None
    mm = re.search(r"const LAYERS[^=]*=\s*&\[(.*?)\n\];", rd(os.path.join(R, "crates/hub-core/tests/it/layers.rs")), re.S)
    if mm: ranked = len(re.findall(r'"(\w+)"', re.sub(r"//[^\n]*", "", mm.group(1))))
    em = re.search(r"pub enum EventKind \{(.*?)\n\}", rd(os.path.join(R, "crates/hub-core/src/model/thread.rs")), re.S)
    kinds = len(re.findall(r"^\s{4}[A-Z]\w*\s*(?:,|\(|\{|$)", em.group(1), re.M)) if em else None
    # kinds whose payload has no struct: every kind but `unknown` has a row in the registry (`event_kinds!`), so 0;
    # the golden-log test fails on a kind without one
    rows = len(re.findall(r"^    \w+, \w+ => \w+;", rd(os.path.join(R, "crates/hub-core/src/model/event_data.rs")), re.M))
    no_struct = max(0, kinds - 1 - rows) if kinds else None
    app_api = sum(1 for p in svel if p.endswith(".svelte") and re.search(r"\bapi\.\w+", rd(p)))
    app_fetch = sum(len(re.findall(r"\bfetch\(", rd(p))) for p in svel if not p.endswith(("api.ts", ".test.ts")))
    return {"status": "ok", "modules": len(mods), "cycle_modules": len(big), "cycle_loc": sum(loc[m] for m in big), "ranked_modules": ranked,
            "impl_hub_files": hub_impl, "arc_hub": arc_hub, "event_kinds": kinds, "event_kinds_without_struct": no_struct, "json_build": json_new, "json_read": json_read,
            "tokio_spawn": spawn, "tokio_spawn_files": len(spawn_files), "own_http_clients": http,
            "app_components_calling_api": app_api, "app_fetch_outside_api": app_fetch}
out["structure"] = structure()

def pr_labels():
    """Merged PRs of the last 7 days and how many carry `bug` + `area:x` (CLAUDE.md "Labels"); needs gh and network."""
    import datetime
    since = (datetime.date.today() - datetime.timedelta(days=7)).isoformat()
    q = "repo:karlssonchristoffer/hub is:pr is:merged merged:>=" + since
    o, err = run(["gh", "api", "-X", "GET", "search/issues", "-f", "q=" + q, "-f", "per_page=1", "--jq", ".total_count"], R, 60)
    if o is None: return {"status": err}
    o2, _ = run(["gh", "api", "-X", "GET", "search/issues", "-f", "q=" + q + " label:bug", "-f", "per_page=100", "--jq",
                 '[.items[].labels[].name | select(startswith("area:"))] | group_by(.) | map({(.[0]): length}) | add // {}'], R, 60)
    areas = json.loads(o2) if o2 else {}
    return {"status": "ok", "merged_7d": int(o.strip() or 0), "bug_by_area": areas, "bug_7d_with_area": sum(areas.values())}
# coverage and security: opt in, each step isolated so one missing tool leaves the rest
def run(cmd, cwd, timeout):
    """(stdout, error). error is a short status text for the JSON; never raises."""
    if shutil.which(cmd[0]) is None: return None, "saknas: " + cmd[0]
    try:
        r = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired: return None, "timeout"
    return (r.stdout, None) if r.stdout.strip() else (None, "fel: " + r.stderr.strip()[-200:])
def pct(d): return round(100.0 * d["covered"] / d["count"], 1) if d.get("count") else None
def cov_rust():
    tmp = tempfile.mkdtemp(prefix="hub-cov-")
    try:
        f = os.path.join(tmp, "rust.json")
        _, err = run(["cargo", "llvm-cov", "--workspace", "--exclude", "hub-app", "--json", "--summary-only", "--output-path", f], R, 5400)
        hint = "cargo install cargo-llvm-cov; rustup component add llvm-tools-preview" if err and err.startswith(("saknas", "fel")) else "runs the whole test suite instrumented: slow on 4 cores (over 50 min), run it on the 6-core machine"
        if not os.path.exists(f): return {"status": err or "ingen utdata", "hint": hint}
        data = json.load(open(f))["data"][0]
        ar = collections.defaultdict(lambda: {"count": 0, "covered": 0})
        for e in data["files"]:
            rel = os.path.relpath(e["filename"], R)
            if "/tests/" in rel or rel.startswith("crates/mock-agent"): continue
            a = ar[area(e["filename"])]; a["count"] += e["summary"]["lines"]["count"]; a["covered"] += e["summary"]["lines"]["covered"]
        t = data["totals"]["lines"]
        return {"status": "ok", "lines_pct": round(t["percent"], 1), "areas": {k: pct(v) for k, v in sorted(ar.items())}}
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
        run(["cargo", "llvm-cov", "clean", "--profraw-only"], R, 120)  # a timed-out run leaves *.profraw in the crates
def cov_app():
    tmp = tempfile.mkdtemp(prefix="hub-cov-")
    try:
        _, err = run(["npx", "vitest", "run", "--coverage", "--coverage.reporter=json-summary", "--coverage.reportsDirectory=" + tmp], os.path.join(R, "app"), 900)
        f = os.path.join(tmp, "coverage-summary.json")
        if not os.path.exists(f): return {"status": err or "ingen utdata", "hint": "cd app && npm ci"}
        d = json.load(open(f))
        return {"status": "ok", "lines_pct": d["total"]["lines"]["pct"], "functions_pct": d["total"]["functions"]["pct"], "branches_pct": d["total"]["branches"]["pct"]}
    finally: shutil.rmtree(tmp, ignore_errors=True)
def audit_cargo():
    o, err = run(["cargo", "audit", "--json"], R, 300)
    if o is None: return {"status": err, "hint": "cargo install cargo-audit"}
    try: d = json.loads(o)
    except ValueError: return {"status": "fel: ingen JSON"}
    vulns = d.get("vulnerabilities", {}); warn = d.get("warnings", {})
    return {"status": "ok", "vulnerabilities": vulns.get("count", 0), "ids": sorted({v["advisory"]["id"] for v in vulns.get("list", [])}),
            "warnings": {k: len(v) for k, v in warn.items()}}
def audit_npm():
    o, err = run(["npm", "audit", "--json"], os.path.join(R, "app"), 300)
    if o is None: return {"status": err}
    try: m = json.loads(o).get("metadata", {}).get("vulnerabilities")
    except ValueError: m = None
    if m is None: return {"status": "fel: ingen sammanfattning (offline?)"}
    return {"status": "ok", **{k: m.get(k, 0) for k in ("critical", "high", "moderate", "low")}}
def timed(cmd, cwd, timeout):
    import time; t = time.time(); o, err = run(cmd, cwd, timeout)
    return None if shutil.which(cmd[0]) is None else round(time.time() - t, 1)
def build_time():
    app = os.path.join(R, "app")
    return {"rust_test_build_s": timed(["cargo", "test", "-p", "hub-core", "--test", "it", "--no-run"], R, 3600),
            **({} if "--no-test-run" in FLAGS else {"rust_it_run_s": timed(["cargo", "test", "-p", "hub-core", "--test", "it"], R, 3600)}),
            "app_build_s": timed(["npx", "vite", "build"], app, 900)}
if "--coverage" in FLAGS: out["coverage"] = {"rust": cov_rust(), "app": cov_app()}
elif "--coverage-app" in FLAGS: out["coverage"] = {"app": cov_app()}
if "--build-time" in FLAGS: out["build_time"] = build_time()
if "--pr-labels" in FLAGS: out["pr_labels"] = pr_labels()
if "--audit" in FLAGS: out["security"] = {"cargo": audit_cargo(), "npm": audit_npm()}
# grades: (area, goal, value, ok) with value None = not measured
def n(v): return v if isinstance(v, (int, float)) else None
def le(v, t): return None if v is None else v <= t
def ge(v, t): return None if v is None else v >= t
def defs(name): return sum(len(re.findall(r"^\s*(?:pub(?:\([a-z]+\))?\s+)?fn\s+" + name + r"\b", rd(p), re.M)) for p in rs)
ra = out["rust_areas"].values(); ua = out["ui_areas"].values()
rsum = lambda k: sum(a.get(k, 0) for a in ra); usum = lambda k: sum(a.get(k, 0) for a in ua)
rcov = n(out.get("coverage", {}).get("rust", {}).get("lines_pct")); acov = n(out.get("coverage", {}).get("app", {}).get("lines_pct"))
cg = out.get("security", {}).get("cargo", {}); ng = out.get("security", {}).get("npm", {})
cg_v = cg.get("vulnerabilities") if cg.get("status") == "ok" else None
ng_v = ng["critical"] + ng["high"] if ng.get("status") == "ok" else None
it_s = out.get("build_time", {}).get("rust_it_run_s")
def debt(*rules):
    """What the rule ratchet still holds for `rules` (frozen/rules.txt, the list `rules::` keeps): the same count the rules use,
    so a cleared habit reads 0 here too. The per-area "unwrap"/"expect"/"let_underscore" above are rougher greps (they take test
    modules in the middle of a file as production, and an `.expect("why")` the rules allow), kept for the trend only."""
    try: rows = [l.split() for l in open(os.path.join(R, "crates/hub-core/tests/it/frozen/rules.txt"), encoding="utf-8")]
    except OSError: return None
    return sum(int(r[-1]) for r in rows if len(r) >= 3 and r[0] in rules)
GOALS = [
    ("Rust-kärnan", "unwrap utan skäl ≤ 20", debt("unwrap", "lock_unwrap"), 20, le),
    ("Rust-kärnan", "let _ = 0", debt("let_discard"), 0, le),
    ("Rust-kärnan", "täckning ≥ 75 %", rcov, 75, ge),
    ("Rusttester", "integrationstester ≤ 90 s", it_s, 90, le),
    ("Appen", "vitest finns (tester > 0)", usum("tests"), 1, ge),
    ("Appen", "täckning ≥ 60 %", acov, 60, ge),
    ("Appen", "svelte-ignore/ts-ignore = 0", usum("ignores"), 0, le),
    ("Appen", "tysta .catch = 0", usum("catch_quiet"), 0, le),
    ("Appen", "filer utan huvud = 0", usum("no_header"), 0, le),
    ("Dubbletter", "clip ≤ 1 definition", defs("clip"), 1, le),
    ("Dubbletter", "short ≤ 1 definition", defs("short"), 1, le),
    ("Dubbletter", "ago ≤ 1 definition", defs("ago"), 1, le),
    ("Dubbletter", "slug ≤ 1 definition", defs("slug"), 1, le),
    ("Docs", "ARCHITECTURE.md och delarna under 800 rader (längsta)", max((v for k, v in out["docs_lines"].items() if k.startswith("ARCHITECTURE") or k.startswith("architecture/")), default=None), 800, le),
    ("Säkerhet", "cargo audit: 0 sårbarheter", cg_v, 0, le),
    ("Säkerhet", "npm audit: 0 höga/kritiska", ng_v, 0, le),
]
def letter(met, measured):
    if not measured: return "?"
    f = met / measured
    return "A" if met == measured else "B" if f >= .75 else "C" if f >= .5 else "D" if f >= .25 else "E"
grades = {}
for area_name, goal, val, tgt, cmp in GOALS:
    ok = cmp(val, tgt); g = grades.setdefault(area_name, {"met": 0, "measured": 0, "unmeasured": 0, "goals": []})
    g["goals"].append({"goal": goal, "value": val, "ok": ok})
    if ok is None: g["unmeasured"] += 1
    else: g["measured"] += 1; g["met"] += int(ok)
for g in grades.values(): g["grade"] = letter(g["met"], g["measured"])
out["grades"] = grades
if "--table" in FLAGS:
    print("| Område | Betyg | Uppfyllda | Ej uppfyllda mål (nu) | Ej mätt |\n|---|---|---|---|---|")
    for a, g in grades.items():
        miss = "; ".join(f"{x['goal']} (nu {x['value']})" for x in g["goals"] if x["ok"] is False) or "–"
        nm = ", ".join(x["goal"] for x in g["goals"] if x["ok"] is None) or "–"
        print(f"| {a} | {g['grade']} | {g['met']}/{g['measured']} | {miss} | {nm} |")
    st = out["structure"]
    if st.get("status") == "ok":
        print(f"\nStruktur: cykel {st['cycle_modules']}/{st['modules']} moduler ({st['cycle_loc']} rader), rankade {st['ranked_modules']}, Arc<Hub> {st['arc_hub']}, otypade händelser {st['event_kinds']}, tokio::spawn {st['tokio_spawn']}, HTTP-klienter {st['own_http_clients']}, komponenter med api. {st['app_components_calling_api']}")
    pl = out.get("pr_labels", {})
    if pl.get("status") == "ok":
        print(f"PR-etiketter 7 d: {pl['merged_7d']} mergade, bug per område {pl['bug_by_area']}")
else: json.dump(out, sys.stdout, indent=1, default=list)
