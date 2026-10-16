#!/usr/bin/env python3
"""Per-user Linux startup without systemd. No credentials are copied or stored."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import shlex
import signal
import stat
import subprocess
import sys
import tempfile
import time


def private_directory(path):
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = path.lstat()
    if path.is_symlink() or not path.is_dir() or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError(f"Expected an owned private directory: {path}")
    return path


def location(name):
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,47}", name):
        raise ValueError("Name must contain 1-48 lowercase letters, digits or hyphens")
    installed = Path(__file__).resolve()
    if installed.name == "manager.py" and installed.parent.name == name:
        return private_directory(installed.parent)
    base = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    if not base.is_absolute():
        raise ValueError("XDG_STATE_HOME must be absolute")
    return private_directory(private_directory(base / "s3drive") / name)


def load(root):
    value = json.loads((root / "startup.json").read_text())
    if value.get("version") != 1:
        raise ValueError("Unsupported startup registration")
    return value


def lock(root):
    fd = os.open(root / "supervisor.lock", os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    stream = os.fdopen(fd, "w")
    try:
        fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        stream.close()
        return None
    return stream


def running(root):
    handle = lock(root)
    if handle is None:
        return True
    handle.close()
    return False


def create_file(path, text, mode=0o600):
    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY | os.O_NOFOLLOW, mode)
    with os.fdopen(fd, "w") as output:
        output.write(text)


def profile_text(path):
    if not path.exists() and not path.is_symlink():
        return ""
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError("Shell profile must be an owned regular file, not a symlink")
    return path.read_text()


def replace_profile(path, expected, replacement):
    # Prepare the complete new profile before replacing it; preserve its mode
    # and refuse a detected concurrent edit instead of truncating user settings.
    if profile_text(path) != expected:
        raise ValueError("Shell profile changed during registration; retry")
    mode = stat.S_IMODE(path.stat().st_mode) if path.exists() else 0o600
    fd, temporary = tempfile.mkstemp(prefix=".s3drive-profile-", dir=path.parent)
    temporary = Path(temporary)
    try:
        with os.fdopen(fd, "w") as stream:
            os.fchmod(stream.fileno(), mode)
            stream.write(replacement)
            stream.flush()
            os.fsync(stream.fileno())
        if profile_text(path) != expected:
            raise ValueError("Shell profile changed during registration; retry")
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def desktop_argument(value):
    # Desktop Entry string escaping is applied before Exec argument escaping.
    if any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError("Startup paths cannot contain control characters")
    value = value.replace("%", "%%")
    for char in ['\\', '"', '`', '$']:
        value = value.replace(char, '\\' + char)
    return '"' + value.replace('\\', '\\\\') + '"'


def install(args, root):
    if (root / "startup.json").exists():
        raise ValueError("Registration already exists; remove it before replacing it")
    executable = args.executable.resolve(strict=True)
    config = args.config.resolve(strict=True)
    mount = args.mountpoint.absolute()
    if mount.is_symlink() or not mount.is_dir() or mount.stat().st_uid != os.getuid():
        raise ValueError("Mountpoint must be an existing owned directory, not a symlink")
    mount = mount.resolve()
    if os.path.ismount(mount) or any(mount.iterdir()):
        raise ValueError("Mountpoint must be empty and unmounted")
    for path in [executable, config, root.resolve()]:
        if path.is_relative_to(mount):
            raise ValueError("Executable, configuration and startup state must be outside the mount")
    subprocess.run([str(executable), "config-check", str(config)], check=True, timeout=30)
    config_home = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    if not config_home.is_absolute():
        raise ValueError("XDG_CONFIG_HOME must be absolute")
    autostart = config_home / "autostart"
    autostart.mkdir(parents=True, exist_ok=True)
    entry = autostart / f"s3drive-{args.name}.desktop"
    manager = root / "manager.py"
    command = [sys.executable, str(manager), "start", args.name]
    begin = f"# BEGIN S3Drive {args.name}\n"
    end = f"# END S3Drive {args.name}\n"
    block = begin + shlex.join(command) + " >/dev/null 2>&1 || :\n" + end
    # Explicit profile selection avoids guessing bash/zsh startup precedence.
    profile = args.shell_profile.expanduser().absolute() if args.shell_profile else None
    original_profile = profile_text(profile) if profile else None
    if profile and begin in original_profile:
        raise ValueError("Shell profile already contains this registration")
    for path in [entry, profile, Path(sys.executable)]:
        if path and path.resolve().is_relative_to(mount):
            raise ValueError("Startup entry, login profile and Python must be outside the mount")
    profile_block = "\n" + block
    made = []
    try:
        create_file(manager, Path(__file__).read_text())
        made.append(manager)
        entry_text = "[Desktop Entry]\nType=Application\nName=S3Drive " + args.name \
            + "\nTerminal=false\nExec=" + " ".join(map(desktop_argument, command)) + "\n"
        create_file(entry, entry_text)
        made.append(entry)
        create_file(root / "login.sh", block)
        made.append(root / "login.sh")
        value = dict(version=1, executable=str(executable), config=str(config), mountpoint=str(mount),
                     autostart=str(entry), autostart_text=entry_text,
                     profile=str(profile) if profile else None, profile_block=profile_block)
        create_file(root / "startup.json", json.dumps(value, indent=2) + "\n")
        made.append(root / "startup.json")
        if profile:
            replace_profile(profile, original_profile, original_profile + profile_block)
    except Exception:
        for path in reversed(made):
            path.unlink(missing_ok=True)
        raise
    print("Installed desktop startup. SSH login hook:", root / "login.sh")
    if not profile:
        print("Source that hook from your actual login profile to start on an SSH-only login.")


def supervise(root):
    handle = lock(root)
    if handle is None:
        return
    with handle:
        value = load(root)
        stopped = False

        def stop_signal(_number, _frame):
            nonlocal stopped
            stopped = True

        signal.signal(signal.SIGHUP, signal.SIG_IGN)
        signal.signal(signal.SIGTERM, stop_signal)
        signal.signal(signal.SIGINT, stop_signal)
        child = None
        try:
            # Bounded retries; never forcibly detach an unknown/stale mount.
            for attempt in range(4):
                if stopped or (root / "stop").exists():
                    break
                child = subprocess.Popen([value["executable"], "mount", value["config"], value["mountpoint"]],
                                         stdin=subprocess.DEVNULL, close_fds=True)
                while child.poll() is None:
                    if stopped or (root / "stop").exists():
                        child.terminate()
                        # Do not SIGKILL an unmount held by open descriptors.
                        child.wait()
                        return
                    time.sleep(0.2)
                if child.returncode == 0:
                    return
                print(f"Mount exited {child.returncode}; attempt {attempt + 1}/4", flush=True)
                for _ in range(50):
                    if stopped or (root / "stop").exists():
                        return
                    time.sleep(0.2)
        finally:
            if child is not None and child.poll() is None:
                child.terminate()
                child.wait()
            (root / "stop").unlink(missing_ok=True)


def start(root, name):
    load(root)
    if running(root):
        print("Supervisor already running")
        return
    (root / "stop").unlink(missing_ok=True)
    logfile = root / "session.log"
    if logfile.exists() and logfile.stat().st_size > 1024 * 1024:
        logfile.replace(root / "session.previous.log")
    fd = os.open(logfile, os.O_CREAT | os.O_APPEND | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a") as output:
        subprocess.Popen([sys.executable, str(root / "manager.py"), "run", name], cwd="/",
                         stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                         close_fds=True, start_new_session=True)
    print("Detached supervisor started; use status to check the mount")


def stop(root):
    if not running(root):
        return
    (root / "stop").touch(mode=0o600)
    until = time.monotonic() + 40
    while running(root):
        if time.monotonic() > until:
            raise RuntimeError("Unmount still pending; close open files and retry status/stop")
        time.sleep(0.1)


def remove(root):
    value = load(root)
    # Validate every managed entry before modifying any of them. An edited
    # desktop entry must not leave a half-removed shell registration.
    if value["profile"]:
        profile = Path(value["profile"])
        original = profile_text(profile)
        if original.count(value["profile_block"]) != 1:
            raise ValueError("Login hook was edited; restore the registered block before retrying")
    entry = Path(value["autostart"])
    if entry.exists():
        if entry.is_symlink() or value["autostart_text"] != entry.read_text():
            raise ValueError("Autostart entry was edited; refusing to remove it")
    stop(root)
    if value["profile"]:
        replace_profile(profile, original, original.replace(value["profile_block"], "", 1))
    if entry.exists():
        entry.unlink()
    for name in ["startup.json", "manager.py", "login.sh", "stop"]:
        (root / name).unlink(missing_ok=True)
    print("Startup removed; mountpoint, configuration, credentials and logs retained")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=["install", "start", "stop", "status", "remove", "run"])
    parser.add_argument("name")
    parser.add_argument("--executable", type=Path)
    parser.add_argument("--config", type=Path)
    parser.add_argument("--mountpoint", type=Path)
    parser.add_argument("--shell-profile", type=Path)
    args = parser.parse_args()
    if sys.platform != "linux" or os.getuid() == 0:
        parser.error("Run as the ordinary mounting user on Linux")
    os.umask(0o077)
    root = location(args.name)
    if args.operation == "install":
        if not all([args.executable, args.config, args.mountpoint]):
            parser.error("install requires --executable, --config and --mountpoint")
        install(args, root)
    elif args.operation == "run":
        supervise(root)
    elif args.operation == "start":
        start(root, args.name)
    elif args.operation == "stop":
        stop(root)
    elif args.operation == "remove":
        remove(root)
    else:
        value = load(root)
        print(json.dumps(dict(supervisor_running=running(root), mounted=os.path.ismount(value["mountpoint"]),
                              mountpoint=value["mountpoint"], log=str(root / "session.log"))))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
