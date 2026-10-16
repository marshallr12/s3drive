#!/usr/bin/env python3
"""Explicit native startup gate: requires FUSE, tmux and gio; never skips."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import time
import uuid

repo = Path(__file__).resolve().parent.parent
binary = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else repo / "target/debug/s3drive"
manager = repo / "scripts/linux-session.py"
name = "acceptance"
session = "s3drive-" + uuid.uuid4().hex[:12]


def run(*args, **kwargs):
    return subprocess.run(list(map(str, args)), check=True, timeout=50, **kwargs)


def wait_for(test, message):
    deadline = time.monotonic() + 20
    while not test():
        if time.monotonic() > deadline:
            raise AssertionError(message)
        time.sleep(0.1)


with tempfile.TemporaryDirectory(prefix='s3drive startup café $ % ') as directory:
    root = Path(directory)
    env = dict(os.environ, XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config"))
    mount = root / "mount"
    mount.mkdir()
    config = root / "drive.toml"
    config.write_text("""version=1
mode='read-only'
[[sources]]
alias='docs'
bucket='test-bucket'
region='us-east-1'
endpoint='http://127.0.0.1:1'
allow_http_loopback=true
path_style=true
profile='nonexistent-startup-test'
""")
    profile = root / "profile"
    profile.write_text("# Existing user settings must survive\n")
    profile.chmod(0o640)
    run(sys.executable, manager, "install", name, "--executable", binary,
        "--config", config, "--mountpoint", mount, "--shell-profile", profile, env=env)
    try:
        registered = root / "state/s3drive" / name
        assert (registered.stat().st_mode & 0o777) == 0o700
        # Real Desktop Entry parser, including dollar, percent, space and Unicode.
        run("gio", "launch", root / f"config/autostart/s3drive-{name}.desktop", env=env)
        # Race startup from the desktop and several other sessions.
        children = [subprocess.Popen([sys.executable, str(manager), "start", name], env=env) for _ in range(5)]
        for child in children:
            assert child.wait(timeout=10) == 0
        wait_for(lambda: os.path.ismount(mount), "desktop startup did not mount")
        assert sorted(p.name for p in mount.iterdir()) == ["docs"]
        run(sys.executable, manager, "stop", name, env=env)
        assert not os.path.ismount(mount)
        # Start from a real terminal multiplexer, then destroy that terminal.
        # SSH need not inherit the desktop's XDG variables or working directory.
        login_env = {key: value for key, value in env.items()
                     if key not in ["XDG_STATE_HOME", "XDG_CONFIG_HOME"]}
        command = shlex.join(["sh", str(profile)]) + "; exec sleep 600"
        run("tmux", "-L", session, "new-session", "-d", "-s", session, command,
            env=login_env, cwd="/")
        wait_for(lambda: os.path.ismount(mount), "SSH login hook did not mount")
        run("tmux", "-L", session, "kill-server", env=env)
        run("python3", "-c", "import os,sys; assert os.listdir(sys.argv[1]) == ['docs']", mount, env=env)
        state = json.loads(run(sys.executable, manager, "status", name, env=env, capture_output=True).stdout)
        assert state["supervisor_running"] and state["mounted"]
        entry = root / f"config/autostart/s3drive-{name}.desktop"
        original_entry = entry.read_text()
        original_profile = profile.read_text()
        entry.write_text(original_entry + "# User edited\n")
        refused = subprocess.run([sys.executable, str(manager), "remove", name], env=env,
                                 capture_output=True, timeout=50)
        assert refused.returncode != 0 and b"edited" in refused.stderr
        assert profile.read_text() == original_profile and os.path.ismount(mount)
        entry.write_text(original_entry)
        run(sys.executable, manager, "remove", name, env=env)
        assert not os.path.ismount(mount)
        assert not (root / f"config/autostart/s3drive-{name}.desktop").exists()
        assert profile.read_text() == "# Existing user settings must survive\n"
        assert (profile.stat().st_mode & 0o777) == 0o640
        assert config.exists() and mount.is_dir()
        print("PASS: no-systemd desktop/SSH startup, duplicate starts, terminal loss, status and removal")
    finally:
        subprocess.run(["tmux", "-L", session, "kill-server"], env=env, capture_output=True, timeout=10)
        subprocess.run([sys.executable, str(manager), "stop", name], env=env, timeout=50)
        if os.path.ismount(mount):
            raise RuntimeError("Startup acceptance left a live mount; preserve its directory")
