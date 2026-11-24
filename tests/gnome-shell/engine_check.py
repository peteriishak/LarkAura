"""Strict input checks and measurements for overlays in the isolated GNOME session.

SPIKE_SCENARIO selects the run:
  after-control   start an input-capturing overlay, stop it, then test OVERLAY_KIND
                  (OVERLAY_KIND=none tests the window below with nothing started)
  after-window    click an ordinary window above the target, close it, then test
  repeat          start and stop OVERLAY_KIND repeatedly with no capturing overlay
  auras           run each aura in the real engine, with screenshots and measurements
  launch-soak     launch the engine repeatedly and check that it keeps drawing
  ghost           kill the managed renderer repeatedly and time its window's removal
  panel           use the top bar menu with real pointer input
  perf            measure every aura and a full scene through the real extension
  storm           capture a lightning flash
  showcase        through the real extension, show every aura and option, with screenshots
  displays        turn each display's effects off and on from the top bar menu
  snowball        through the real extension, hold the button on the snow to pack a
                  snowball, drag it and let go, with screenshots
  product         test the real extension: placement, input, Stillness, settings,
                  pause, full screen, crash recovery, disable and multiple displays
"""
import ast, glob, json, os, re, signal, struct, subprocess, time, zlib

DIR, OUT = os.environ["SPIKE_DIR"], os.environ["SPIKE_OUT"]
ENGINE = os.environ.get("SPIKE_ENGINE", "")
KIND = os.environ.get("OVERLAY_KIND", "gjs")
SCENARIO = os.environ.get("SPIKE_SCENARIO", "repeat")
CYCLES = int(os.environ.get("SPIKE_CYCLES", "8"))
SETTLE = float(os.environ.get("SPIKE_SETTLE", "0"))
AURAS = ["first-light", "quiet-snow", "deep-snow", "soft-rain", "night-storm", "autumn-leaves",
         "blossom-drift", "falling-stars", "fireflies", "low-fog"]
BUS = ["gdbus", "call", "--session", "--dest", "dev.larklabs.AuraSpike",
       "--object-path", "/dev/larklabs/AuraSpike", "--method"]
LOG = os.path.join(OUT, "target-events.jsonl")
TARGET = "Target A"


def call(method, *args):
    r = subprocess.run(BUS + [f"dev.larklabs.AuraSpike.{method}", *map(str, args)],
                       capture_output=True, text=True, timeout=15)
    if r.returncode:
        raise RuntimeError(f"{method}: {r.stderr.strip()}")
    return r.stdout.strip()


def state():
    return json.loads(ast.literal_eval(call("State"))[0])


def wait(check, what, timeout=25):
    end, last = time.time() + timeout, None
    while time.time() < end:
        try:
            if check():
                return
        except Exception as e:
            last = e
        time.sleep(0.1)
    raise TimeoutError(f"{what} (last error: {last})")


def events():
    if not os.path.exists(LOG):
        return []
    with open(LOG) as f:
        rows = (json.loads(l) for l in f if l.startswith("{"))
        return [e for e in rows if e["title"] == TARGET]


def window(title):
    return next((w for w in state()["windows"] if w["title"] == title), None)


def overlay():
    return next((w for w in state()["windows"] if w["overlay"]), None)


def pointer(action, x=0.0, y=0.0):
    call("Pointer", action, float(x), float(y))
    time.sleep(0.08)


def exercise():
    r = window(TARGET)["rect"]
    cx, cy = r[0] + r[2] / 2, r[1] + r[3] / 2
    before = len(events())
    pointer("move", cx - 30, cy - 30)
    pointer("move", cx, cy)
    pointer("press")
    pointer("release")
    time.sleep(0.45)
    pointer("press")
    for i in range(1, 6):
        pointer("move", cx + i * 12, cy + i * 6)
    pointer("release")
    call("Scroll")
    time.sleep(0.15)
    call("Key")
    time.sleep(0.4)
    got = events()[before:]
    return {k: sum(e["e"] == k for e in got)
            for k in ("press", "release", "motion", "drag", "scroll", "key")}


def complete(r):
    """Every action must arrive. One click out of two is a failure."""
    return r["press"] == 2 and r["release"] == 2 and r["drag"] >= 1 and r["motion"] >= 5 \
        and r["scroll"] >= 1 and r["key"] >= 1


def argv(kind, aura="first-light", *flags):
    if kind == "engine":
        return [ENGINE, "--mode", "gnome", "--aura", aura, "--stats", *flags]
    return ["gjs", "-m", f"{DIR}/overlay.js", *flags]


def start(command):
    call("LaunchOverlay", str(command))
    work = state()["workArea"]
    wait(lambda: overlay() and overlay()["rect"][2] == work[2], "overlay at work area size")


def stop():
    call("StopOverlay")
    wait(lambda: overlay() is None, "overlay gone")


def cpu_ns(pid):
    """Scheduler run time of every thread, far finer than clock ticks."""
    total = 0
    for path in glob.glob(f"/proc/{pid}/task/*/schedstat"):
        try:
            with open(path) as f:
                total += int(f.read().split()[0])
        except (OSError, ValueError, IndexError):
            pass
    return total


def thread_ns(pid):
    """Run time of each named thread, to show where a process spends its CPU."""
    out = {}
    for path in glob.glob(f"/proc/{pid}/task/*"):
        try:
            with open(f"{path}/comm") as f:
                name = f.read().strip()
            with open(f"{path}/schedstat") as f:
                out[name] = out.get(name, 0) + int(f.read().split()[0])
        except (OSError, ValueError, IndexError):
            pass
    return out


def gpu_clock():
    """The GPU's current clock, because busy time rises when the clock is low."""
    for path in glob.glob("/sys/class/drm/card*/gt_act_freq_mhz"):
        try:
            with open(path) as f:
                return int(f.read())
        except (OSError, ValueError):
            pass
    return None


def gpu(pid):
    """Per-client GPU time and memory that the kernel reports through fdinfo."""
    clients = {}
    for path in glob.glob(f"/proc/{pid}/fdinfo/*"):
        try:
            with open(path) as f:
                fields = dict(l.rstrip("\n").split(":\t", 1) for l in f if ":\t" in l)
        except OSError:
            continue
        if "drm-client-id" in fields:
            clients[fields["drm-client-id"]] = fields
    ns, mem = 0, {}
    for fields in clients.values():
        for k, v in fields.items():
            if k.startswith("drm-engine-") and v.endswith(" ns"):
                ns += int(v.split()[0])
            elif k.startswith(("drm-total-", "drm-resident-")):
                n, *unit = v.split()
                mem[k] = mem.get(k, 0) + int(n) * {"KiB": 1024, "MiB": 1048576}.get(unit[0] if unit else "", 1)
    return ns, mem


def memory(pid):
    with open(f"/proc/{pid}/smaps_rollup") as f:
        rows = dict(l.split(":", 1) for l in f if ":" in l and not l.startswith("0"))
    return {k.lower(): round(int(rows[k].split()[0]) / 1024, 1) for k in ("Rss", "Pss")}


def measure(pids, engine_pid, seconds=8):
    cpu0 = {k: cpu_ns(p) for k, p in pids.items()}
    gpu0 = {k: gpu(p)[0] for k, p in pids.items()}
    threads0 = thread_ns(engine_pid)
    started = time.monotonic()
    clocks = []
    for _ in range(int(seconds)):
        time.sleep(1)
        clocks.append(gpu_clock())
    dt = time.monotonic() - started
    out = {}
    if all(c is not None for c in clocks) and clocks:
        out["gpu_mhz"] = round(sum(clocks) / len(clocks))
    threads1 = thread_ns(engine_pid)
    out["engine_threads"] = {k: round((v - threads0.get(k, 0)) / 1e9 / dt * 100, 2)
                             for k, v in sorted(threads1.items(), key=lambda kv: threads0.get(kv[0], 0) - kv[1])[:5]}
    for k, p in pids.items():
        out[f"cpu_{k}"] = round((cpu_ns(p) - cpu0[k]) / 1e9 / dt * 100, 2)
        out[f"gpu_busy_{k}"] = round((gpu(p)[0] - gpu0[k]) / 1e9 / dt * 100, 2)
    out["gpu_memory_mib"] = {k: round(v / 1048576, 1) for k, v in gpu(engine_pid)[1].items()
                             if k in ("drm-total-system0", "drm-resident-system0")}
    out["engine_memory_mib"] = memory(engine_pid)
    return out


def launch_soak():
    rows = []
    for i in range(CYCLES):
        start(argv("engine", AURAS[i % len(AURAS)]))
        time.sleep(1.5)
        pid = state()["overlayPid"]
        ns0, cpu0 = gpu(pid)[0], cpu_ns(pid)
        time.sleep(2.0)
        rows.append({"aura": AURAS[i % len(AURAS)], "gpu_ms": round((gpu(pid)[0] - ns0) / 1e6, 2),
                     "cpu_ms": round((cpu_ns(pid) - cpu0) / 1e6, 1)})
        stopped = time.monotonic()
        call("StopOverlay")
        wait(lambda: not os.path.exists(f"/proc/{pid}"), "renderer exit")
        rows[-1]["process_gone_s"] = round(time.monotonic() - stopped, 2)
        wait(lambda: overlay() is None, "overlay gone")
        rows[-1]["window_gone_s"] = round(time.monotonic() - stopped, 2)
        time.sleep(0.2)
    return rows


def test_after_disturbance():
    """Runs the exercise with the configured overlay, or with none at all."""
    if KIND == "none":
        time.sleep(1.3)  # About as long as an overlay takes to launch and settle.
        return exercise()
    start(argv(KIND))
    time.sleep(1.0)
    got = exercise()
    stop()
    return got


def cycle_after_control():
    rows = []
    for _ in range(CYCLES):
        call("Activate", TARGET)
        time.sleep(0.3)
        start(argv("gjs", "first-light", "--capture-input"))
        time.sleep(1.0)
        blocked = exercise()
        stop()
        time.sleep(SETTLE)
        got = test_after_disturbance()
        rows.append({"blocked_by_control": blocked, "passthrough": got, "complete": complete(got)})
        time.sleep(0.4)
    return rows


def cycle_after_window():
    rows = []
    title = "Closing Window"
    for _ in range(CYCLES):
        call("Activate", TARGET)
        time.sleep(0.3)
        other = subprocess.Popen(["gjs", "-m", f"{DIR}/target.js", title],
                                 stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        wait(lambda: window(title), "closing window")
        time.sleep(0.4)
        call("Place", title, 200, 150, 700, 420)
        time.sleep(0.3)
        call("Activate", title)
        time.sleep(0.3)
        r = window(title)["rect"]
        pointer("move", r[0] + r[2] / 2, r[1] + r[3] / 2)
        pointer("press")
        pointer("release")
        time.sleep(0.5)
        other.terminate()
        other.wait()
        wait(lambda: window(title) is None, "closing window gone")
        time.sleep(SETTLE)
        got = test_after_disturbance()
        rows.append({"passthrough": got, "complete": complete(got), "focus": state()["focus"]})
        time.sleep(0.4)
    return rows


def cycle_repeat():
    rows = []
    for _ in range(CYCLES):
        call("Activate", TARGET)
        time.sleep(0.3)
        start(argv(KIND))
        time.sleep(1.0)
        got = exercise()
        s = state()
        rows.append({"passthrough": got, "complete": complete(got), "focus": s["focus"]})
        stop()
        time.sleep(0.4)
    return rows


def run_auras():
    s = state()
    shell_pid = s["shellPid"]
    rows = {"no_overlay": measure({"shell": shell_pid}, shell_pid)}
    for aura in AURAS:
        start(argv("engine", aura))
        time.sleep(4.0)
        s = state()
        pid = s["overlayPid"]
        numbers = measure({"engine": pid, "shell": shell_pid}, pid)
        got = exercise()
        shot = os.path.join(OUT, f"aura-{aura}.png")
        call("Screenshot", shot)
        s = state()
        rows[aura] = {**numbers, "input": got, "input_complete": complete(got),
                      "overlay_rect": overlay()["rect"], "work_area": s["workArea"],
                      "focus": s["focus"], "overlay_on_top": s["windows"][-1]["overlay"],
                      "listed": "LarkAura Overlay" in s["tabList"]}
        stop()
        time.sleep(0.5)
    return rows


SHELL_LOG = os.path.join(OUT, "shell.log")
SETTINGS = os.path.join(OUT, "home", ".config", "larkaura", "settings.json")
STAT = re.compile(r"larkaura-engine: aura=(\S+) state=(\w+) fps=([\d.]+) frame_ms=([\d.]+) "
                  r"instances=(\d+) presence=([\d.]+)")


def overlays():
    return [w for w in state()["windows"] if w["overlay"] and w["visible"]]


def log_mark():
    return os.path.getsize(SHELL_LOG)


def stats_since(mark):
    with open(SHELL_LOG, errors="ignore") as f:
        f.seek(mark)
        return [{"aura": m[1], "state": m[2], "fps": float(m[3]), "presence": float(m[6])}
                for m in STAT.finditer(f.read())]


def write_settings(**values):
    os.makedirs(os.path.dirname(SETTINGS), exist_ok=True)
    temporary = SETTINGS + ".tmp"
    with open(temporary, "w") as f:
        json.dump(values, f)
    os.replace(temporary, SETTINGS)


def process_state(pid):
    try:
        with open(f"/proc/{pid}/stat") as f:
            return f.read().rsplit(")", 1)[1].split()[0]
    except OSError:
        return "gone"


def usage(pid, seconds=4):
    cpu, busy = cpu_ns(pid), gpu(pid)[0]
    time.sleep(seconds)
    return {"cpu_percent": round((cpu_ns(pid) - cpu) / 1e9 / seconds * 100, 3),
            "gpu_percent": round((gpu(pid)[0] - busy) / 1e9 / seconds * 100, 3)}


def watch(seconds):
    mark = log_mark()
    time.sleep(seconds)
    return stats_since(mark)


def extension(action):
    subprocess.run(["gnome-extensions", action, "larkaura@larklabs.dev"], check=True, timeout=10)


def placed():
    s = state()
    found = sorted(w["rect"] for w in s["windows"] if w["overlay"] and w["visible"])
    return found == sorted(s["workAreas"])


def click_at(x, y):
    pointer("move", x, y)
    time.sleep(0.15)
    pointer("press")
    pointer("release")
    time.sleep(0.6)


def center(rect):
    return rect[0] + rect[2] / 2, rect[1] + rect[3] / 2


def saved():
    try:
        with open(SETTINGS) as f:
            return json.load(f)
    except OSError:
        return None


def open_menu():
    indicator = state()["indicator"]
    if not indicator["open"]:
        click_at(*center(indicator["rect"]))
    wait(lambda: state()["indicator"]["open"], "menu open", 5)
    return state()["indicator"]


def choose(label):
    item = next(i for i in open_menu()["items"] if i["label"] == label)
    click_at(*center(item["rect"]))
    time.sleep(0.8)


def panel():
    out = {}
    wait(placed, "overlays placed", 30)
    wait(lambda: state()["indicator"], "top bar icon", 10)
    time.sleep(1.5)
    menu = open_menu()
    out["items"] = [i["label"] for i in menu["items"]]
    call("Screenshot", os.path.join(OUT, "panel-menu.png"))
    click_at(*center(state()["indicator"]["rect"]))
    choose("Fireflies")
    out["chose_aura"] = saved()
    out["engine_aura"] = watch(2)[-1:]
    choose("Effects")
    out["after_off"] = saved()
    pid = overlays()[0]["pid"]
    time.sleep(1)
    out["off_usage"] = usage(pid, 2)
    if state()["indicator"]["open"]:
        click_at(*center(state()["indicator"]["rect"]))
    choose("Effects")
    out["after_on"] = saved()
    time.sleep(1)
    out["on_usage"] = usage(pid, 2)
    if state()["indicator"]["open"]:
        click_at(*center(state()["indicator"]["rect"]))
    choose("Open LarkAura Studio")
    wait(lambda: window("LarkAura Studio"), "Studio opened from the menu", 30)
    out["studio_opened"] = True
    time.sleep(3)
    call("Screenshot", os.path.join(OUT, "panel-opened-studio.png"))
    subprocess.run(["pkill", "-f", os.environ["SPIKE_STUDIO"]], check=False)
    extension("disable")
    wait(lambda: state()["indicator"] is None, "icon removed", 10)
    out["icon_removed_on_disable"] = True
    return out


def ghost():
    """Whether a dead renderer's last frame stays on screen, and for how long."""
    rows = []
    wait(placed, "overlays placed", 30)
    for delay in (2, 7, 32):
        time.sleep(1.5)
        before = overlays()[0]
        killed = time.monotonic()
        os.kill(before["pid"], signal.SIGKILL)
        wait(lambda: all(w["pid"] != before["pid"] for w in overlays()), "old frame hidden", 20)
        gone = round(time.monotonic() - killed, 2)
        wait(placed, "replacement placed", delay + 15)
        rows.append({"old_frame_hidden_s": gone, "replacement_ready_s": round(time.monotonic() - killed, 2)})
    return rows


def product():
    out = {}
    wait(placed, "overlays placed on every display", 30)
    time.sleep(2)
    s = state()
    own = [w for w in s["windows"] if w["overlay"]]
    out["placement"] = {
        "displays": len(s["workAreas"]), "overlays": len(own),
        "rects": [w["rect"] for w in own], "work_areas": s["workAreas"],
        "dock_type": all(w["type"] == 2 for w in own), "on_all_workspaces": all(w["onAll"] for w in own),
        "skip_taskbar": all(w["skip"] for w in own), "in_window_list": "LarkAura Overlay" in s["tabList"],
        "focus": s["focus"], "top_window_is_overlay": s["windows"][-1]["overlay"],
    }
    runs = [exercise() for _ in range(3)]
    out["input"] = {"runs": runs, "complete": sum(map(complete, runs)), "of": len(runs)}
    # Pause and full-screen checks need the overlay on the display with the window.
    primary = next(w for w in own if w["visible"] and w["rect"] == s["workArea"])
    pid = primary["pid"]
    out["overlay_processes"] = [{"pid": w["pid"], "rect": w["rect"], "visible": w["visible"],
                                 "process_state": process_state(w["pid"])} for w in own]

    time.sleep(6)
    out["active_usage"] = usage(pid, 2)
    out["resting"] = watch(3)
    r = window(TARGET)["rect"]
    busy = []
    mark = log_mark()
    for i in range(50):
        call("Pointer", "move", float(r[0] + 100 + i * 4), float(r[1] + 100))
        time.sleep(0.1)
    busy = stats_since(mark)
    out["while_working"] = busy[-3:]
    time.sleep(7)
    out["after_working"] = watch(3)

    write_settings(aura="quiet-snow", intensity=1.0)
    time.sleep(9)
    out["changed_aura"] = watch(2)[-1:]
    call("Screenshot", os.path.join(OUT, "product-snow-on-window.png"))

    write_settings(aura="quiet-snow", intensity=1.0, paused=True)
    time.sleep(1.5)
    out["paused_usage"] = usage(pid)
    write_settings(aura="quiet-snow", intensity=1.0, paused=False)
    time.sleep(1)
    out["resumed"] = watch(2)[-1:]

    call("Fullscreen", TARGET, "true")
    time.sleep(2)
    out["fullscreen_usage"] = usage(pid)
    out["fullscreen_input"] = exercise()
    call("Fullscreen", TARGET, "false")
    time.sleep(1.5)
    out["after_fullscreen"] = watch(2)[-1:]

    out["state_before_kill"] = process_state(pid)
    os.kill(pid, signal.SIGKILL)
    started = time.monotonic()
    wait(lambda: pid not in [w["pid"] for w in overlays()], "dead renderer hidden", 15)
    out["dead_frame_hidden_seconds"] = round(time.monotonic() - started, 2)
    wait(lambda: placed() and pid not in [w["pid"] for w in overlays()], "renderer restarted", 15)
    out["crash_recovery_seconds"] = round(time.monotonic() - started, 2)
    time.sleep(1.5)
    after_crash = exercise()
    out["input_after_crash"] = {"run": after_crash, "complete": complete(after_crash)}

    pids = [w["pid"] for w in overlays()]
    extension("disable")
    wait(lambda: not overlays(), "overlays removed", 10)
    time.sleep(1.5)
    out["processes_left_after_disable"] = sum(os.path.exists(f"/proc/{p}") for p in pids)
    extension("enable")
    wait(placed, "overlays back after enable", 15)
    out["restored_after_enable"] = True
    time.sleep(1.5)
    final = exercise()
    out["final_input"] = {"run": final, "complete": complete(final)}
    return out


def shot(name):
    path = os.path.join(OUT, f"show-{name}.png")
    call("Screenshot", path)
    return path


def read_png(path):
    """Decodes an 8-bit, non-interlaced PNG into rows of bytes."""
    with open(path, "rb") as f:
        data = f.read()
    pos, idat, width, height, channels = 8, [], 0, 0, 4
    while pos < len(data):
        size, kind = struct.unpack(">I4s", data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + size]
        if kind == b"IHDR":
            width, height, _depth, color = struct.unpack(">IIBB", body[:10])
            channels = {2: 3, 6: 4}[color]
        elif kind == b"IDAT":
            idat.append(body)
        pos += size + 12
    raw = zlib.decompress(b"".join(idat))
    stride, rows, previous = width * channels, [], bytearray(width * channels)
    for y in range(height):
        start = y * (stride + 1)
        kind, row = raw[start], bytearray(raw[start + 1:start + 1 + stride])
        for i in range(stride):
            a = row[i - channels] if i >= channels else 0
            b = previous[i]
            c = previous[i - channels] if i >= channels else 0
            if kind == 1:
                row[i] = (row[i] + a) & 255
            elif kind == 2:
                row[i] = (row[i] + b) & 255
            elif kind == 3:
                row[i] = (row[i] + (a + b) // 2) & 255
            elif kind == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                row[i] = (row[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(row)
        previous = row
    return rows, channels


def marks_on_window(path, rect):
    """Counts pixels in a plain part of the window that differ from its background."""
    rows, channels = read_png(path)
    x, y, w, h = rect
    plain = rows[y + 60][(x + 20) * channels:(x + 21) * channels]
    count = 0
    for py in range(y + 60, y + h // 2 - 30, 2):
        row = rows[py]
        for px in range(x + 20, x + w - 20, 2):
            at = px * channels
            count += any(abs(row[at + c] - plain[c]) > 24 for c in range(3))
    return count


def behind_windows_stays_behind():
    """Moves and raises the window, and looks for effects drawn over it."""
    write_settings(aura="blossom-drift", intensity=1.0, tuning={"placement": "everywhere"})
    time.sleep(8)
    call("Place", TARGET, 200, 150, 700, 420)
    time.sleep(0.4)
    over = marks_on_window(shot("detector-over"), window(TARGET)["rect"])
    write_settings(aura="blossom-drift", intensity=1.0, tuning={"placement": "behind_windows"})
    time.sleep(3)
    frames = []
    for i in range(24):
        call("Place", TARGET, 200 + i * 6, 150 + (i % 3) * 8, 700, 420)
        call("Activate", TARGET)
        path = shot("behind-probe")
        frames.append(marks_on_window(path, window(TARGET)["rect"]))
    call("Place", TARGET, 200, 150, 700, 420)
    return {"marks_when_over_windows": over, "frames": len(frames), "frames_with_marks": sum(n > 0 for n in frames),
            "most_marks": max(frames)}


def displays():
    out = {}
    wait(placed, "overlays placed", 30)
    wait(lambda: state()["indicator"], "top bar icon", 10)
    time.sleep(1.5)
    labels = [i["label"] for i in open_menu()["items"]]
    out["switches"] = [l for l in labels if l.startswith("Display ")]
    close_menu()
    second = out["switches"][1]
    choose(second)
    wait(lambda: len(overlays()) == 1, "second display off", 10)
    out["after_off"] = {"overlays": len(overlays()), "saved": saved().get("displays_off")}
    close_menu()
    choose(second)
    wait(lambda: len(overlays()) == 2, "second display back", 10)
    wait(placed, "both displays placed", 15)
    out["after_on"] = {"overlays": len(overlays()), "saved": saved().get("displays_off")}
    close_menu()
    got = exercise()
    out["input"] = {"run": got, "complete": complete(got)}
    return out


def snowball():
    """Holding the button still on lying snow packs a ball; a quick click does not."""
    out = {}
    wait(placed, "overlays placed", 30)
    write_settings(aura="deep-snow", intensity=1.0, tuning={"speed": 2.5, "pile_depth": 1.0})
    time.sleep(25)
    bottom = state()["workArea"][1] + state()["workArea"][3]
    x, y = 1000.0, float(bottom - 10)
    call("Pointer", "move", x, y)
    time.sleep(0.5)
    shot("snowball-before")
    call("Pointer", "press", x, y)
    time.sleep(0.08)
    call("Pointer", "release", x, y)
    time.sleep(0.6)
    shot("snowball-quick-click")
    call("Pointer", "press", x, y)
    time.sleep(0.25)
    shot("snowball-packing")
    time.sleep(0.5)
    shot("snowball-held")
    for i in range(12):
        call("Pointer", "move", x - i * 10, y)
        time.sleep(0.03)
    shot("snowball-dragged")
    call("Pointer", "release", x - 110, y)
    time.sleep(1.5)
    shot("snowball-set-down")
    out["background_menu_open"] = bool(state().get("backgroundMenuOpen"))
    out["shots"] = ["before", "quick-click", "packing", "held", "dragged", "set-down"]
    return out


def close_menu():
    if state()["indicator"]["open"]:
        click_at(*center(state()["indicator"]["rect"]))


def showcase():
    out = {"auras": {}}
    wait(placed, "overlays placed", 30)
    wait(lambda: state()["indicator"], "top bar icon", 10)
    out["menu"] = [i["label"] for i in open_menu()["items"]]
    shot("menu")
    close_menu()
    for aura in AURAS:
        write_settings(aura=aura, intensity=1.0)
        time.sleep(9)
        pid = overlays()[0]["pid"]
        numbers = usage(pid, 3)
        shot(aura)
        got = exercise()
        out["auras"][aura] = {**numbers, "input_complete": complete(got), "input": got,
                              "stat": watch(1.2)[-1:]}
    decorations = {
        "trees": [{"x": 0.07}, {"x": 0.965, "size": 0.8, "candy": False}],
        "snowmen": [{"x": 0.85}, {"x": 0.76, "size": 0.75, "hat": False}],
    }
    snow = {"aura": "deep-snow", "intensity": 1.0, "decorations": decorations}
    write_settings(**snow, tuning={"speed": 2.5, "pile_depth": 1.0})
    time.sleep(40)
    shot("piles")
    pid = overlays()[0]["pid"]
    out["scene_usage"] = usage(pid, 3)
    out["scene_stat"] = watch(1.2)[-1:]
    r = window(TARGET)["rect"]
    call("Place", TARGET, r[0] + 150, r[1] + 40, r[2], r[3])
    time.sleep(0.3)
    shot("thrown-off")
    call("Place", TARGET, 200, 150, 700, 420)
    time.sleep(10)
    bottom = state()["workArea"][1] + state()["workArea"][3]
    # Holding the button still packs a snowball; brushing alone only sweeps
    # snow off, and a quick click does nothing.
    call("Pointer", "move", 300.0, float(bottom - 14))
    time.sleep(0.1)
    call("Pointer", "press", 300.0, float(bottom - 14))
    time.sleep(0.7)
    for i in range(30):
        call("Pointer", "move", 300.0 + i * 16, float(bottom - 14))
        time.sleep(0.03)
    shot("snowball")
    call("Pointer", "release", 780.0, float(bottom - 14))
    time.sleep(1)
    choose("Send the Sleigh")
    close_menu()
    choose("Send the Snowplough")
    close_menu()
    time.sleep(5)
    shot("visitors")
    choose("Shake Off Snow and Leaves")
    close_menu()
    time.sleep(0.25)
    shot("shaken")
    for place in ("desktop", "behind_windows", "everywhere"):
        write_settings(aura="blossom-drift", intensity=1.0, tuning={"placement": place})
        time.sleep(8)
        call("Activate", TARGET)
        time.sleep(0.5)
        shot(f"place-{place}")
        got = exercise()
        s = state()
        out[place] = {"input_complete": complete(got), "focus": s["focus"],
                      "order": [w["title"] for w in s["windows"]]}
    out["behind_check"] = behind_windows_stays_behind()
    return out


def perf():
    out = {}
    wait(placed, "overlays placed", 30)
    shell = state()["shellPid"]
    call("Pointer", "move", 5.0, 5.0)
    full = {"trees": [{"x": 0.05}, {"x": 0.1}, {"x": 0.95}],
            "snowmen": [{"x": 0.8}, {"x": 0.85}, {"x": 0.9}]}
    runs = [("paused", {"aura": "first-light", "paused": True})]
    runs += [(a, {"aura": a}) for a in AURAS]
    runs.append(("full-scene", {"aura": "deep-snow", "intensity": 1.0, "decorations": full,
                                "tuning": {"speed": 2.5, "pile_depth": 1.0}}))
    only = os.environ.get("PERF_ONLY")
    if only:
        runs = [r for r in runs if r[0] in only.split(",")]
    for name, values in runs:
        write_settings(battery="ignore", **values)
        time.sleep(30 if name == "full-scene" else 9)
        pid = overlays()[0]["pid"]
        out[name] = {**measure({"engine": pid, "shell": shell}, pid, 6), "stat": watch(1.2)[-1:]}
    return out


def storm():
    wait(placed, "overlays placed", 30)
    wallpaper = os.environ.get("SPIKE_WALLPAPER")
    if wallpaper:
        for key in ("picture-uri", "picture-uri-dark"):
            subprocess.run(["gsettings", "set", "org.gnome.desktop.background", key, f"file://{wallpaper}"], check=True)
        time.sleep(1.5)
    write_settings(aura="night-storm", intensity=1.0, battery="ignore")
    time.sleep(8)
    for i in range(170):
        call("Screenshot", os.path.join(OUT, f"storm-{i:03}.png"))
        time.sleep(0.05)
    return {"frames": 170}


result = {"scenario": SCENARIO, "kind": KIND, "settle": SETTLE}
target = None
try:
    wait(lambda: not state()["startingUp"], "shell startup", 45)
    time.sleep(1.5)
    wait(lambda: state()["overview"] is False, "overview hidden", 20)
    time.sleep(1.0)
    call("SetOverlayType", os.environ.get("SPIKE_FULL_TYPE", "DOCK"))
    target = subprocess.Popen(["gjs", "-m", f"{DIR}/target.js", TARGET],
                              stdout=open(LOG, "a"), stderr=subprocess.DEVNULL)
    wait(lambda: window(TARGET), "target window")
    time.sleep(0.5)
    call("Place", TARGET, 200, 150, 700, 420)
    time.sleep(0.5)
    call("Activate", TARGET)
    time.sleep(0.3)
    result["baseline"] = exercise()
    result["baseline_complete"] = complete(result["baseline"])
    if SCENARIO == "after-control":
        result["cycles"] = cycle_after_control()
    elif SCENARIO == "after-window":
        result["cycles"] = cycle_after_window()
    elif SCENARIO == "repeat":
        result["cycles"] = cycle_repeat()
    elif SCENARIO == "startup":
        time.sleep(1)
    elif SCENARIO == "panel":
        result["panel"] = panel()
    elif SCENARIO == "ghost":
        result["ghost"] = ghost()
    elif SCENARIO == "perf":
        result["perf"] = perf()
    elif SCENARIO == "storm":
        result["storm"] = storm()
    elif SCENARIO == "showcase":
        result["showcase"] = showcase()
    elif SCENARIO == "displays":
        result["displays"] = displays()
    elif SCENARIO == "behind":
        wait(placed, "overlays placed", 30)
        result["behind"] = behind_windows_stays_behind()
    elif SCENARIO == "snowball":
        result["snowball"] = snowball()
    elif SCENARIO == "product":
        result["product"] = product()
    elif SCENARIO == "launch-soak":
        result["launches"] = launch_soak()
        # An empty frame costs about 6 ms of GPU time per 2 s, so a stalled engine is far below that.
        result["stalled"] = sum(r["gpu_ms"] < 3 for r in result["launches"])
    else:
        result["auras"] = run_auras()
    if "cycles" in result:
        result["complete"] = sum(c["complete"] for c in result["cycles"])
        result["total"] = len(result["cycles"])
except Exception as e:
    result["error"] = repr(e)
finally:
    if target:
        target.terminate()
    with open(os.path.join(OUT, "results.json"), "w") as f:
        json.dump(result, f, indent=1)
