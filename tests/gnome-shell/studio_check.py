"""Drives the real Studio app with real pointer input, through the overlay."""
import ast, json, os, subprocess, time

OUT = os.environ["SPIKE_OUT"]
STUDIO = os.environ["SPIKE_STUDIO"]
CLICKS = json.loads(os.environ.get("STUDIO_CLICKS", "[]"))
TITLE = "LarkAura Studio"
BUS = ["gdbus", "call", "--session", "--dest", "dev.larklabs.AuraSpike",
       "--object-path", "/dev/larklabs/AuraSpike", "--method"]
SETTINGS = os.path.join(OUT, "home", ".config", "larkaura", "settings.json")


def call(method, *args):
    r = subprocess.run(BUS + [f"dev.larklabs.AuraSpike.{method}", *map(str, args)],
                       capture_output=True, text=True, timeout=15)
    if r.returncode:
        raise RuntimeError(f"{method}: {r.stderr.strip()}")
    return r.stdout.strip()


def state():
    return json.loads(ast.literal_eval(call("State"))[0])


def wait(check, what, timeout=30):
    end = time.time() + timeout
    while time.time() < end:
        try:
            if check():
                return
        except RuntimeError:
            pass  # GNOME Shell is still starting.
        time.sleep(0.2)
    raise TimeoutError(what)


def window():
    return next((w for w in state()["windows"] if w["title"] == TITLE), None)


def background():
    out = {}
    for key in ("picture-uri", "picture-uri-dark", "picture-options"):
        out[key] = subprocess.run(["gsettings", "get", "org.gnome.desktop.background", key],
                                  capture_output=True, text=True).stdout.strip()
    return out


def settings():
    try:
        with open(SETTINGS) as f:
            return json.load(f)
    except OSError:
        return None


def click(x, y):
    call("Pointer", "move", float(x), float(y))
    time.sleep(0.15)
    call("Pointer", "press", 0.0, 0.0)
    time.sleep(0.08)
    call("Pointer", "release", 0.0, 0.0)


def decorations():
    return (settings() or {}).get("decorations", {})


def wallpaper():
    return background()["picture-uri"]


# What each named click must have changed.
CHECKS = {
    "sleigh often": lambda: decorations().get("sleigh") == "often",
    "add snowman": lambda: len(decorations().get("snowmen", [])) == 1,
    "add tree": lambda: len(decorations().get("trees", [])) == 1,
    "use rainfall at night": lambda: "rainfall-night.png" in wallpaper(),
    "use starfield through the day": lambda: "starfield-day.xml" in wallpaper(),
    "restore wallpaper": lambda: wallpaper() == result["background_before"]["picture-uri"],
}

result = {"steps": [], "failed": []}
app = None
try:
    wait(lambda: not state()["startingUp"], "shell startup", 45)
    time.sleep(1.5)
    wait(lambda: state()["overview"] is False, "overview hidden", 20)
    wait(lambda: any(w["overlay"] for w in state()["windows"]), "overlay", 20)
    result["background_before"] = background()
    log = open(os.path.join(OUT, "studio.log"), "w")
    app = subprocess.Popen([STUDIO], stdout=log, stderr=log)
    wait(window, "studio window", 40)
    time.sleep(1)
    size = os.environ.get("STUDIO_SIZE", "1000,650").split(",")
    call("Place", TITLE, 100, 50, size[0], size[1])
    time.sleep(0.5)
    call("Activate", TITLE)
    time.sleep(4)
    result["window"] = window()["rect"]
    call("Screenshot", os.path.join(OUT, "studio-0.png"))
    for i, step in enumerate(CLICKS, 1):
        click(step["x"], step["y"])
        check = CHECKS.get(step["name"])
        if check:
            try:
                wait(check, step["name"], step.get("wait", 3))
            except TimeoutError:
                result["failed"].append(step["name"])
            time.sleep(0.5)
        else:
            time.sleep(step.get("wait", 1.5))
        call("Screenshot", os.path.join(OUT, f"studio-{i}.png"))
        result["steps"].append({"name": step["name"], "settings": settings(), "background": background(),
                                "focus": state()["focus"]})
    result["still_running"] = app.poll() is None
except Exception as e:
    result["error"] = repr(e)
finally:
    if app:
        app.terminate()
    with open(os.path.join(OUT, "results.json"), "w") as f:
        json.dump(result, f, indent=1)
