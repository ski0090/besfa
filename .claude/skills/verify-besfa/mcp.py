"""Drives a besfa MCP server over stdio when the session has no mcp__besfa__ tools,
or when the change under test is in crates/besfa_mcp (the session's server predates it).

Usage: python -I mcp.py <server.exe> [server args...] < calls.jsonl
Each input line is one tool call, {"name": "entities", "arguments": {...}}.
Prints each result's text, prefixed ERROR when the tool failed. Closing stdin
at the end stops the game, as the MCP client does.
"""
import json, os, queue, subprocess, sys, threading

# CreateProcess finds no relative path with forward slashes.
command = [os.path.abspath(sys.argv[1]), *sys.argv[2:]]
server = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding="utf-8")
replies = queue.Queue()
threading.Thread(target=lambda: [replies.put(line) for line in server.stdout], daemon=True).start()


def request(id, method, params, timeout):
    server.stdin.write(json.dumps({"jsonrpc": "2.0", "id": id, "method": method, "params": params}) + "\n")
    server.stdin.flush()
    while True:
        reply = json.loads(replies.get(timeout=timeout))  # queue.Empty: the server hung
        if reply.get("id") == id:
            return reply


request(0, "initialize", {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "verify-besfa"}}, 60)
for id, line in enumerate(sys.stdin, 1):
    if not line.strip():
        continue
    call = json.loads(line)
    wait = call.get("arguments", {}).get("timeout_secs", 30) + 60
    result = request(id, "tools/call", call, wait)["result"]
    print(("ERROR " if result["isError"] else "") + result["content"][0]["text"], flush=True)
server.stdin.close()
try:
    server.wait(timeout=30)
except subprocess.TimeoutExpired:
    subprocess.run(["taskkill", "/PID", str(server.pid), "/T", "/F"], capture_output=True)
