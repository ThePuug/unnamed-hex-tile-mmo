"""Pushes a build into a running Blender so its viewport follows the
command line. The MCP extension's bridge takes one JSON request,
`{"type": "execute", "code": ..., "strict_json": bool}`, terminated by a
NUL byte, and answers the same way. Nothing listening is not an error:
the build is the product, the live view a convenience."""

import json
import os
import socket
from pathlib import Path

HOST = "127.0.0.1"
PORT = int(os.environ.get("BLENDER_MCP_PORT", "9877"))
PACKAGE_ROOT = str(Path(__file__).resolve().parents[1])


def push(name, seed):
    """Rebuilds `name` at `seed` in the listening Blender through
    `modelgen.live`. Returns its check report, or None when no Blender
    listens."""
    code = (
        "import sys\n"
        f"sys.path.insert(0, {PACKAGE_ROOT!r}) if {PACKAGE_ROOT!r} not in sys.path else None\n"
        "import modelgen\n"
        f"result = modelgen.live({name!r}, {seed})\n"
    )
    request = json.dumps({"type": "execute", "code": code, "strict_json": True}) + "\0"
    try:
        with socket.create_connection((HOST, PORT), timeout=1.0) as s:
            s.settimeout(60.0)
            s.sendall(request.encode("utf-8"))
            buf = b""
            while b"\0" not in buf:
                chunk = s.recv(65536)
                if not chunk:
                    break
                buf += chunk
    except OSError:
        return None
    response = json.loads(buf.split(b"\0")[0].decode("utf-8")) if buf else {}
    if response.get("status") != "ok":
        raise RuntimeError(response.get("message", "live Blender refused the build"))
    return response.get("result")
