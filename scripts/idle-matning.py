#!/usr/bin/env python3
"""Hub idle measurement (report only, never blocks). Usage: python3 -I scripts/idle-matning.py (<hubd-binary> | --pid N) [--seconds 60] [--warmup 10] > idle.json
Starts hubd on an empty data folder (XDG_DATA_HOME/HOME/LOCALAPPDATA in a temp `hub-idle-*` folder, removed afterwards) or attaches to a running one with --pid, waits for --warmup, then samples /proc for --seconds and prints one JSON: cpu_pct (of one core), rss_mb, threads, wakeups_per_s (voluntary context switches, summed over threads), seconds. Linux only (tincan, cloud); elsewhere gives {"status": ...}. The started hubd gets a free port of its own (`bind` in its temp hub.json), so it runs beside the machine's own Hub; --pid measures that one instead. Weekly routine: run it next to code-health.py and keep the JSON in halsorutin/idle-<date>.json; compare like with like (same machine, same binary kind)."""
import os, sys, json, time, shutil, socket, tempfile, subprocess, datetime
A = sys.argv[1:]
def opt(name, default):
    return float(A[A.index(name) + 1]) if name in A else default
SECS, WARM = opt("--seconds", 60), opt("--warmup", 10)
def done(d):
    print(json.dumps(d, indent=1)); sys.exit(0)
if not os.path.isdir("/proc"): done({"status": "needs /proc (Linux)"})
tmp = proc = None
try:
    if "--pid" in A: pid = int(A[A.index("--pid") + 1])
    else:
        exe = [a for a in A if not a.startswith("--") and not a.replace(".", "").isdigit()]
        if not exe or not os.path.exists(exe[0]): done({"status": "no hubd binary given or found"})
        tmp = tempfile.mkdtemp(prefix="hub-idle-")
        with socket.socket() as k: k.bind(("127.0.0.1", 0)); port = k.getsockname()[1]   # a free port: tincan's own Hub holds 7878
        with open(os.path.join(tmp, "hub.json"), "w", encoding="utf-8") as f: json.dump({"bind": f"127.0.0.1:{port}"}, f)
        env = dict(os.environ, HOME=tmp, XDG_DATA_HOME=tmp, LOCALAPPDATA=tmp, HUB_CONFIG=os.path.join(tmp, "hub.json"))
        err = open(os.path.join(tmp, "stderr.log"), "w+b")   # a file, not a pipe: nobody drains a pipe during the measurement
        proc = subprocess.Popen([exe[0]], env=env, stdout=subprocess.DEVNULL, stderr=err)
        pid = proc.pid
    time.sleep(WARM)
    if proc and proc.poll() is not None: done({"status": "hubd exited during warmup", "code": proc.returncode, "stderr": (err.seek(0), err.read())[1][-300:].decode("utf-8", "replace")})
    tck = os.sysconf("SC_CLK_TCK")
    def cpu():
        f = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
        return (int(f[11]) + int(f[12])) / tck
    def ctx():   # per thread: one that ended between the samples would otherwise count as negative wakeups
        n = {}
        for t in os.listdir(f"/proc/{pid}/task"):
            try:
                for l in open(f"/proc/{pid}/task/{t}/status"):
                    if l.startswith("voluntary_ctxt_switches"): n[t] = int(l.split()[1])
            except OSError: pass   # the thread ended while it was read
        return n
    c0, w0, t0 = cpu(), ctx(), time.time()
    time.sleep(SECS)
    c1, w1, t1 = cpu(), ctx(), time.time()
    st = open(f"/proc/{pid}/status").read()
    g = lambda k: int(next(l for l in st.splitlines() if l.startswith(k)).split()[1])
    done({"date": datetime.date.today().isoformat(), "seconds": round(t1 - t0, 1),
          "cpu_pct": round((c1 - c0) / (t1 - t0) * 100, 2), "rss_mb": round(g("VmRSS") / 1024, 1),
          "threads": g("Threads"), "wakeups_per_s": round(sum(n - w0.get(t, 0) for t, n in w1.items()) / (t1 - t0), 1)})
except (OSError, StopIteration) as e:
    done({"status": f"measurement failed: {e}"})
finally:
    if proc and proc.poll() is None: proc.terminate()
    if tmp: shutil.rmtree(tmp, ignore_errors=True)
