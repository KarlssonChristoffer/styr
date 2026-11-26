#!/usr/bin/env python3
"""Hub mutation tests, weekly, one source file at a time (report only; nothing is deleted from the list it prints). Usage:
python3 -I scripts/mutanter.py <repo> --out-dir <halsorutin folder> [--file crates/hub-core/src/x.rs] [--timeout 120]
Runs `cargo mutants` (install once: `cargo install cargo-mutants`) on ONE file of hub-core's src: --file, or else the next one in turn (files sorted by path, 150-800 lines, one per ISO week, so the whole core is covered over many weeks). A mutant is a small change of the code (a `+` turned into `-`, a body replaced by a default); a SURVIVOR is a mutant no test failed on: the test does not guard that line. Single job (`-j1`), because tincan has little memory. Writes <out-dir>/mutanter-<date>.md: the file, caught/missed/unviable counts and every survivor with its line. The list goes to Christoffer as it is; no test or code is removed or added on it without his say. Missing cargo-mutants gives a note, never a failure."""
import os, re, sys, glob, json, shutil, datetime, tempfile, subprocess
A = sys.argv[1:]
def opt(n, d=None): return A[A.index(n) + 1] if n in A else d
R = os.path.abspath(A[0]); OUT = opt("--out-dir")
if not OUT: sys.exit("--out-dir <folder> needed")
def out(text):
    os.makedirs(OUT, exist_ok=True)
    open(os.path.join(OUT, f"mutanter-{datetime.date.today().isoformat()}.md"), "w", encoding="utf-8").write(text)
    print(text)
if subprocess.run(["cargo", "mutants", "--version"], capture_output=True).returncode != 0:
    out("cargo-mutants is not installed here: `cargo install cargo-mutants`, then run again.\n"); sys.exit(0)
def pick():
    if opt("--file"): return opt("--file")
    src = os.path.join(R, "crates/hub-core/src"); fs = []
    for d, _, names in os.walk(src):
        for n in names:
            if n.endswith(".rs"):
                p = os.path.join(d, n); lines = open(p, encoding="utf-8", errors="ignore").read().count("\n")
                if 150 <= lines <= 800: fs.append(os.path.relpath(p, R).replace("\\", "/"))
    fs.sort()
    return fs[datetime.date.today().isocalendar()[1] % len(fs)] if fs else None
f = pick()
if not f: out("no file of 150-800 lines found in crates/hub-core/src\n"); sys.exit(0)
tmp = tempfile.mkdtemp(prefix="hub-mutants-")
try:
    p = subprocess.run(["cargo", "mutants", "-p", "hub-core", "--file", f, "-j1", "--timeout", opt("--timeout", "120"), "--output", tmp,
                        "--", "--", "--skip", "update::checks::"],  # that test group needs the machine (cargo on PATH, memory) and fails in the unmutated tree on tincan; mutants need a green baseline
                       cwd=R, capture_output=True, text=True, timeout=3 * 3600)
    base = os.path.join(tmp, "mutants.out")
    def lines(name):
        try: return [l.strip() for l in open(os.path.join(base, name), encoding="utf-8") if l.strip()]
        except OSError: return []
    missed, caught, unviable = lines("missed.txt"), lines("caught.txt"), lines("unviable.txt")
    if not (missed or caught or unviable):
        # No mutant ran, mostly because the tests fail before any change: name them, so the report says what to fix.
        failed = []
        for log in glob.glob(os.path.join(base, "log", "baseline*")):
            failed += re.findall(r"^test (\S+) \.\.\. FAILED", open(log, encoding="utf-8", errors="ignore").read(), re.M)
        names = "".join(f"- `{t}`\n" for t in sorted(set(failed))) or "- none named in the baseline log\n"
        out(f"# Mutation test: {f}\n\nNo result ({(p.stderr or p.stdout)[-400:].strip()})\n\n## Tests that fail in the unchanged tree\n{names}"); sys.exit(0)
    body = "\n".join(f"- `{m}`" for m in missed) or "- none: every mutant was caught"
    out(f"# Mutation test: {f}\n\ncaught {len(caught)}, missed {len(missed)}, unviable {len(unviable)}.\n\n## Survivors (no test fails on these)\n{body}\n")
except subprocess.TimeoutExpired:
    out(f"# Mutation test: {f}\n\nStopped after 3 hours; run it with --file on a smaller file.\n")
finally:
    shutil.rmtree(tmp, ignore_errors=True)
