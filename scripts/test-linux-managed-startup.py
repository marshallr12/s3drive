#!/usr/bin/env python3
"""Owned paired native fixture controller. Rust owns the live S3 fixture.

Start this process before opening FUSE file handles. All later process control
arrives over stdin so the native test never forks with those descriptors open.
Only disposable XDG/profile paths from the prepared manifest are touched.
"""
import json
import os
from pathlib import Path
import shlex
import signal
import ssl
import subprocess
import sys
import time
from urllib.parse import urlsplit
from urllib.request import HTTPSHandler, ProxyHandler, Request, build_opener
import uuid


def run(*args, **kwargs):
    result = subprocess.run(list(map(str, args)), capture_output=True, timeout=50, **kwargs)
    if result.returncode:
        raise RuntimeError(result.stderr.decode(errors="replace") + result.stdout.decode(errors="replace"))
    return result


def wait_for(predicate, message, seconds=40):
    until = time.monotonic() + seconds
    while True:
        value = predicate()
        if value:
            return value
        if time.monotonic() >= until:
            raise AssertionError(message)
        time.sleep(0.1)


def main():
    if sys.platform != "linux" or os.getuid() == 0:
        raise RuntimeError("Paired native startup requires an ordinary Linux account")
    manifest = json.loads(Path(sys.argv[1]).read_text())
    root = Path(manifest["root"])
    manager = Path(__file__).resolve().parent / "linux-session.py"
    binary = Path(manifest["executable"])
    mount = Path(manifest["mount"])
    state = root / "paired-startup-state"
    config = root / "paired-startup-config"
    profile = root / "paired-profile"
    original_profile = "# Owned paired native fixture\n"
    profile.write_text(original_profile)
    profile.chmod(0o640)
    env = dict(os.environ, XDG_STATE_HOME=str(state), XDG_CONFIG_HOME=str(config))
    session = "s3drive-paired-" + uuid.uuid4().hex[:12]
    auth_root = state / "s3drive-authority/paired"
    mount_root = state / "s3drive/paired"
    address = urlsplit(manifest["endpoint"])
    trust = ssl.create_default_context(cafile=manifest["cert"])
    http = build_opener(ProxyHandler({}), HTTPSHandler(context=trust))
    token = Path(manifest["token"]).read_text().strip()

    def online():
        try:
            request = Request(manifest["endpoint"], data=b'{"action":"info"}',
                              headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
            with http.open(request, timeout=2) as response:
                reply = json.load(response)
            return reply if reply.get("result") == "info" and reply["value"]["id"] == manifest["volume"] else False
        except (OSError, ValueError):
            return False

    def control(action, component):
        return run(sys.executable, manager, action, "paired", "--component", component, env=env)

    def runtime(component):
        directory = auth_root if component == "authority" else mount_root
        return json.loads((directory / "runtime.json").read_text())

    def kill_owned(component):
        value = runtime(component)
        pid = value["child_pid"]
        fd = os.pidfd_open(pid)
        try:
            argv = Path(f"/proc/{pid}/cmdline").read_bytes().split(b"\0")
            assert str(binary).encode() in argv
            if component == "authority":
                assert b"authority-serve" in argv and b"--expect-volume" in argv
                assert manifest["volume"].encode() in argv and manifest["database"].encode() in argv
            else:
                assert b"mount" in argv and manifest["drive_config"].encode() in argv and str(mount).encode() in argv
                # Prevent the fixture supervisor from remounting while the test
                # inspects/reopens its durable journal. This is failure-injection
                # control of our own registration, never production stop policy.
                (mount_root / "stop").touch(mode=0o600)
            signal.pidfd_send_signal(fd, signal.SIGKILL)
        finally:
            os.close(fd)
        if component == "mount":
            control("stop", "mount")
            run("fusermount3", "-uz", mount, env=env)
            assert not os.path.ismount(mount)
            assert online(), "stopping the drive stopped its separate coordinator"
        return value

    try:
        run(sys.executable, manager, "install", "paired", "--component", "authority", "--executable", binary,
            "--authority-directory", manifest["database"], "--authority-volume", manifest["volume"],
            "--authority-name-policy", manifest["policy"], "--authority-token-file", manifest["token"],
            "--authority-listen", f"{address.hostname}:{address.port}", "--tls-cert", manifest["cert"],
            "--tls-key", manifest["key"], "--shell-profile", profile, env=env)
        mount.mkdir(exist_ok=True)
        run(sys.executable, manager, "install", "paired", "--executable", binary,
            "--config", manifest["drive_config"], "--mountpoint", mount, "--shell-profile", profile, env=env)
        # Exercise a real desktop entry, then the independent shell hooks. The
        # profile captures its fixture XDG paths; a login need not inherit them.
        run("gio", "launch", config / "autostart/s3drive-authority-paired.desktop", env=env)
        wait_for(online, "desktop did not start original HTTPS coordinator")
        login_env = {k: v for k, v in env.items() if k not in ["XDG_STATE_HOME", "XDG_CONFIG_HOME"]}
        command = shlex.join(["sh", str(profile)]) + "; exec sleep 600"
        run("tmux", "-L", session, "new-session", "-d", "-s", session, command, env=login_env, cwd="/")
        wait_for(lambda: os.path.ismount(mount), "login hook did not create actual native mount")
        wait_for(lambda: all((directory / "runtime.json").exists() for directory in [auth_root, mount_root]),
                 "supervisors did not record their owned fixture children")
        run("tmux", "-L", session, "kill-server", env=env)
        before = {c: runtime(c) for c in ["mount", "authority"]}
        for component in before:
            control("start", component)
        after = {c: runtime(c) for c in before}
        assert before == after and online() and os.path.ismount(mount)
        print(json.dumps({"event": "ready", "volume": manifest["volume"]}), flush=True)
        for line in sys.stdin:
            command = line.strip()
            if command == "crash-authority":
                old = kill_owned("authority")
                wait_for(lambda: not online(), "killed coordinator remained online", 3)
                result = {"event": "authority-down", "old_child": old["child_pid"]}
            elif command == "recover-authority":
                wait_for(lambda: runtime("authority")["child_pid"] != old["child_pid"] and online(),
                         "coordinator supervisor did not recover original volume")
                result = {"event": "authority-recovered", "volume": manifest["volume"]}
            elif command == "crash-mount-stop":
                kill_owned("mount")
                result = {"event": "mount-stopped", "coordinator_online": True}
            elif command == "start-mount":
                control("start", "mount")
                wait_for(lambda: os.path.ismount(mount), "managed drive did not remount")
                assert online()
                result = {"event": "mount-ready"}
            elif command == "remove":
                control("remove", "mount")
                assert online() and not os.path.ismount(mount)
                control("remove", "authority")
                assert not online()
                assert profile.read_text() == original_profile and profile.stat().st_mode & 0o777 == 0o640
                assert Path(manifest["database"], "metadata.sqlite3").exists()
                assert Path(manifest["token"]).exists() and Path(manifest["drive_config"]).exists()
                assert Path(os.environ["AWS_SHARED_CREDENTIALS_FILE"]).exists()
                result = {"event": "removed", "data_preserved": True}
            else:
                raise AssertionError("Unknown owned fixture command")
            print(json.dumps(result), flush=True)
    finally:
        subprocess.run(["tmux", "-L", session, "kill-server"], env=env, capture_output=True, timeout=10)
        failures = []
        for component, directory in [("mount", mount_root), ("authority", auth_root)]:
            if (directory / "startup.json").exists():
                try:
                    # Reaching EOF with registrations still present is failure
                    # cleanup. Kill only our verified fixture mount so a dirty
                    # descriptor cannot hold detached supervisors indefinitely.
                    if component == "mount" and os.path.ismount(mount):
                        if (mount_root / "runtime.json").exists():
                            kill_owned("mount")
                        else:
                            run("fusermount3", "-uz", mount, env=env)
                except Exception as error:
                    failures.append(f"Fixture {component} detach failed: {error}")
                try:
                    control("stop", component)
                except Exception as error:
                    failures.append(f"Fixture {component} shutdown failed: {error}")
        if failures:
            raise RuntimeError("; ".join(failures))


if __name__ == "__main__":
    main()
