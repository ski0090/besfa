"""Clicks the Hierarchy panel a windowless game draws, with `pointer` commands,
and checks what the game reports. See features/panels.md.
usage: python .claude/skills/verify-besfa/panels.py <project dir>"""
import json, os, queue, subprocess, sys, threading, time

project = sys.argv[1]
env = dict(os.environ, NO_COLOR="1", BESFA_EDIT_MODE="1", BESFA_HEADLESS="1",
           BESFA_PANELS="hierarchy", BESFA_SCALE="1")
game = subprocess.Popen(["cargo", "run", "-q", "--features", "bevy/dynamic_linking,besfa_editor"],
                        cwd=project, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                        stderr=subprocess.STDOUT, text=True, encoding="utf-8", errors="replace")
lines = queue.Queue()
threading.Thread(target=lambda: [lines.put(l.rstrip()) for l in game.stdout], daemon=True).start()
seq = 0
failures = []


def send(command, wait=False):
    global seq
    if wait:
        seq += 1
        command["seq"] = seq
    game.stdin.write(json.dumps(command) + "\n")
    game.stdin.flush()
    return seq


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


def click(x, y):
    for event in ("move", "down", "up"):
        command = {"command": "pointer", "event": event, "x": x, "y": y}
        if event != "move":
            command["button"] = "left"
        send(command)
        time.sleep(0.12)


def check(name, ok, detail=""):
    print(("ok   " if ok else "FAIL ") + name + (f"  {detail}" if detail else ""), flush=True)
    if not ok:
        failures.append(name)


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

    click(50, 41)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("the selected row again clears the selection", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": None}], str(kinds(seen, "selected")))

    # The scene view is what the panel leaves: 1280 wide, its middle moved right.
    click(754, 360)
    seen = reports(2, lambda r: r.get("type") == "selected")
    check("a click on the cube in the scene view selects it", kinds(seen, "selected")[-1:] == [{"type": "selected", "id": cube}], str(kinds(seen, "selected")))

    click(50, 400)
    seen = reports(1.5)
    check("a click on the empty panel does not reach the scene", not kinds(seen, "selected"), str(kinds(seen, "selected")))

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
    time.sleep(1.5)
    n = send({"command": "save"}, wait=True)
    seen = reports(10, lambda r: r.get("type") == "done" and r.get("seq") == n)
    after = open(scene, encoding="utf-8").read()
    check("Play, Stop, Save leaves the scene file as it was", before == after, f"{len(before)} vs {len(after)} bytes")
    check("the game is still running", game.poll() is None)
finally:
    subprocess.run(["taskkill", "/PID", str(game.pid), "/T", "/F"], capture_output=True)
    shot = os.path.join(project, "panel-shot.png")
    if os.path.exists(shot):
        os.remove(shot)
print("FAILED: " + ", ".join(failures) if failures else "all passed")
sys.exit(1 if failures else 0)
