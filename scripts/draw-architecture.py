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


d = Diagram("S3Drive: what runs where", "Working read/cache path in blue  •  Bulk files, writes and coordination roadmap in amber")
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
d.box(50, 705, 350, 135, "Local SQLite + owner", ["One bounded async queue", "Clean cache / staged journal", "Write integration pending"], color=BLUE, fill="#e9f3fb")
d.box(440, 705, 365, 125, "Future bulk files", ["Optional payload backend", "Separate durability proof"], color=AMBER, fill="#fff4de", planned=True)
d.arrow([(620, 640), (620, 705)], color=AMBER)
d.arrow([(440, 615), (420, 615), (420, 675), (225, 675), (225, 705)], color=BLUE, label="Authorized snapshots", position=(65, 649))
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


d = Diagram("Local staged generation publication", "Implemented library boundary  •  No writable mount or S3 mutation enabled", 850)
d.box(45, 145, 350, 135, "1. Builder", ["Target + base + local parent", "Declared logical length", "Unpublished until seal"])
d.box(465, 145, 350, 135, "2. Immutable chunks", ["Bounded SHA-256 payloads", "Explicit sparse zero extents", "Durable append progress"])
d.box(885, 145, 350, 135, "3. Validate seal", ["Complete extent coverage", "Checksums + manifest digest", "Expected local head"])
d.arrow([(395, 210), (465, 210)])
d.arrow([(815, 210), (885, 210)])
d.draw.rounded_rectangle((25, 335, 1255, 575), 20, fill="#edf7f4", outline=GREEN, width=2)
d.text(50, 355, "ONE SQLITE TRANSACTION", 19, color=GREEN, bold=True)
d.box(50, 400, 350, 135, "Retain + seal", ["Protect referenced chunks", "Freeze the manifest digest", "Never evict dirty payload"])
d.box(465, 400, 350, 135, "Pending journal", ["Generation-specific intent", "Original remote condition", "Upload execution still pending"])
d.box(885, 400, 345, 135, "Publish local head", ["Reject competing builders", "Keep older sealed snapshots", "COMMIT before receipt"])
d.arrow([(1060, 280), (1060, 335)], color=GREEN)
d.arrow([(400, 467), (465, 467)])
d.arrow([(815, 467), (885, 467)])
d.box(45, 640, 350, 130, "Receipt", ["One locally retained generation", "Replay without rewinding head"], color=GREEN, fill="#dff1e9")
d.box(465, 640, 350, 130, "Failure / crash", ["Roll back partial publication", "Preserve prior committed bytes"], color=GREEN, fill="#dff1e9")
d.box(885, 640, 350, 130, "Later phases", ["Writable mount callbacks", "Conditional S3 commit / flush"], color=AMBER, fill="#fff4de", planned=True)
d.arrow([(220, 575), (220, 640)], color=GREEN)
d.arrow([(640, 575), (640, 640)], color=GREEN)
d.save("staged-generations.png")

d = Diagram("One owner for local state", "Shared cache / journal / edit libraries implemented  •  Mounted writes still pending", 900)
d.box(45, 150, 350, 130, "Async callers", ["Mounted cache reads", "Staging library commands", "Bound inputs before admission"])
d.box(465, 150, 350, 130, "Shared queue", ["Eight accepted commands", "One executing command", "30-second caller deadline"])
d.box(885, 150, 350, 130, "Dedicated OS thread", ["One SQLite connection", "One directory ownership lock", "SQL + checksum work"])
d.arrow([(395, 215), (465, 215)])
d.arrow([(815, 215), (885, 215)])
d.draw.rounded_rectangle((25, 340, 1255, 570), 20, fill="#edf7f4", outline=GREEN, width=2)
d.text(50, 355, "SAME LOCAL DATABASE", 19, color=GREEN, bold=True)
d.box(50, 400, 350, 125, "Clean cache", ["Evict clean unpinned data", "Bounded immutable buffers"])
d.box(465, 400, 350, 125, "Staged generations", ["Retain dirty payloads", "Guarded append / seal"])
d.box(885, 400, 345, 125, "Local recovery", ["Heads + pending intent", "Reopen and reconcile"])
d.arrow([(1060, 280), (1060, 340)], color=GREEN)
d.box(45, 635, 350, 150, "S3 networking", ["Pinned hydration outside SQL", "No transaction waits on S3", "Remote upload still later"])
d.box(465, 635, 350, 150, "Missing reply", ["Outcome may be committed", "Inspect durable progress", "Replay guarded operation"], color=AMBER, fill="#fff4de")
d.box(885, 635, 350, 150, "Orderly close", ["Stop callers / callbacks", "Drain earlier commands", "Release DB + lock, then ack"])
d.text(45, 834, "Local receipts do not acknowledge a cloud write. Current filesystem mounts remain read-only.", 19, color=MUTED)
d.save("state-worker.png")

d = Diagram("Editing a file: one immutable result", "Local edit library implemented  •  No S3 mutation or writable mount enabled", 900)
d.box(45, 150, 350, 145, "1. Fixed input", ["Empty / local snapshot", "Or pinned S3 ReadHandle", "Exact key + original revision"])
d.box(465, 150, 350, 145, "2. Immutable edit plan", ["Patch / shrink / grow", "Explicit sparse zero ranges", "Hash full intent for replay"])
d.box(885, 150, 350, 145, "3. Recover one builder", ["Intent key + generation", "Commit mapping atomically", "Reuse after a missing reply"])
d.arrow([(395, 220), (465, 220)])
d.arrow([(815, 220), (885, 220)])
d.box(45, 405, 350, 150, "Unchanged S3 bytes", ["Read pinned ranges <=1 MiB", "Credentials / revision fixed", "Runs outside SQLite"])
d.box(465, 405, 350, 150, "4. Fill local generation", ["Verify inherited local pieces", "Append patch / explicit zeros", "Guarded cursor replay"])
d.box(885, 405, 350, 150, "5. Seal local snapshot", ["Validate complete manifest", "Compare expected local head", "Read your committed edits"], color=GREEN, fill="#dff1e9")
d.arrow([(1060, 295), (1060, 350), (640, 350), (640, 405)])
d.arrow([(395, 480), (465, 480)])
d.arrow([(815, 480), (885, 480)], color=GREEN)
d.box(45, 655, 1190, 145, "Failure preserves the prior snapshot and the unpublished builder", ["Changed / short / stalled remote data fails; missing bytes are never silently replaced with zeros.", "Identical plans recover one generation. Competing plans report a local-head conflict.", "The new snapshot is local. Conditional remote publication and mounted writes are later phases."], color=AMBER, fill="#fff4de")
d.text(45, 836, "Example: abcdef  →  patch XY at offset 2  →  abXYef  →  truncate to 3  →  abX", 21, bold=True)
d.save("file-edits.png")

d = Diagram("One upload attempt; one durable send claim", "Implemented local journal in blue  •  Network executor still planned in amber", 990)
d.box(40, 150, 355, 140, "Sealed local generation", ["Exact binding + Unicode key", "Manifest, size and full SHA-256", "Dirty bytes stay in SQLite"])
d.box(460, 150, 360, 140, "Prepared intent", ["Random operation ID", "Absent-key / ETag guard", "No permission to send yet"])
d.arrow([(395, 220), (460, 220)])
d.box(460, 355, 360, 145, "Commit claim in SQLite", ["Retain the attempt identity", "Record in-flight state", "Return one SendPermit"])
d.arrow([(640, 290), (640, 355)])
d.box(890, 355, 345, 145, "Future S3 client", ["Consume permit once", "One conditional PUT", "Wasabi / other S3 service"], color=AMBER, fill="#fff4de", planned=True)
d.arrow([(820, 425), (890, 425)], color=AMBER)
d.box(40, 565, 355, 145, "Uncertain result", ["Lost reply / timeout / crash", "Keep local bytes + identity", "Absence never permits retry"])
d.box(460, 565, 360, 145, "Verified receipt", ["Exact operation + full bytes", "Store remote ETag / revision", "Preserve newer local edits"])
d.arrow([(460, 460), (215, 460), (215, 565)])
d.arrow([(215, 710), (215, 755), (640, 755), (640, 710)], label="Matching evidence", position=(345, 726))
d.arrow([(1060, 500), (1060, 637), (820, 637)], color=AMBER, label="Trusted observation", position=(865, 660))
d.box(890, 150, 345, 140, "Next local descendant", ["Guard uses verified ETag", "Receipt anchors its ancestry", "Separate operation identity"])
d.arrow([(820, 600), (850, 600), (850, 220), (890, 220)])
d.box(40, 790, 1195, 145, "Current boundary", ["Local recovery only: no upload executor, remote flush, writable mount or payload reclamation.", "A claim reply can be lost before any send; recovery remains conservative and never reissues it.", "Local serialization does not coordinate multiple machines. Optional Turso authority remains proposed."], color=AMBER, fill="#fff4de")
d.save("remote-journal.png")
