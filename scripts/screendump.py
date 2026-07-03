#!/usr/bin/env python3
"""Connect to a QEMU QMP socket and capture the VGA screen to a PNG."""
import socket, json, sys, time

sock_path = sys.argv[1] if len(sys.argv) > 1 else "build/qmp.sock"
out = sys.argv[2] if len(sys.argv) > 2 else "build/screen.png"

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
for _ in range(50):
    try:
        s.connect(sock_path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        time.sleep(0.2)
f = s.makefile("rw")

def cmd(obj):
    f.write(json.dumps(obj) + "\n")
    f.flush()
    return json.loads(f.readline())

f.readline()  # QMP greeting
cmd({"execute": "qmp_capabilities"})
resp = cmd({"execute": "screendump", "arguments": {"filename": out, "format": "png"}})
print(json.dumps(resp))
