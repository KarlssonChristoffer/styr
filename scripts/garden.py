#!/usr/bin/env python3
"""Hub garden: what looks like weeds (report only, never blocks). Usage: python3 -I scripts/garden.py <repo> [--write] [--json]
Finds: pub Rust items (fn/struct/enum/const/trait) whose name occurs once in the whole workspace (nobody uses them), unused crate dependencies (cargo-machete; the report says \"not run\" when it is not installed, never a false 0), `hubdHas(...)` calls with no `compat-until` marker, TS/Svelte exports and components nothing imports, `compat-until: YYYY-MM-DD` markers that have passed (remove the fallback they mark), Rust tests with no assertion. Prints markdown; --write puts it in docs/GARDEN.md; --json prints the counts (the PR check reads `dead`). Name-count is a heuristic: a hit is a candidate, check it before deleting; a false positive goes in docs/garden-keep.txt (one name per line, why after #). Generated bindings are skipped. Never use it to delete without reading."""
import os, re, sys, json, shutil, datetime, subprocess, collections
ARGS = [a for a in sys.argv[1:] if not a.startswith("--")]
FLAGS = {a for a in sys.argv[1:] if a.startswith("--")}
R = os.path.abspath(ARGS[0] if ARGS else ".")
SKIP = {"node_modules", "target", ".git", "bindings", "dist"}
def walk(roots, exts):
    for r in roots:
        for d, ds, fs in os.walk(os.path.join(R, r)):
            ds[:] = [x for x in ds if x not in SKIP]
            for f in fs:
                if f.endswith(exts): yield os.path.join(d, f)
def rd(p):
    try: return open(p, encoding="utf-8", errors="ignore").read()
    except OSError: return ""
def rel(p): return os.path.relpath(p, R)
def words(srcs):
    c = collections.Counter()
    for s in srcs: c.update(re.findall(r"\b[A-Za-z_]\w+\b", s))
    return c
keep = {l.split("#")[0].strip() for l in rd(os.path.join(R, "docs/garden-keep.txt")).splitlines()} - {""}
rs = list(walk(["crates", "app/src-tauri/src"], (".rs",)))
rs_src = {p: rd(p) for p in rs}
cr = words(rs_src.values())
dead_rs = []
for p, s in rs_src.items():
    if "/tests/" in p: continue
    for i, l in enumerate(s.splitlines()):
        m = re.match(r"\s*pub(?:\([a-z]+\))?\s+(?:async\s+)?(?:unsafe\s+)?(fn|struct|enum|const|trait)\s+(\w+)", l)
        if m and cr[m.group(2)] == 1 and m.group(2) not in keep: dead_rs.append(f"{rel(p)}:{i+1} {m.group(1)} {m.group(2)}")
ts = list(walk(["app/src"], (".ts", ".svelte")))
ts_src = {p: rd(p) for p in ts}
ct = words(ts_src.values())
dead_ts = []
for p, s in ts_src.items():
    if p.endswith(".test.ts"): continue
    for m in re.finditer(r"^export\s+(?:async\s+)?(?:function|const|let|class|type|interface)\s+(\w+)", s, re.M):
        if ct[m.group(1)] == 1 and m.group(1) not in keep: dead_ts.append(f"{rel(p)} {m.group(1)}")
allts = " ".join(ts_src.values())
for p in ts:
    if p.endswith(".svelte"):
        n = os.path.basename(p)[:-7]
        if n not in keep and not re.search(r"[/'\"]" + re.escape(n) + r"\.svelte", allts): dead_ts.append(f"{rel(p)} (component nobody imports)")
deps = [] if shutil.which("cargo-machete") else None  # None = not run: the report says so instead of a false 0
if deps is not None:
    try:
        o = subprocess.run(["cargo", "machete"], cwd=R, capture_output=True, text=True, timeout=300).stdout
        crate = None
        for l in o.splitlines():
            if l.endswith(":") and "--" in l: crate = l.split(" --")[0].strip()
            elif l.startswith("\t") and crate: deps.append(f"{crate} {l.strip()}")
    except Exception as e: deps.append(f"(cargo machete failed: {e})")
unmarked = []
for p, s in ts_src.items():
    if p.endswith(".test.ts") or p.endswith("lib/compat.ts"): continue
    ls = s.splitlines()
    for i, l in enumerate(ls):
        if "hubdHas(" in l and not l.lstrip().startswith("import ") and not any("compat-until" in x for x in ls[max(0, i - 3):i + 1]): unmarked.append(f"{rel(p)}:{i+1} {l.strip()[:90]}")
today = datetime.date.today().isoformat()
expired = []
for p, s in list(rs_src.items()) + list(ts_src.items()):
    for m in re.finditer(r"compat-until:\s*(\d{4}-\d{2}-\d{2})", s):
        if m.group(1) < today and "garden.py" not in p: expired.append(f"{rel(p)} compat-until {m.group(1)}")
noassert = []
# A brace inside a string, char or comment is not the end of the test: blank those out (same length) before counting braces.
LITERAL = re.compile(r'r(#*)".*?"\1|"(?:\\.|[^"\\])*"|\'(?:\\.|[^\'\\])\'|//[^\n]*|/\*.*?\*/', re.S)
for p, s in rs_src.items():
    code = LITERAL.sub(lambda x: " " * len(x.group(0)), s)
    for m in re.finditer(r"#\[(?:tokio::)?test[^\]]*\]\s*(?:#\[[^\]]*\]\s*)*(?:async\s+)?fn\s+(\w+)[^{]*\{", code):
        i = m.end(); d = 1; j = i
        while j < len(code) and d: d += (code[j] == "{") - (code[j] == "}"); j += 1
        if not re.search(r"assert|panic!|unwrap\(|expect\(|\?;|should_panic|matches!", code[m.start():m.start(1)] + s[i:j]) and m.group(1) not in keep:   # the attributes (#[should_panic]) and the body, not the test's own name
            noassert.append(f"{rel(p)}:{s[:m.start()].count(chr(10))+1} {m.group(1)}")
counts = {"dead": len(dead_rs) + len(dead_ts), "dead_rust": len(dead_rs), "dead_app": len(dead_ts), "unused_deps": None if deps is None else len(deps), "unmarked_compat": len(unmarked), "expired_compat": len(expired), "tests_without_assert": len(noassert)}
if "--json" in FLAGS: print(json.dumps(counts)); sys.exit(0)
def sec(title, rows, hint):
    if rows is None: return f"## {title} (not run)\n{hint}\n- not run: the tool is not installed on this machine, so this says nothing about it\n\n"
    return f"## {title} ({len(rows)})\n{hint}\n" + ("".join(f"- `{r}`\n" for r in rows) or "- none\n") + "\n"
md = ("<!-- för: agent -->\n# Garden\n\nGenerated by `python3 -I scripts/garden.py . --write` (the Monday routine on tincan). Candidates, not verdicts: "
      "name-count is a heuristic. If a file you touch is listed, delete what is listed or say why it stays (add it to `docs/garden-keep.txt` with a reason). "
      "When you find weeds, ask why an agent added them and fix the rule or doc that was missing, not only the code.\n\n"
      + sec("Rust items nothing uses", dead_rs, "A `pub` name that occurs once in the whole workspace.")
      + sec("App exports and components nothing imports", dead_ts, "")
      + sec("Unused crate dependencies", deps, "From `cargo machete`.")
      + sec("Expired `compat-until`", expired, "The date has passed: remove the fallback it marks.")
      + sec("App fallbacks without `compat-until`", unmarked, "A `hubdHas(...)` call (a fallback for an older hubd) with no `compat-until: YYYY-MM-DD` on its line or the three above it. Mark it with the feature's date plus 30 days so it is removed on time.")
      + sec("Rust tests without an assertion", noassert, "May be deliberate (a test that only must not panic); read before deleting."))
if "--write" in FLAGS: open(os.path.join(R, "docs/GARDEN.md"), "w").write(md)
else: print(md)
