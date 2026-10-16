#!/usr/bin/env python3
"""Rebuild the documentation's original PNG diagrams (requires Pillow)."""
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

OUT = Path(__file__).resolve().parent.parent / "docs/src/images"
OUT.mkdir(exist_ok=True)
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"
BOLD = "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"
INK = "#182b46"
MUTED = "#52647c"
BLUE = "#176a9c"
GREEN = "#11695a"
AMBER = "#9a6116"


class Diagram:
    def __init__(self, title, subtitle, height=920):
        self.image = Image.new("RGB", (1280, height), "#f6f8fc")
        self.draw = ImageDraw.Draw(self.image)
        self.text(40, 28, title, 30, bold=True)
        self.text(40, 76, subtitle, 19, color=MUTED)

    def text(self, x, y, text, size=20, color=INK, bold=False):
        font = ImageFont.truetype(BOLD if bold else FONT, size)
        self.draw.text((x, y), text, font=font, fill=color, spacing=9)

    def box(self, x, y, w, h, title, lines, color=BLUE, fill="#e9f3fb", planned=False):
        self.draw.rounded_rectangle((x, y, x+w, y+h), 15, fill=fill, outline=color, width=2)
        self.text(x+20, y+15, title, 21, bold=True, color=color)
        self.text(x+20, y+51, "\n".join(lines), 18)
        if planned:
            self.text(x+w-116, y+h-28, "PROPOSED", 13, bold=True, color=AMBER)

    def arrow(self, points, color=BLUE, label=None, position=None):
        self.draw.line(points, fill=color, width=3)
        x, y = points[-1]
        px, py = points[-2]
        import math
        angle = math.atan2(y-py, x-px)
        vertices = [(x, y)] + [(x-13*math.cos(angle+a), y-13*math.sin(angle+a)) for a in [-0.45, 0.45]]
        self.draw.polygon(vertices, fill=color)
        if label:
            self.text(*position, label, 16, color=color)

    def save(self, name):
        self.image.save(OUT / name, optimize=True)


d = Diagram("S3Drive: what runs where", "Working read path in blue  •  Local persistence and coordination roadmap in amber")
d.draw.rounded_rectangle((25, 120, 835, 885), 20, fill="#ffffff", outline="#d5dfe9", width=2)
d.draw.rounded_rectangle((865, 120, 1255, 885), 20, fill="#edf7f4", outline="#bcdcd0", width=2)
d.text(50, 136, "YOUR COMPUTER", 16, bold=True, color=MUTED)
d.text(890, 136, "REMOTE SERVICES", 16, bold=True, color=GREEN)
d.box(50, 180, 330, 115, "Applications", ["Desktop / SSH / tmux", "Ordinary file operations"])
d.box(440, 180, 365, 115, "OS filesystem adapter", ["Linux: FUSE", "Windows: WinFsp"])
d.arrow([(380, 235), (440, 235)])
d.box(440, 340, 365, 125, "Shared Rust engine", ["Aliases, exact keys, handles", "Revision checks and limits"])
d.arrow([(620, 295), (620, 340)])
d.box(50, 340, 330, 125, "Local configuration", ["Bucket / endpoint / prefix", "Credentials stay on this host"])
d.arrow([(380, 400), (440, 400)])
d.box(440, 515, 365, 125, "S3 client + Tokio", ["AWS SDK for Rust", "LIST / HEAD / ranged GET"])
d.arrow([(620, 465), (620, 515)])
d.box(890, 440, 340, 200, "S3-compatible service", ["Wasabi / other providers", "Bucket A: documents/…", "Bucket B: photos/…", "Stores the actual file bytes"], color=GREEN, fill="#dff1e9")
d.arrow([(805, 560), (890, 560)], label="HTTPS", position=(816, 532))
d.box(50, 515, 330, 125, "Startup supervisors", ["Linux: hooks, no systemd", "Windows: Task Scheduler"])
d.arrow([(380, 575), (415, 575), (415, 420), (440, 420)])
d.box(50, 705, 350, 135, "Local SQLite", ["Schema bootstrap exists", "Planned: cache metadata,", "local recovery / upload journal"], color=AMBER, fill="#fff4de")
d.box(440, 705, 365, 125, "Local cache files", ["Planned: downloaded data", "and durable staged generations"], color=AMBER, fill="#fff4de", planned=True)
d.arrow([(620, 640), (620, 705)], color=AMBER)
d.arrow([(440, 766), (400, 766)], color=AMBER)
d.box(890, 705, 340, 125, "Optional Turso service", ["Shared metadata authority", "Separate from local SQLite"], color=AMBER, fill="#fff4de", planned=True)
d.text(50, 850, "State and cache stay outside the mounted filesystem. Current mounts are read-only.", 17, color=MUTED)
d.save("architecture.png")

d = Diagram("A session ends; the mount keeps running", "Linux terminal loss + Windows cross-logon reads verified  •  Workstation reboot checks pending", 790)
d.box(45, 160, 350, 130, "Sign in after reboot", ["Linux desktop or SSH hook", "Windows registered startup"], color=GREEN, fill="#dff1e9")
d.box(470, 160, 350, 130, "Start / reuse one daemon", ["Private per-user ownership", "No dependency on a terminal"])
d.box(895, 160, 340, 130, "Stable filesystem path", ["Linux: ~/S3Drive", "Windows: shared NTFS folder"])
d.arrow([(395, 225), (470, 225)])
d.arrow([(820, 225), (895, 225)])
d.box(45, 370, 350, 120, "SSH disconnects", ["The terminal connection ends", "tmux work may continue"], color=MUTED, fill="#eef1f6")
d.box(470, 370, 350, 120, "Daemon remains alive", ["Same UID / SID can access it", "Read requests still reach S3"])
d.arrow([(645, 290), (645, 370)])
d.arrow([(395, 430), (470, 430)])
d.box(895, 370, 340, 120, "Reconnect", ["New SSH / desktop session", "Uses the existing mount"])
d.arrow([(820, 430), (895, 430)])
d.box(45, 570, 1190, 145, "Explicit boundaries", ["Reboot destroys processes and open handles; login recreates the mount, not the old handles.", "MX Linux needs no systemd. A machine-wide policy that kills user processes needs a service backend.", "Windows uses an NTFS folder across logons. Windows 10/11 desktop checks remain pending."], color=AMBER, fill="#fff4de")
d.save("startup-lifecycle.png")

d = Diagram("Turso as a shared metadata authority", "Proposed managed mode  •  Not implemented or enabled  •  File bytes remain in S3", 870)
d.box(35, 170, 335, 145, "Computer A", ["Rust drive process", "Local SQLite + cache", "Local staging / recovery"])
d.box(910, 170, 335, 145, "Computer B", ["Rust drive process", "Local SQLite + cache", "Local staging / recovery"])
d.box(445, 160, 390, 150, "Coordination API", ["Authenticated operations", "Revision CAS + fencing", "Idempotency / reconciliation"], color=AMBER, fill="#fff4de", planned=True)
d.arrow([(370, 230), (445, 230)], color=AMBER)
d.arrow([(910, 230), (835, 230)], color=AMBER)
d.box(445, 370, 390, 160, "Turso Cloud", ["One authoritative metadata DB", "Remote transactions decide", "which generation is published"], color=GREEN, fill="#dff1e9")
d.arrow([(640, 310), (640, 370)], color=AMBER)
d.box(445, 610, 390, 140, "S3 / Wasabi buckets", ["Immutable object generations", "No database/S3 atomic commit"], color=GREEN, fill="#dff1e9")
d.arrow([(195, 315), (195, 680), (445, 680)], label="File bytes\nupload / read", position=(45, 550))
d.arrow([(1075, 315), (1075, 680), (835, 680)], label="File bytes\nupload / read", position=(1100, 550))
d.arrow([(640, 530), (640, 610)], color=AMBER, label="References\nobject keys", position=(660, 548))
d.text(45, 796, "Offline sync is not the lock authority. A lease alone cannot fence an ordinary S3 overwrite.", 20, bold=True, color=AMBER)
d.save("turso-coordination.png")
