#!/usr/bin/env python3
"""Exercise Baseball Hour in a real PTY and save its terminal transcript."""

import argparse
import codecs
import errno
import fcntl
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unicodedata


class Screen:
    def __init__(self, columns, rows):
        self.pending = ""
        self.row = 0
        self.column = 0
        self.resize(columns, rows)

    def resize(self, columns, rows):
        self.columns, self.rows = columns, rows
        self.cells = [[" "] * columns for _ in range(rows)]
        self.row = min(self.row, rows - 1)
        self.column = min(self.column, columns - 1)

    def feed(self, text):
        text = self.pending + text
        self.pending = ""
        index = 0
        while index < len(text):
            char = text[index]
            if char == "\x1b":
                if index + 1 >= len(text):
                    break
                if text[index + 1] == "[":
                    match = re.match(r"\x1b\[([0-?]*)([ -/]*)([@-~])", text[index:])
                    if not match:
                        break
                    self.csi(match[1], match[3])
                    index += len(match[0])
                    continue
                if text[index + 1] == "]":
                    match = re.match(r"\x1b\].*?(?:\x07|\x1b\\)", text[index:], re.S)
                    if not match:
                        break
                    index += len(match[0])
                    continue
                index += 2
                continue
            if char == "\r":
                self.column = 0
            elif char == "\n":
                self.row = min(self.rows - 1, self.row + 1)
            elif char == "\b":
                self.column = max(0, self.column - 1)
            elif char >= " " and not unicodedata.combining(char):
                width = 2 if unicodedata.east_asian_width(char) in "WF" else 1
                if self.column < self.columns:
                    self.cells[self.row][self.column] = char
                    if width == 2 and self.column + 1 < self.columns:
                        self.cells[self.row][self.column + 1] = ""
                self.column += width
            index += 1
        self.pending = text[index:]

    def csi(self, raw, command):
        if raw.startswith(("?", ">")):
            return
        args = [int(value or 0) for value in raw.split(";")]
        amount = args[0] or 1
        if command in "Hf":
            self.row = min(self.rows - 1, max(0, amount - 1))
            self.column = min(self.columns - 1, max(0, (args[1] if len(args) > 1 else 1) - 1))
        elif command == "A":
            self.row = max(0, self.row - amount)
        elif command == "B":
            self.row = min(self.rows - 1, self.row + amount)
        elif command == "C":
            self.column = min(self.columns - 1, self.column + amount)
        elif command == "D":
            self.column = max(0, self.column - amount)
        elif command == "G":
            self.column = min(self.columns - 1, amount - 1)
        elif command == "d":
            self.row = min(self.rows - 1, amount - 1)
        elif command == "J" and args[0] in (2, 3):
            self.cells = [[" "] * self.columns for _ in range(self.rows)]
        elif command == "J" and args[0] == 0:
            self.cells[self.row][self.column:] = [" "] * (self.columns - self.column)
            for row in range(self.row + 1, self.rows):
                self.cells[row] = [" "] * self.columns
        elif command == "K":
            start = 0 if args[0] in (1, 2) else self.column
            end = self.column + 1 if args[0] == 1 else self.columns
            self.cells[self.row][start:end] = [" "] * (end - start)
        elif command == "X":
            end = min(self.columns, self.column + amount)
            self.cells[self.row][self.column:end] = [" "] * (end - self.column)

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.cells)


class Session:
    def __init__(self, binary, directory, name):
        self.directory = directory
        self.name = name
        self.master, self.slave = pty.openpty()
        self.original = termios.tcgetattr(self.slave)
        self.screen = Screen(140, 42)
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.transcript = bytearray()
        self.resize(140, 42, notify=False)
        env = dict(os.environ, TERM="xterm-256color")
        command = [str(binary)] if isinstance(binary, (str, Path)) else list(binary)
        self.process = subprocess.Popen(
            command + ["--demo", "--timezone", "America/Denver", "--date", "2026-07-04",
             "--config-dir", str(directory / name / "config"),
             "--cache-dir", str(directory / name / "cache")],
            stdin=self.slave, stdout=self.slave, stderr=self.slave,
            env=env, process_group=0,
        )

    def resize(self, columns, rows, notify=True):
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, columns, 0, 0))
        self.screen.resize(columns, rows)
        if notify:
            os.killpg(self.process.pid, signal.SIGWINCH)

    def read(self, timeout):
        ready, _, _ = select.select([self.master], [], [], timeout)
        if not ready:
            return
        try:
            chunk = os.read(self.master, 65536)
        except OSError as error:
            if error.errno != errno.EIO:
                raise
            return
        self.transcript.extend(chunk)
        self.screen.feed(self.decoder.decode(chunk))

    def settle(self):
        deadline = time.monotonic() + 0.5
        while time.monotonic() < deadline:
            before = len(self.transcript)
            self.read(0.06)
            if len(self.transcript) == before:
                return

    def expect(self, pattern, label, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if re.search(pattern, self.screen.text(), re.I):
                self.settle()
                if re.search(pattern, self.screen.text(), re.I):
                    self.save_screen(label)
                    return
            if self.process.poll() is not None:
                self.read(0)
                raise AssertionError(f"Exited {self.process.returncode} while waiting for {label}")
            self.read(min(0.25, max(0, deadline - time.monotonic())))
        raise AssertionError(f"Missing {label}: /{pattern}/\n{self.screen.text()}")

    def expect_absent(self, pattern, label, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.read(min(0.15, max(0, deadline - time.monotonic())))
            if not re.search(pattern, self.screen.text(), re.I):
                self.settle()
                if not re.search(pattern, self.screen.text(), re.I):
                    self.save_screen(label)
                    return
            if self.process.poll() is not None:
                raise AssertionError(f"Exited while waiting for {label}")
        raise AssertionError(f"Still visible after {label}: /{pattern}/\n{self.screen.text()}")

    def send(self, keys):
        os.write(self.master, keys)
        self.read(0.15)

    def save_screen(self, label):
        (self.directory / f"{self.name}-{label}.txt").write_text(self.screen.text() + "\n")

    def finish(self, keys):
        self.send(keys)
        deadline = time.monotonic() + 5
        while self.process.poll() is None and time.monotonic() < deadline:
            self.read(0.1)
        assert self.process.poll() == 0, f"Expected exit 0, got {self.process.poll()}"
        self.read(0)
        assert termios.tcgetattr(self.slave) == self.original, "Terminal attributes were not restored"
        assert b"\x1b[?1049l" in self.transcript, "Alternate screen was not restored"

    def close(self):
        if self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGKILL)
            self.process.wait(timeout=3)
        self.read(0)
        (self.directory / f"{self.name}.ansi").write_bytes(self.transcript)
        self.save_screen("last")
        termios.tcsetattr(self.slave, termios.TCSANOW, self.original)
        os.close(self.master)
        os.close(self.slave)


def verify(binary, output):
    with tempfile.TemporaryDirectory(prefix="baseballhour-pty-") as isolated:
        session = Session(binary, Path(isolated), "navigation")
        session.directory = output
        try:
            session.expect(r"baseball\s+hour", "wide-layout")
            assert not termios.tcgetattr(session.slave)[3] & termios.ICANON, "App did not enter raw mode"
            session.resize(80, 24)
            session.expect(r"baseball\s+hour", "narrow-layout")
            session.send(b"?")
            session.expect(r"Keyboard guide", "help")
            session.send(b"\x1b")
            session.expect_absent(r"Keyboard guide", "help-closed")
            session.send(b"/")
            session.expect(r"Search teams / ballparks (?:>|›)", "search-mode")
            session.send(b"NYY\r")
            session.expect(r"Search teams / ballparks (?:>|›) NYY\s+/\s*edit", "search-applied")
            session.expect(r"THE SLATE\s+1 game\b", "filtered-yankees")
            session.send(b"\x1b")
            session.expect_absent(r"Search teams / ballparks (?:>|›)", "search-cleared")
            session.send(b"f")
            session.expect(r"Follow a team", "follow-dialog")
            session.send(b"\x1b")
            session.expect_absent(r"Follow a team", "follow-closed")
            session.send(b"3")
            session.expect(r"Choose (?:a|your) nearby place", "place-dialog")
            session.send(b"Denver\r")
            session.expect(r"NEAR Denver", "near-denver")
            session.expect(r"\d[\d,.]*\s*(?:mi\b|miles\b)", "nearby-distance")
            session.send(b"\x1b[C")
            session.expect(r"2026-07-05|Jul(?:y)?\s+0?5|0?5\s+Jul(?:y)?|Sun[^\n]*0?5", "next-date")
            session.finish(b"q")
        finally:
            session.close()
        session = Session(binary, Path(isolated), "small-help")
        session.directory = output
        try:
            session.expect(r"baseball\s+hour", "initial")
            session.resize(40, 12)
            session.expect(r"3 Nearby", "all-views-visible")
            session.send(b"?")
            session.expect(r"Keyboard guide", "help")
            session.expect(r"Esc Close.*q Quit", "help-controls-visible")
            session.finish(b"q")
        finally:
            session.close()
        session = Session(binary, Path(isolated), "interrupt")
        session.directory = output
        try:
            session.expect(r"baseball\s+hour", "initial")
            session.send(b"?")
            session.expect(r"Keyboard guide", "help")
            session.finish(b"\x03")
        finally:
            session.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path, default=Path("target/debug/baseballhour"))
    parser.add_argument("--output", type=Path, default=Path("/tmp/baseballhour-terminal-check"))
    parser.add_argument("--command", nargs=argparse.REMAINDER, help="command and arguments before app flags")
    argv = sys.argv[1:]
    command = None
    if "--command" in argv:
        index = argv.index("--command")
        command = argv[index + 1:]
        argv = argv[:index]
        if not command:
            parser.error("--command requires an executable")
    args = parser.parse_args(argv)
    binary = command or args.binary.resolve()
    if not command and not binary.is_file():
        parser.error(f"binary does not exist: {binary}")
    args.output.mkdir(parents=True, exist_ok=True)
    try:
        verify(binary, args.output.resolve())
    except (AssertionError, OSError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Terminal verification failed: {error}\nTranscripts: {args.output.resolve()}\n")
    print(f"PTY checks passed. Terminal restored after q and Ctrl-C. Transcripts: {args.output.resolve()}")


if __name__ == "__main__":
    main()
