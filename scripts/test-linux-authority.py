#!/usr/bin/env python3
"""Disposable real coordinator startup gate: no FUSE, S3, systemd or credentials."""
import json
import os
from pathlib import Path
import queue
import shlex
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from urllib.parse import urlsplit
from urllib.request import HTTPSHandler, ProxyHandler, Request, build_opener
import uuid

repo = Path(__file__).resolve().parent.parent
binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else repo / "target/debug/s3drive"
if len(sys.argv) not in [1, 2, 4]:
    raise RuntimeError("Use test-linux-authority.py [binary [private-cert private-key]]")
cert_file = Path(sys.argv[2]).resolve() if len(sys.argv) == 4 else None
key_file = Path(sys.argv[3]).resolve() if len(sys.argv) == 4 else None
trust = ssl.create_default_context(cafile=str(cert_file)) if cert_file else None
http = build_opener(ProxyHandler({}), HTTPSHandler(context=trust))
manager = repo / "scripts/linux-session.py"
name = "acceptance"
session = "s3drive-authority-" + uuid.uuid4().hex[:12]


def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, timeout=50, **kwargs)


def wait_for(predicate, message, seconds=30):
    until = time.monotonic() + seconds
    while True:
        result = predicate()
        if result:
            return result
        if time.monotonic() >= until:
            raise AssertionError(message)
        time.sleep(0.1)


def readiness(child):
    result = queue.Queue()
    threading.Thread(target=lambda: result.put(child.stdout.readline()), daemon=True).start()
    line = result.get(timeout=20)
    if not line:
        raise AssertionError("bootstrap coordinator did not report readiness")
    return json.loads(line)


if sys.platform != "linux" or os.getuid() == 0:
    raise RuntimeError("Run coordinator startup acceptance as an ordinary Linux account")
with tempfile.TemporaryDirectory(prefix="s3drive authority café 🦀 $ % ") as directory:
    root = Path(directory)
    env = dict(os.environ, XDG_STATE_HOME=str(root / "session-state"), XDG_CONFIG_HOME=str(root / "session-config"))
    data = root / "database"
    token_file = root / "secret/authority.token"
    run(binary, "authority-token", "--directory", token_file.parent, env=env)
    launch = [str(binary), "authority-serve", "--directory", str(data),
                              "--token-file", str(token_file), "--name-policy", "portable-collision-v1",
                              "--stop-on-stdin-close"]
    if cert_file:
        launch += ["--tls-cert", str(cert_file), "--tls-key", str(key_file)]
    child = subprocess.Popen(launch, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
    try:
        ready = readiness(child)
        endpoint, volume = ready["endpoint"], ready["volume"]
    finally:
        child.stdin.close()
        assert child.wait(timeout=20) == 0
    address = urlsplit(endpoint)
    listen = f"{address.hostname}:{address.port}"
    token = token_file.read_text().strip()  # Owned disposable fixture only; never print.
    startup = root / "session-state/s3drive-authority" / name
    profile = root / "profile"
    profile.write_text("# Retain original profile\n")
    profile.chmod(0o640)
    arguments = ["--component", "authority", "--executable", binary,
                 "--authority-directory", data, "--authority-volume", volume,
                 "--authority-name-policy", "portable-collision-v1",
                 "--authority-token-file", token_file, "--authority-listen", listen,
                 "--shell-profile", profile]
    if cert_file:
        arguments += ["--tls-cert", cert_file, "--tls-key", key_file]
    bad = list(arguments)
    bad[bad.index("--authority-volume") + 1] = "f" * 32
    refused = subprocess.run(list(map(str, [sys.executable, manager, "install", "wrong-volume", *bad])), env=env, capture_output=True, timeout=40)
    assert refused.returncode != 0
    assert profile.read_text() == "# Retain original profile\n"
    assert not (root / "session-config/autostart/s3drive-authority-wrong-volume.desktop").exists()
    before = sorted(p.name for p in data.iterdir())
    overlap_env = dict(env, XDG_STATE_HOME=str(data))
    refused = subprocess.run(list(map(str, [sys.executable, manager, "install", "overlap", *arguments])), env=overlap_env, capture_output=True, timeout=40)
    assert refused.returncode != 0 and sorted(p.name for p in data.iterdir()) == before
    assert profile.read_text() == "# Retain original profile\n"
    run(sys.executable, manager, "install", name, *arguments, env=env)

    def control(action, **kwargs):
        return run(sys.executable, manager, action, name, "--component", "authority", env=env, **kwargs)

    def rpc(value):
        request = Request(endpoint, data=json.dumps(value, ensure_ascii=False).encode(),
                          headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
        with http.open(request, timeout=3) as response:
            return json.load(response)

    def online():
        try:
            reply = rpc({"action": "info"})
            return reply if reply["result"] == "info" and reply["value"]["id"] == volume else False
        except (OSError, ValueError):
            return False

    entry = root / f"session-config/autostart/s3drive-authority-{name}.desktop"
    try:
        assert (startup.stat().st_mode & 0o777) == 0o700
        run("gio", "launch", entry, env=env)
        starts = [subprocess.Popen(list(map(str, [sys.executable, manager, "start", name, "--component", "authority"])), env=env) for _ in range(5)]
        for start in starts:
            assert start.wait(timeout=20) == 0
        wait_for(online, "desktop/duplicate starts did not produce the original coordinator")
        state = json.loads(control("status", capture_output=True).stdout)
        assert state["supervisor_running"] and state["expected_volume"] == volume and state["provider_access_verified"] is False
        runtime = json.loads((startup / "runtime.json").read_text())
        assert runtime["expected_volume"] == volume
        control("stop")
        wait_for(lambda: not online(), "coordinator did not stop")
        # A new login shell has no desktop XDG variables or inherited terminal.
        login_env = {k: v for k, v in env.items() if k not in ["XDG_STATE_HOME", "XDG_CONFIG_HOME"]}
        command = shlex.join(["sh", str(profile)]) + "; exec sleep 600"
        run("tmux", "-L", session, "new-session", "-d", "-s", session, command, env=login_env, cwd="/")
        wait_for(online, "login hook did not restart the pinned coordinator")
        run("tmux", "-L", session, "kill-server", env=env)
        assert online(), "launching terminal loss stopped coordinator"
        create = {"volume": volume, "operation": "1" * 32, "expected_sequence": 0,
                  "mutation": {"kind": "create-entry", "parent": "0" * 32, "parent_revision": 0,
                               "store": None, "name": "資料 café 🦀", "node_kind": "directory"}}
        assert rpc({"action": "apply", "request": create})["result"] == "applied"
        request = {"volume": volume, "operation": "2" * 32, "expected_sequence": 1,
                   "mutation": {"kind": "register-store", "route": "2" * 64, "root": "managed/.s3drive/"}}
        # Submit a real mutation, then deliberately never read its response.
        body = json.dumps({"action": "apply", "request": request}).encode()
        unread = socket.create_connection((address.hostname, address.port), timeout=5)
        if trust:
            unread = trust.wrap_socket(unread, server_hostname=address.hostname)
        unread.sendall((f"POST /v1/namespace HTTP/1.1\r\nHost: {address.netloc}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {len(body)}\r\nConnection: close\r\n\r\n").encode() + body)

        def recorded():
            reply = rpc({"action": "operation", "volume": volume, "operation": request["operation"]})
            return reply if reply["result"] == "operation" and reply["value"] is not None else False

        retained = wait_for(recorded, "unread operation was never recorded")
        unread.close()
        old = json.loads((startup / "runtime.json").read_text())
        # A pidfd pins this exact disposable process. The runtime PID alone is
        # not authority to kill a process from a previous/reused PID lifetime.
        fd = os.pidfd_open(old["child_pid"])
        try:
            cmdline = Path(f"/proc/{old['child_pid']}/cmdline").read_bytes().split(b"\0")
            assert str(data).encode() in cmdline and b"--expect-volume" in cmdline and str(binary).encode() in cmdline
            signal.pidfd_send_signal(fd, signal.SIGKILL)
        finally:
            os.close(fd)

        def restarted():
            value = json.loads((startup / "runtime.json").read_text())
            return value["child_pid"] != old["child_pid"] and online()

        wait_for(restarted, "supervisor did not restart its killed child with the original volume", 40)
        assert recorded() == retained
        assert rpc({"action": "apply", "request": request})["result"] == "applied"
        assert online()["value"]["sequence"] == 2, "recovery repeated the metadata effect"
        listing = rpc({"action": "list", "volume": volume, "parent": "0" * 32})
        assert [n["name"] for n in listing["value"]["children"]] == ["資料 café 🦀"]
        # An edited desktop entry must preserve the running coordinator/profile.
        original_entry, original_profile = entry.read_text(), profile.read_text()
        entry.write_text(original_entry + "# edited\n")
        refused = subprocess.run(list(map(str, [sys.executable, manager, "remove", name, "--component", "authority"])), env=env, capture_output=True, timeout=40)
        assert refused.returncode != 0 and profile.read_text() == original_profile and online()
        entry.write_text(original_entry)
        control("stop")
        # Missing metadata must not become a new database during retry.
        saved = root / "retained-database.sqlite3"
        (data / "metadata.sqlite3").rename(saved)
        control("start")
        wait_for(lambda: "initialization/restart failed" in (startup / "session.log").read_text(), "missing database startup did not refuse")
        assert not (data / "metadata.sqlite3").exists() and not online()
        control("stop")
        saved.rename(data / "metadata.sqlite3")
        control("start")
        wait_for(online, "restored original database did not restart")
        assert online()["value"]["sequence"] == 2
        control("remove")
        assert not online() and not entry.exists()
        assert profile.read_text() == "# Retain original profile\n" and (profile.stat().st_mode & 0o777) == 0o640
        assert (data / "metadata.sqlite3").exists() and token_file.exists()
        print("PASS: no-systemd pinned coordinator desktop/SSH startup, terminal loss, duplicate starts, killed-child WAL/unknown-operation recovery, missing-database refusal and controlled removal; " + ("HTTPS/private CA" if trust else "loopback HTTP"))
    finally:
        subprocess.run(["tmux", "-L", session, "kill-server"], env=env, capture_output=True, timeout=10)
        if (startup / "startup.json").exists():
            control("stop")
