"""Clicks and types into the panels a windowless game draws, the Hierarchy and
the Inspector, with `pointer` and `panel_input` commands, and checks what the
game reports. See features/panels.md.
usage: python .claude/skills/verify-besfa/panels.py <project dir>"""
import json, os, queue, subprocess, sys, threading, time

project = sys.argv[1]
env = dict(os.environ, NO_COLOR="1", BESFA_EDIT_MODE="1", BESFA_HEADLESS="1",
           BESFA_PANELS="hierarchy,inspector", BESFA_SCALE="1")


def start():
    """Starts the game; `game` and `lines` are the one running now."""
    global game, lines
    game = subprocess.Popen(["cargo", "run", "-q", "--features", "bevy/dynamic_linking,besfa_editor"],
                            cwd=project, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
    lines = queue.Queue()
    out, sink = game.stdout, lines
    threading.Thread(target=lambda: [sink.put(l.rstrip()) for l in out], daemon=True).start()


def stop():
    subprocess.run(["taskkill", "/PID", str(game.pid), "/T", "/F"], capture_output=True)


start()
seq = 0
failures = []
NAME = "bevy_ecs::name::Name"


def send(command, wait=False):
    global seq
    if wait:
        seq += 1
        command["seq"] = seq
    game.stdin.write(json.dumps(command) + "\n")
    game.stdin.flush()
    return seq


def send_together(commands):
    """Several commands in one write, so the game reads them in one frame."""
    game.stdin.write("".join(json.dumps(command) + "\n" for command in commands))
    game.stdin.flush()


def reports(seconds, until=None):
    """Reports seen within `seconds`, stopping early at one `until` accepts."""
    seen, end = [], time.time() + seconds
    while time.time() < end:
        try:
            line = lines.get(timeout=0.1)
        except queue.Empty:
            if game.poll() is not None:
                break
            continue
        if line.startswith("@besfa "):
            report = json.loads(line[7:])
            seen.append(report)
            if until and until(report):
                break
        elif "ERROR" in line or "panicked" in line:
            print("   log:", line[:240])
    return seen


def press(event, x, y, button="left"):
    command = {"command": "pointer", "event": event, "x": x, "y": y}
    if event != "move":
        command["button"] = button
    send(command)
    time.sleep(0.12)


def click(x, y):
    for event in ("move", "down", "up"):
        press(event, x, y)


def panel_input(**fields):
    send({"command": "panel_input", **fields})
    time.sleep(0.08)


def key(name, **modifiers):
    for pressed in (True, False):
        panel_input(input="key", key=name, pressed=pressed, modifiers=modifiers)


def focus_a_field(top=30, bottom=140):
    """Clicks down the Inspector from `top` until a field there takes the keyboard."""
    for y in range(top, bottom, 6):
        click(1100, y)
        focus = kinds(reports(0.7, lambda r: r.get("type") == "panel_focus"), "panel_focus")[-1:]
        if focus and focus[0].get("keyboard") and (focus[0].get("ime") or [0, top])[1] >= top:
            return focus[0]
        if focus and focus[0].get("keyboard"):
            send({"command": "focus_lost"})
            reports(0.4)
    return None


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f"  {detail}" if detail else ""), flush=True)
    if not ok:
        failures.append(name)


def near(a, b):
    """Whether two component values are the same but for a float's rounding."""
    if isinstance(a, dict) and isinstance(b, dict):
        return a.keys() == b.keys() and all(near(a[k], b[k]) for k in a)
    if isinstance(a, list) and isinstance(b, list):
        return len(a) == len(b) and all(near(x, y) for x, y in zip(a, b))
    if isinstance(a, float) and isinstance(b, float):
        return abs(a - b) < 1e-5
    return a == b


def kinds(seen, kind):
    return [r for r in seen if r.get("type") == kind]


try:
    first = reports(300, lambda r: r.get("type") == "entities")
    entities = kinds(first, "entities")[-1]["entities"]
    cube = next(e["id"] for e in entities if e.get("name") == "Cube")
    check("the game starts with panels and no window", True, f"{len(entities)} entities")
    time.sleep(1.5)  # a few frames, so the panel is laid out

    click(50, 41)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("a click on the Cube row selects it", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": cube}], str(kinds(seen, "selected")))

    click(171, 11)
    seen = reports(2, lambda r: r.get("type") == "panel")
    check("the delete button asks the editor", kinds(seen, "panel") == [{"type": "panel", "action": "delete"}], str(kinds(seen, "panel")))

    click(115, 11)
    time.sleep(0.4)
    asked = []
    for y in (36, 44, 52):
        click(125, y)
        asked = kinds(reports(1.2, lambda r: r.get("type") == "panel"), "panel")
        if asked:
            break
    check("the add menu asks the editor to spawn", bool(asked) and asked[0].get("action") == "spawn", str(asked))
    middle = (asked[0].get("pointer") or [0, 0]) if asked else [0, 0]
    check("and says where the scene view's middle is", 550 < middle[0] < 650 and middle[1] == 360, str(middle))

    click(50, 41)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("the selected row again clears the selection", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": None}], str(kinds(seen, "selected")))

    # The scene view is what the panel leaves: 1280 wide, its middle moved right.
    click(590, 360)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("a click on the cube in the scene view selects it", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": cube}], str(kinds(seen, "selected")))

    click(50, 400)
    seen = reports(1.5)
    check("a click on the empty panel does not reach the scene", not kinds(seen, "selected"), str(kinds(seen, "selected")))

    # A drag in the scene that starts near the panel is the scene's alone:
    # the panel's edge, at 220 and grabbed up to 3 pixels past it, stays
    # where it is. egui would take a press up to 8 pixels past it.
    press("move", 226, 600)
    press("down", 226, 600)
    press("move", 500, 600)
    press("up", 500, 600)
    click(115, 11)
    time.sleep(0.4)
    click(125, 36)
    asked = kinds(reports(1.2, lambda r: r.get("type") == "panel"), "panel")
    check("a drag that starts beside the panel leaves its edge alone", bool(asked) and asked[0].get("pointer") == middle, str(asked))

    # ---- the Inspector, on the right: the name field
    focus = focus_a_field()
    check("a click on a field of the Inspector takes the keyboard", bool(focus and focus.get("ime")), str(focus))
    key("A", ctrl=True)
    panel_input(input="text", text="Box")
    key("Enter")
    seen = reports(3, lambda r: r.get("type") == "edited")
    edited = kinds(seen, "edited")[-1:]
    check("typing a name and Enter sets it at once and tells the editor",
          bool(edited) and edited[0]["component"] == NAME and edited[0]["before"] == "Cube" and edited[0]["after"] == "Box",
          str(edited))
    seen += reports(2, lambda r: r.get("type") == "entities")
    names = [e.get("name") for r in kinds(seen, "entities")[-1:] for e in r["entities"]]
    check("the Hierarchy lists the new name", "Box" in names, str(names))

    focus_a_field()
    key("A", ctrl=True)
    panel_input(input="copy")
    seen = reports(2, lambda r: r.get("type") == "copied")
    check("copying in a field hands the text to the editor", kinds(seen, "copied")[-1:] == [{"type": "copied", "text": "Box"}], str(kinds(seen, "copied")))

    send({"command": "focus_lost"})
    seen = reports(2, lambda r: r.get("type") == "panel_focus")
    released = kinds(seen, "panel_focus")[-1:]
    check("the field lets the keyboard go when the viewport loses it", bool(released) and not released[0]["keyboard"], str(released))

    focus_a_field()
    click(800, 600)
    seen = reports(2, lambda r: r.get("type") == "panel_focus")
    released = kinds(seen, "panel_focus")[-1:]
    check("and when the scene is pressed", bool(released) and not released[0]["keyboard"], str(released))
    click(590, 360)
    reports(1.0)

    # What is typed and not yet entered is set before a save that follows the
    # field losing the keyboard, as Ctrl+S does: the file has it.
    scene = os.path.join(project, "assets", "scenes", "main.scn.ron")
    focus_a_field()
    key("A", ctrl=True)
    panel_input(input="text", text="Zed")
    seq += 1
    send_together([{"command": "focus_lost"}, {"command": "save", "seq": seq}])
    seen = reports(10, lambda r: r.get("type") == "saved")
    order = [r["type"] for r in seen if r["type"] in ("edited", "saved")]
    check("a save right after typing waits for the typed value", order == ["edited", "saved"] and '"Zed"' in open(scene, encoding="utf-8").read(), str(order))

    # And before a click on the scene changes the selection, even when the
    # press and the release reach the game in one frame.
    focus_a_field()
    key("A", ctrl=True)
    panel_input(input="text", text="Qux")
    send_together([{"command": "pointer", "event": event, "x": 800, "y": 600, **({} if event == "move" else {"button": "left"})} for event in ("move", "down", "up")])
    seen = reports(3, lambda r: r.get("type") == "selected")
    edited = kinds(seen, "edited")[-1:]
    check("a fast click on the scene while typing sets the typed value first",
          bool(edited) and edited[0]["id"] == cube and edited[0]["before"] == "Zed" and edited[0]["after"] == "Qux"
          and kinds(seen, "selected")[-1:] == [{"type": "selected", "id": None}], str(edited) + str(kinds(seen, "selected")))
    click(590, 360)
    reports(1.0)

    # And before a handle is dragged: the drag starts from the value typed
    # in a field of the Transform, below the name, and not entered.
    focus_a_field(top=60, bottom=300)
    key("A", ctrl=True)
    panel_input(input="text", text="7")
    press("move", 590, 300)
    press("down", 590, 300)
    for y in range(292, 259, -8):
        press("move", 590, y)
    press("up", 590, 260)
    edited = [r for r in kinds(reports(2.5), "edited") if r["id"] == cube]
    check("a handle dragged right after typing starts from the typed value",
          len(edited) == 2 and not near(edited[0]["before"], edited[0]["after"]) and near(edited[1]["before"], edited[0]["after"])
          and not near(edited[1]["after"], edited[1]["before"]), json.dumps([e["after"] for e in edited]))
    if edited:
        n = send({"command": "set", "id": cube, "component": edited[0]["component"], "value": edited[0]["before"]}, wait=True)
        reports(5, lambda r: r.get("type") == "done" and r.get("seq") == n)

    # Back to the template's name, so the next run finds the Cube.
    n = send({"command": "set", "id": cube, "component": NAME, "value": "Cube"}, wait=True)
    reports(5, lambda r: r.get("type") == "done" and r.get("seq") == n)

    # A menu left open closes on a press in the scene.
    click(115, 11)
    time.sleep(0.4)
    click(800, 600)
    reports(0.8)
    click(125, 36)
    seen = reports(1.2)
    check("a press in the scene closes the add menu", not kinds(seen, "panel"), str(kinds(seen, "panel")))

    # A right press on the panel is the panel's: the fly keys the editor
    # sends while it is held do not fly the scene view on the next hold.
    click(800, 600)
    reports(0.8)
    press("move", 50, 400)
    press("down", 50, 400, "right")
    send({"command": "fly", "right": 1, "up": 0, "forward": 0, "fast": False})
    time.sleep(0.3)
    press("up", 50, 400, "right")
    press("move", 800, 600)
    press("down", 800, 600, "right")
    time.sleep(1.5)
    press("up", 800, 600, "right")
    reports(0.5)
    click(590, 360)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("fly keys during a press on the panel leave the scene view still", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": cube}], str(kinds(seen, "selected")))

    scene = os.path.join(project, "assets", "scenes", "main.scn.ron")
    n = send({"command": "save"}, wait=True)
    reports(10, lambda r: r.get("type") == "done" and r.get("seq") == n)
    before = open(scene, encoding="utf-8").read()

    send({"command": "play"})
    time.sleep(2)
    n = send({"command": "screenshot", "file": "panel-shot.png", "game": True}, wait=True)
    seen = reports(30, lambda r: r.get("type") == "screenshot")
    shot = kinds(seen, "screenshot")
    check("a picture of the game is still taken while it plays", bool(shot) and not shot[-1].get("error"), str(shot))
    send({"command": "reset"})
    seen = reports(20, lambda r: r.get("type") == "entities")
    time.sleep(1.5)
    click(50, 41)
    seen = reports(2, lambda r: r.get("type") == "selected")
    picked = kinds(seen, "selected")[-1:]
    check("the panel is still there after Stop", bool(picked) and picked[0]["id"] is not None, str(picked))
    n = send({"command": "save"}, wait=True)
    seen = reports(10, lambda r: r.get("type") == "done" and r.get("seq") == n)
    after = open(scene, encoding="utf-8").read()
    check("Play, Stop, Save leaves the scene file as it was", before == after, f"{len(before)} vs {len(after)} bytes")
    check("the game is still running", game.poll() is None)

    # ---- what the panels and the scene view look like is kept for the next
    # game: zoom the scene view and make the Hierarchy wider by its edge
    press("move", 590, 300)
    send({"command": "pointer", "event": "scroll", "delta": 240})
    press("move", 221, 300)
    press("down", 221, 300)
    press("move", 400, 300)
    press("up", 400, 300)
    states = kinds(reports(2.5), "panel_state")
    state = states[-1]["state"] if states else "{}"
    check("the game tells the editor what its panels look like", bool(states), state)
    kept = json.loads(state)
    check("and only when that changes", not kinds(reports(2.0), "panel_state"))

    # The next game is built again, as after a change to the game's code.
    stop()
    os.utime(os.path.join(project, "src", "main.rs"))
    start()
    first = reports(300, lambda r: r.get("type") == "entities")
    first += reports(1.5)
    fresh = kinds(first, "panel_state")
    fresh = json.loads(fresh[-1]["state"]) if fresh else {}
    check("a new game starts with the scene view where it comes", fresh.get("camera") not in (None, kept.get("camera")), f"{fresh.get('camera')} vs {kept.get('camera')}")
    # The game says how much of the viewport its panels cover.
    wide = kept.get("widths", [None])[0]
    layout = kinds(first, "panel_layout")[-1:]
    check("and with its Hierarchy as wide as it comes", bool(layout) and layout[0]["left"] not in (None, wide), f"{layout} vs {wide}")

    send({"command": "panel_state", "state": state})
    seen = reports(3, lambda r: r.get("type") == "panel_layout")
    layout = kinds(seen, "panel_layout")[-1:]
    check("handed the last game's, the Hierarchy is as wide as it was", bool(layout) and layout[0]["left"] == wide, f"{layout} vs {wide}")
    press("move", 401, 300)
    press("down", 401, 300)
    press("move", 380, 300)
    press("up", 380, 300)
    again = kinds(reports(2.5), "panel_state")
    camera = json.loads(again[-1]["state"]).get("camera") if again else None
    check("and the scene view's camera is where it was", camera is not None and camera == kept.get("camera"), f"{camera} vs {kept.get('camera')}")

    # Stop starts the scene over in the same game, which puts the scene
    # view's camera back at the scene's own: handed its state, it stays.
    n = send({"command": "reset"}, wait=True)
    reports(30, lambda r: r.get("type") == "hello")
    n = send({"command": "panel_state", "state": again[-1]["state"] if again else state}, wait=True)
    reports(5, lambda r: r.get("type") == "done" and r.get("seq") == n)
    after = kinds(reports(2.5), "panel_state")
    camera = json.loads(after[-1]["state"]).get("camera") if after else kept.get("camera")
    check("and after Stop too", camera == kept.get("camera") and game.poll() is None, f"{camera} vs {kept.get('camera')}")
finally:
    stop()
    shot = os.path.join(project, "panel-shot.png")
    if os.path.exists(shot):
        os.remove(shot)
print("FAILED: " + ", ".join(failures) if failures else "all passed")
sys.exit(1 if failures else 0)
