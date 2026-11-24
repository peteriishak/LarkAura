"""Drives the spike through its test-only D-Bus API and records what really happened."""
import ast, json, os, subprocess, time

DIR, OUT = os.environ["SPIKE_DIR"], os.environ["SPIKE_OUT"]
BUS = ["gdbus", "call", "--session", "--dest", "dev.larklabs.AuraSpike",
       "--object-path", "/dev/larklabs/AuraSpike", "--method"]
LOG = os.path.join(OUT, "target-events.jsonl")

def call(method, *args):
    r = subprocess.run(BUS + [f"dev.larklabs.AuraSpike.{method}", *map(str, args)],
                       capture_output=True, text=True, timeout=15)
    if r.returncode:
        raise RuntimeError(f"{method}: {r.stderr.strip()}")
    return r.stdout.strip()

def state():
    return json.loads(ast.literal_eval(call("State"))[0])

def wait(check, what, timeout=25):
    end = time.time() + timeout
    last = None
    while time.time() < end:
        try:
            if check():
                return
        except Exception as e:
            last = e
        time.sleep(0.2)
    raise TimeoutError(f"{what} (last error: {last})")

def events(title):
    if not os.path.exists(LOG):
        return []
    with open(LOG) as f:
        return [e for e in map(json.loads, (l for l in f if l.startswith("{"))) if e["title"] == title]

def win(title):
    return next((w for w in state()["windows"] if w["title"] == title), None)

def overlay():
    return next((w for w in state()["windows"] if w["overlay"]), None)

def launch_target(title):
    f = open(LOG, "a")
    p = subprocess.Popen(["gjs", "-m", f"{DIR}/target.js", title], stdout=f, stderr=subprocess.DEVNULL)
    wait(lambda: win(title), f"{title} window")
    time.sleep(0.5)
    return p

def pointer(a, x=0.0, y=0.0):
    call("Mark", f"pointer {a} {x:.0f},{y:.0f}")
    call("Pointer", a, float(x), float(y))
    time.sleep(0.08)

def exercise(title):
    call("Mark", "exercise start")
    r = win(title)["rect"]
    cx, cy = r[0] + r[2] / 2, r[1] + r[3] / 2
    before = len(events(title))
    pointer("move", cx - 30, cy - 30); pointer("move", cx, cy)
    pointer("press"); pointer("release")
    time.sleep(0.45)                        # keep the drag from joining the click
    pointer("press")
    for i in range(1, 6):
        pointer("move", cx + i * 12, cy + i * 6)
    pointer("release")
    call("Scroll"); time.sleep(0.15)
    call("Key"); time.sleep(0.4)
    got = events(title)[before:]
    call("Mark", f"exercise end, {sum(e['e'] == 'press' for e in got)} presses")
    n = lambda k: sum(e["e"] == k for e in got)
    return {"press": n("press"), "release": n("release"), "motion": n("motion"),
            "drag": n("drag"), "scroll": n("scroll"), "key": n("key")}

def passed(r):
    return all(r[k] >= 1 for k in ("press", "release", "motion", "drag", "scroll", "key"))

def blocked(r):
    return r["press"] == 0 and r["release"] == 0 and r["drag"] == 0 and r["scroll"] == 0

def top_is_overlay():
    w = state()["windows"]
    return bool(w) and w[-1]["overlay"]

def start_overlay(*flags):
    call("LaunchOverlay", str(["gjs", "-m", f"{DIR}/overlay.js", *flags]))
    wait(lambda: overlay() and overlay()["rect"][2] == 1280, "overlay at work area size")
    time.sleep(1.0)

def cpu_ticks(pid):
    with open(f"/proc/{pid}/stat") as f:
        s = f.read().rsplit(")", 1)[1].split()
    return int(s[11]) + int(s[12])

def cpu_percent(pids, seconds=6):
    a = {k: cpu_ticks(p) for k, p in pids.items()}
    time.sleep(seconds)
    hz = os.sysconf("SC_CLK_TCK")
    return {k: round((cpu_ticks(p) - a[k]) * 100 / hz / seconds, 1) for k, p in pids.items()}

res = {}
targets = []
try:
    wait(lambda: not state()["startingUp"], "shell startup", 45)
    time.sleep(1.5)
    wait(lambda: state()["overview"] is False, "overview hidden", 20)
    time.sleep(1.0)
    targets.append(launch_target("Target A"))
    call("Place", "Target A", 200, 150, 700, 420); time.sleep(0.5)
    call("Activate", "Target A"); time.sleep(0.3)
    kind = os.environ.get("SPIKE_TYPE")
    if kind:
        call("SetOverlayType", kind)
        rows = []
        for n in range(int(os.environ.get("SPIKE_CYCLES", "10"))):
            call("Activate", "Target A"); time.sleep(0.3)
            start_overlay()
            s = state()
            ex = exercise("Target A")
            s2 = state()
            rows.append({"pass": passed(ex), "ex": ex, "focus": s2["focus"], "top": top_is_overlay(),
                         "overlayFocus": s2["overlayFocusCount"], "type": s["overlayType"],
                         "inTabs": "LarkAura Overlay Spike" in s["tabList"], "rect": overlay()["rect"]})
            call("StopOverlay"); wait(lambda: overlay() is None, "overlay gone"); time.sleep(0.4)
        res["type"] = kind
        res["cycles"] = rows
        res["summary"] = {"input_ok": sum(r["pass"] for r in rows), "cycles": len(rows),
                          "focus_ok": sum(r["focus"] == "Target A" for r in rows),
                          "on_top": sum(r["top"] for r in rows),
                          "overlay_focus_events": sum(r["overlayFocus"] for r in rows),
                          "hidden_from_tabs": sum(not r["inTabs"] for r in rows),
                          "work_area": sum(r["rect"] == [0, 32, 1280, 688] for r in rows)}
        raise SystemExit
    call("SetOverlayType", os.environ.get("SPIKE_FULL_TYPE", "DOCK"))
    res["baseline_no_overlay"] = exercise("Target A")
    shell_pid = state()["shellPid"]
    res["cpu_no_overlay"] = cpu_percent({"shell": shell_pid})

    start_overlay("--capture-input")
    res["control_overlay_captures_input"] = exercise("Target A")
    call("StopOverlay"); wait(lambda: overlay() is None, "control overlay gone"); time.sleep(0.5)
    call("Activate", "Target A"); time.sleep(0.3)

    start_overlay()
    s = state(); o = overlay()
    res["overlay_window"] = o
    res["focus_after_overlay"] = s["focus"]
    res["overlay_in_tab_list"] = o["title"] in s["tabList"]
    res["overlay_on_top"] = top_is_overlay()
    res["passthrough"] = exercise("Target A")
    res["repeat_passes"] = [exercise("Target A") for _ in range(4)]
    r = win("Target A")["rect"]
    before = sum(e["e"] == "press" for e in events("Target A"))
    for i in range(20):
        pointer("move", r[0] + 40 + i * 31, r[1] + 60 + (i * 47) % (r[3] - 120))
        pointer("press"); pointer("release"); time.sleep(0.32)
    time.sleep(0.3)
    res["clicks_delivered_of_20"] = sum(e["e"] == "press" for e in events("Target A")) - before
    res["work_area"] = state()["workArea"]
    call("Screenshot", f"{OUT}/overlay-over-window.png")
    pid = int(state()["overlayPid"])
    res["cpu_with_overlay"] = cpu_percent({"shell": shell_pid, "overlay": pid})

    targets.append(launch_target("Target B"))
    call("Place", "Target B", 80, 80, 500, 300); time.sleep(0.5)
    res["focus_after_new_window"] = state()["focus"]
    res["on_top_after_new_window"] = top_is_overlay()
    call("Activate", "Target A"); time.sleep(0.4)
    res["on_top_after_activate"] = top_is_overlay()
    res["focus_after_activate"] = state()["focus"]

    call("Fullscreen", "Target A", "true"); time.sleep(1.0)
    res["target_fullscreen"] = win("Target A")["fullscreen"]
    res["on_top_over_fullscreen"] = top_is_overlay()
    res["passthrough_fullscreen"] = exercise("Target A")
    call("Screenshot", f"{OUT}/overlay-over-fullscreen.png")
    call("Fullscreen", "Target A", "false"); time.sleep(0.8)

    call("SwitchWorkspace", 1); time.sleep(1.0)
    s = state(); o = overlay()
    res["workspace_after_switch"] = s["workspace"]
    res["overlay_on_all_workspaces"] = o["onAll"]
    res["overlay_visible_other_workspace"] = o["visible"]
    call("SwitchWorkspace", 0); time.sleep(0.8)
    res["overlay_alive"] = overlay() is not None

    res["verdict"] = {
        "harness_detects_input": passed(res["baseline_no_overlay"]),
        "control_blocks_input": blocked(res["control_overlay_captures_input"]),
        "input_passes_through": passed(res["passthrough"]) and all(map(passed, res["repeat_passes"])),
        "input_passes_through_fullscreen": passed(res["passthrough_fullscreen"]),
        "focus_not_stolen": res["focus_after_overlay"] == "Target A",
        "stays_on_top": all(res[k] for k in ("overlay_on_top", "on_top_after_new_window", "on_top_after_activate", "on_top_over_fullscreen")),
        "hidden_from_window_list": not res["overlay_in_tab_list"],
        "all_20_clicks_delivered": res["clicks_delivered_of_20"] == 20,
        "covers_work_area": res["overlay_window"]["rect"] == res["work_area"],
        "follows_workspaces": res["overlay_on_all_workspaces"] and res["overlay_visible_other_workspace"],
    }
except SystemExit:
    pass
except Exception as e:
    res["error"] = repr(e)
    try:
        res["state_at_error"] = state()
    except Exception as e2:
        res["state_error"] = repr(e2)
finally:
    for p in targets:
        p.terminate()
    with open(f"{OUT}/results.json", "w") as f:
        json.dump(res, f, indent=2)
    print(json.dumps(res, indent=2))
raise SystemExit(1 if "error" in res else 0)
