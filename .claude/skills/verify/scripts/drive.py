"""Drive the real grind binary through a scenario file and record proof.

Usage: python3 -I drive.py SCENARIO [--size WxH] [--out DIR] [--keep]

Each run gets a private tmux server, a fresh temp HOME, a stub bin dir that
shadows pbcopy and the GUI editors, and no network. Every step writes
NN-label.txt and NN-label.ansi; `snap` steps add NN-label.png. The temp HOME's
.grind-tui is copied to OUT/home before the temp HOME is removed.
"""

import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parents[4]
BINARY = REPO / "target" / "release" / "grind"
NO_NETWORK = "(version 1)(allow default)(deny network*)"
KEY_ALIASES = {"Esc": "Escape", "Backspace": "BSpace", "S-Tab": "BTab", "PgUp": "PPage", "PgDn": "NPage"}

STUBS = {
    "pbcopy": 'cat > "$HOME/clipboard.txt"\n',
    # GUI editors run detached; a real launch would open an app on the host.
    # The stub records the call, then saves an edit like the editor would.
    **{
        name: f'echo "{name} $*" >> "$HOME/launched.txt"\n'
        f'( sleep 1; printf "\\n// saved by {name} stub\\n" >> "$1" ) >/dev/null 2>&1 &\n'
        for name in ("code", "cursor", "zed")
    },
}


class Run:
    def __init__(self, scenario: Path, size: str | None, out: Path | None):
        self.scenario = scenario
        self.steps = parse(scenario.read_text())
        declared = next((args for cmd, args, _ in self.steps if cmd == "size"), "140x40")
        self.width, self.height = map(int, (size or declared).split("x"))
        self.out = out or REPO / "target" / "verify" / f"{scenario.stem}-{self.width}x{self.height}"
        self.sock = f"grind-verify-{os.getpid()}"
        self.home = Path(tempfile.mkdtemp(prefix="grind-verify-home-"))
        self.n = 0
        self.failures: list[str] = []
        self.log: list[str] = []
        self.screen = ""
        self.ansi = ""

    def tmux(self, *args: str, check: bool = True) -> str:
        cmd = ["tmux", "-L", self.sock, "-f", "/dev/null", *args]
        return subprocess.run(cmd, check=check, capture_output=True, text=True).stdout

    def record(self, line: str):
        self.log.append(line)
        print(line, flush=True)

    def check(self, ok: bool, what: str):
        self.record(f"  {'PASS' if ok else 'FAIL'} {what}")
        if not ok:
            self.failures.append(f"step {self.n}: {what}")

    def setup(self):
        if self.out.exists():
            shutil.rmtree(self.out)
        self.out.mkdir(parents=True)
        bin_dir = self.home / "stub-bin"
        bin_dir.mkdir()
        for name, body in STUBS.items():
            path = bin_dir / name
            path.write_text("#!/bin/sh\n" + body)
            path.chmod(0o755)
        (self.home / ".grind-tui").mkdir()
        self.tmux("new-session", "-d", "-s", "boot")
        self.tmux("set", "-g", "terminal-features", ",*:RGB")
        self.tmux("set", "-g", "remain-on-exit", "on")
        self.tmux("set", "-g", "escape-time", "0")

    def launch(self, args: str):
        env = {
            "HOME": str(self.home),
            "PATH": f"{self.home / 'stub-bin'}:{os.environ['PATH']}",
            "TERM": "xterm-256color",
            "COLORTERM": "truecolor",
            "LANG": "en_US.UTF-8",
        }
        inner = " ".join(
            ["env", "-i", *(f"{k}={shlex.quote(v)}" for k, v in env.items()),
             "sandbox-exec", "-p", shlex.quote(NO_NETWORK), shlex.quote(str(BINARY)), args]
        )
        self.tmux("new-session", "-d", "-s", "g", "-x", str(self.width), "-y", str(self.height), inner)
        self.tmux("kill-session", "-t", "boot", check=False)

    def pane_text(self) -> str:
        return self.tmux("capture-pane", "-t", "g", "-p")

    def settle(self, timeout: float = 3.0):
        """Waits until two captures 150ms apart match, so the frame is drawn."""
        deadline = time.time() + timeout
        prev = None
        while time.time() < deadline:
            time.sleep(0.15)
            now = self.tmux("capture-pane", "-t", "g", "-p", "-e")
            if now == prev and self.pane_text().strip():
                return
            prev = now

    def capture(self, label: str, png: bool):
        self.screen = self.pane_text()
        # Per-row capture: a whole-pane capture writes colours relative to the
        # previous line and drops trailing background, which freeze misdraws.
        rows = [
            self.tmux("capture-pane", "-t", "g", "-p", "-e", "-N", "-S", str(r), "-E", str(r)).rstrip("\n")
            for r in range(self.height)
        ]
        self.ansi = "\n".join(rows) + "\n"
        stem = self.out / f"{self.n:02d}-{slug(label)}"
        stem.with_suffix(".txt").write_text(self.screen)
        stem.with_suffix(".ansi").write_text(self.ansi)
        if png:
            with stem.with_suffix(".ansi").open() as src:
                subprocess.run(
                    ["freeze", "--language", "ansi", "--output", str(stem.with_suffix(".png"))],
                    stdin=src, check=True, capture_output=True, timeout=60,
                )
            self.record(f"  snap {stem.with_suffix('.png')}")

    def pane_dead(self) -> tuple[bool, str]:
        out = self.tmux("display", "-p", "-t", "g", "#{pane_dead} #{pane_dead_status}").split()
        return out[0] == "1", (out[1] if len(out) > 1 else "")

    def step(self, cmd: str, args: str, body: str | None):
        self.n += 1
        self.record(f"[{self.n:02d}] {cmd} {args}".rstrip())
        if cmd == "size":
            return
        if cmd == "write":
            path = self.home / args
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body or "")
            return
        if cmd == "launch":
            self.launch(args)
            self.settle()
            self.capture("launch", png=False)
            return
        if cmd == "key":
            for k in args.split():
                self.tmux("send-keys", "-t", "g", KEY_ALIASES.get(k, k))
                time.sleep(0.05)
            self.settle()
            self.capture(f"key-{args}", png=False)
            return
        if cmd == "text":
            self.tmux("send-keys", "-t", "g", "-l", args)
            self.settle()
            self.capture(f"text-{args}", png=False)
            return
        if cmd == "sleep":
            time.sleep(float(args))
            self.capture(f"sleep-{args}", png=False)
            return
        if cmd == "resize":
            w, h = map(int, args.split("x"))
            self.tmux("resize-window", "-t", "g", "-x", str(w), "-y", str(h))
            self.width, self.height = w, h
            self.settle()
            self.capture(f"resize-{args}", png=False)
            return
        if cmd == "snap":
            self.capture(args or "snap", png=True)
            return
        if cmd == "wait-for":
            text, _, timeout = args.partition(" ~ ")
            deadline = time.time() + float(timeout or 5)
            while text not in self.pane_text() and time.time() < deadline:
                time.sleep(0.1)
            self.capture(f"wait-{text}", png=False)
            self.check(text in self.screen, f"screen shows {text!r}")
            return
        if cmd == "expect":
            self.check(args in self.screen, f"screen shows {args!r}")
            return
        if cmd == "reject":
            self.check(args not in self.screen, f"screen lacks {args!r}")
            return
        if cmd == "expect-ansi":
            self.check(re.search(args, self.ansi) is not None, f"ansi matches /{args}/")
            return
        if cmd in ("expect-file", "reject-file"):
            rel, _, text = args.partition(" ~ ")
            path = self.home / rel
            contents = path.read_text() if path.is_file() else None
            if cmd == "reject-file":
                ok = contents is None if not text else (contents is not None and text not in contents)
                self.check(ok, f"{rel} {'lacks ' + repr(text) if text else 'absent'}")
            else:
                ok = contents is not None and text in contents
                self.check(ok, f"{rel} {'contains ' + repr(text) if text else 'exists'}")
            return
        if cmd == "expect-exit":
            deadline = time.time() + 5
            dead, status = self.pane_dead()
            while not dead and time.time() < deadline:
                time.sleep(0.1)
                dead, status = self.pane_dead()
            self.check(dead and status == args, f"app exited with status {args} (dead={dead}, status={status!r})")
            return
        raise SystemExit(f"{self.scenario}:{self.n}: unknown step {cmd!r}")

    def teardown(self):
        socket = self.tmux("display", "-p", "#{socket_path}", check=False).strip()
        self.tmux("kill-server", check=False)
        if socket:
            Path(socket).unlink(missing_ok=True)
        if (self.home / ".grind-tui").exists():
            shutil.copytree(self.home / ".grind-tui", self.out / "home" / ".grind-tui")
        for extra in ("clipboard.txt", "launched.txt"):
            if (self.home / extra).exists():
                shutil.copy(self.home / extra, self.out / "home" / extra)
        shutil.rmtree(self.home, ignore_errors=True)


def parse(text: str) -> list[tuple[str, str, str | None]]:
    """One step per line: `cmd args`. `write REL` takes a body up to a line `EOF`."""
    steps, lines = [], iter(text.splitlines())
    for line in lines:
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        cmd, _, args = line.strip().partition(" ")
        body = None
        if cmd == "write":
            body_lines = []
            for b in lines:
                if b == "EOF":
                    break
                body_lines.append(b)
            body = "\n".join(body_lines) + "\n"
        steps.append((cmd, args if cmd == "text" else args.strip(), body))
    return steps


def slug(label: str) -> str:
    return re.sub(r"[^A-Za-z0-9]+", "-", label).strip("-")[:40] or "step"


def main():
    argv = sys.argv[1:]
    keep = "--keep" in argv
    argv = [a for a in argv if a != "--keep"]
    opts = dict(zip(argv[1::2], argv[2::2]))
    if not argv or not set(opts) <= {"--size", "--out"}:
        raise SystemExit(__doc__)
    build = subprocess.run(["cargo", "build", "--release"], cwd=REPO, capture_output=True, text=True)
    if build.returncode:
        raise SystemExit(build.stderr)
    run = Run(Path(argv[0]).resolve(), opts.get("--size"), Path(opts["--out"]).resolve() if "--out" in opts else None)
    run.setup()
    try:
        for cmd, args, body in run.steps:
            run.step(cmd, args, body)
    finally:
        if keep:
            print(f"kept: tmux -L {run.sock} attach -t g   HOME={run.home}")
        else:
            run.teardown()
        verdict = "PASS" if not run.failures else f"FAIL ({len(run.failures)})"
        run.log.append(f"RESULT {verdict} {run.scenario.name} {run.width}x{run.height}")
        run.log.extend(f"  {f}" for f in run.failures)
        (run.out / "report.txt").write_text("\n".join(run.log) + "\n")
        print(f"RESULT {verdict} -> {run.out}")
    sys.exit(1 if run.failures else 0)


if __name__ == "__main__":
    main()
