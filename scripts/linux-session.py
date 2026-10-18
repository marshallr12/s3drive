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


def location(name, component="mount", authority_directory=None):
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{0,47}", name):
        raise ValueError("Name must contain 1-48 lowercase letters, digits or hyphens")
    installed = Path(__file__).resolve()
    if installed.name == "manager.py" and installed.parent.name == name:
        return private_directory(installed.parent)
    base = Path(os.environ.get("XDG_STATE_HOME", Path.home() / ".local/state"))
    if not base.is_absolute():
        raise ValueError("XDG_STATE_HOME must be absolute")
    namespace = "s3drive-authority" if component == "authority" else "s3drive"
    if authority_directory is not None:
        data = authority_directory.resolve(strict=True)
        candidate = (base / namespace / name).resolve()
        if candidate.is_relative_to(data) or data.is_relative_to(candidate):
            raise ValueError("Authority database and startup registration must be separate trees")
    return private_directory(private_directory(base / namespace) / name)


def load(root, component=None):
    value = json.loads((root / "startup.json").read_text())
    kind = "mount" if value.get("version") == 1 else value.get("component")
    if value.get("version") == 2 and kind == "authority":
        expected = {"version", "component", "executable", "authority", "autostart", "autostart_text", "profile", "profile_block"}
        fields = {"directory", "volume", "name_policy", "token_file", "listen", "tls_cert", "tls_key"}
        authority = value.get("authority")
        if set(value) != expected or not isinstance(authority, dict) or set(authority) != fields:
            raise ValueError("Unsupported authority registration fields")
        if any(not isinstance(authority[k], str) or not authority[k] for k in fields - {"tls_cert", "tls_key"}):
            raise ValueError("Incomplete pinned authority registration")
        if any(authority[k] is not None and not isinstance(authority[k], str) for k in ["tls_cert", "tls_key"]):
            raise ValueError("Invalid authority TLS registration")
        if bool(authority["tls_cert"]) != bool(authority["tls_key"]):
            raise ValueError("Incomplete authority TLS pair")
    elif value.get("version") != 1:
        raise ValueError("Unsupported startup registration")
    if component is not None and kind != component:
        raise ValueError("Startup component does not match its registration")
    return value


def child_command(value, check=False):
    if value.get("version") == 1:
        return [value["executable"], "mount", value["config"], value["mountpoint"]]
    authority = value["authority"]
    command = [value["executable"], "authority-check" if check else "authority-serve",
               "--directory", authority["directory"], "--expect-volume", authority["volume"],
               "--name-policy", authority["name_policy"], "--token-file", authority["token_file"],
               "--listen", authority["listen"]]
    if authority["tls_cert"]:
        command += ["--tls-cert", authority["tls_cert"], "--tls-key", authority["tls_key"]]
    return command


def record_child(root, value, child):
    # Operational hints only, never authority to kill a PID from another run.
    fd, path = tempfile.mkstemp(prefix=".runtime-", dir=root)
    temporary = Path(path)
    try:
        with os.fdopen(fd, "w") as out:
            runtime = dict(supervisor_pid=os.getpid(), child_pid=child.pid,
                           component=value.get("component", "mount"))
            if runtime["component"] == "authority":
                runtime["expected_volume"] = value["authority"]["volume"]
            json.dump(runtime, out)
            out.flush()
            os.fsync(out.fileno())
        temporary.replace(root / "runtime.json")
    finally:
        temporary.unlink(missing_ok=True)


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
        output.flush()
        os.fsync(output.fileno())


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
    authority = None
    config = None
    if args.component == "authority":
        mount = args.authority_directory.absolute()
        authority = dict(directory=str(mount), volume=args.authority_volume,
                         name_policy=args.authority_name_policy, token_file=str(args.authority_token_file.absolute()),
                         listen=args.authority_listen,
                         tls_cert=str(args.tls_cert.absolute()) if args.tls_cert else None,
                         tls_key=str(args.tls_key.absolute()) if args.tls_key else None)
        # Rust validates private paths/ACLs, the existing volume and schema
        # without initialization or migration. No S3 credentials are involved.
        subprocess.run(child_command(dict(version=2, executable=str(executable), authority=authority), True), check=True, timeout=30)
        protected = [executable, root.resolve(), Path(authority["token_file"])]
        protected += [Path(p) for p in [authority["tls_cert"], authority["tls_key"]] if p]
    else:
        config = args.config.resolve(strict=True)
        mount = args.mountpoint.absolute()
        if mount.is_symlink() or not mount.is_dir() or mount.stat().st_uid != os.getuid():
            raise ValueError("Mountpoint must be an existing owned directory, not a symlink")
        if os.path.ismount(mount) or any(mount.iterdir()):
            raise ValueError("Mountpoint must be empty and unmounted")
        subprocess.run([str(executable), "config-check", str(config)], check=True, timeout=30)
        protected = [executable, config, root.resolve()]
    mount = mount.resolve(strict=True)
    for path in protected:
        if path.resolve().is_relative_to(mount):
            raise ValueError("Executable, private inputs and startup state must be outside the mounted/data directory")
    if authority and mount.is_relative_to(root.resolve()):
        raise ValueError("Authority database and startup registration must be separate trees")
    config_home = Path(os.environ.get("XDG_CONFIG_HOME", Path.home() / ".config"))
    if not config_home.is_absolute():
        raise ValueError("XDG_CONFIG_HOME must be absolute")
    autostart = config_home / "autostart"
    if autostart.resolve().is_relative_to(mount):
        raise ValueError("Autostart state must be outside the mounted/data directory")
    autostart.mkdir(parents=True, exist_ok=True)
    prefix = "s3drive-authority" if authority else "s3drive"
    entry = autostart / f"{prefix}-{args.name}.desktop"
    manager = root / "manager.py"
    command = [sys.executable, str(manager), "start", args.name]
    if authority:
        command += ["--component", "authority"]
    label = f"authority {args.name}" if authority else args.name
    begin = f"# BEGIN S3Drive {label}\n"
    end = f"# END S3Drive {label}\n"
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
        entry_text = "[Desktop Entry]\nType=Application\nName=S3Drive " + label \
            + "\nTerminal=false\nExec=" + " ".join(map(desktop_argument, command)) + "\n"
        create_file(entry, entry_text)
        made.append(entry)
        create_file(root / "login.sh", block)
        made.append(root / "login.sh")
        value = dict(version=1, executable=str(executable), config=str(config), mountpoint=str(mount),
                     autostart=str(entry), autostart_text=entry_text,
                     profile=str(profile) if profile else None, profile_block=profile_block)
        if authority:
            value.update(version=2, component="authority", authority=authority)
            del value["config"], value["mountpoint"]
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
                child = subprocess.Popen(child_command(value),
                                         stdin=subprocess.DEVNULL, close_fds=True)
                record_child(root, value, child)
                while child.poll() is None:
                    if stopped or (root / "stop").exists():
                        child.terminate()
                        # Do not SIGKILL an unmount held by open descriptors.
                        child.wait()
                        return
                    time.sleep(0.2)
                if child.returncode == 0:
                    return
                print(f"{value.get('component', 'mount')} exited {child.returncode}; attempt {attempt + 1}/4", flush=True)
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
    value = load(root)
    if running(root):
        print("Supervisor already running")
        return
    (root / "stop").unlink(missing_ok=True)
    logfile = root / "session.log"
    if logfile.exists() and logfile.stat().st_size > 1024 * 1024:
        logfile.replace(root / "session.previous.log")
    fd = os.open(logfile, os.O_CREAT | os.O_APPEND | os.O_WRONLY | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "a") as output:
        command = [sys.executable, str(root / "manager.py"), "run", name]
        if value.get("component") == "authority":
            command += ["--component", "authority"]
        subprocess.Popen(command, cwd="/",
                         stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                         close_fds=True, start_new_session=True)
    print("Detached supervisor started; use status and an authenticated operation to check readiness")


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
    for name in ["startup.json", "manager.py", "login.sh", "stop", "runtime.json"]:
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
    parser.add_argument("--component", choices=["mount", "authority"], default="mount")
    parser.add_argument("--authority-directory", type=Path)
    parser.add_argument("--authority-volume")
    parser.add_argument("--authority-name-policy", choices=["exact-utf8-v1", "portable-collision-v1"])
    parser.add_argument("--authority-token-file", type=Path)
    parser.add_argument("--authority-listen")
    parser.add_argument("--tls-cert", type=Path)
    parser.add_argument("--tls-key", type=Path)
    args = parser.parse_args()
    if sys.platform != "linux" or os.getuid() == 0:
        parser.error("Run as the ordinary mounting user on Linux")
    os.umask(0o077)
    if args.operation == "install":
        authority_options = [args.authority_directory, args.authority_volume, args.authority_name_policy, args.authority_token_file, args.authority_listen]
        if args.component == "authority":
            if not all([args.executable, *authority_options]) or args.config or args.mountpoint or bool(args.tls_cert) != bool(args.tls_key):
                parser.error("authority install requires executable, all authority options, no mount options and a complete optional TLS pair")
        elif not all([args.executable, args.config, args.mountpoint]) or any(authority_options + [args.tls_cert, args.tls_key]):
            parser.error("install requires --executable, --config and --mountpoint")
    root = location(args.name, args.component, args.authority_directory if args.operation == "install" else None)
    if args.operation == "install":
        install(args, root)
        return
    value = load(root, args.component)
    if args.operation == "run":
        supervise(root)
    elif args.operation == "start":
        start(root, args.name)
    elif args.operation == "stop":
        stop(root)
    elif args.operation == "remove":
        remove(root)
    else:
        status = dict(supervisor_running=running(root), log=str(root / "session.log"))
        if value.get("component") == "authority":
            status.update(component="authority", expected_volume=value["authority"]["volume"],
                          listen=value["authority"]["listen"], provider_access_verified=False)
        else:
            status.update(mounted=os.path.ismount(value["mountpoint"]), mountpoint=value["mountpoint"])
        print(json.dumps(status))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
