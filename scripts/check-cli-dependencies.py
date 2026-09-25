#!/usr/bin/env python3
"""Prevent the controller from depending on the desktop daemon or UI toolkit."""
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parents[1]
target = sys.argv[1] if len(sys.argv) > 1 else "x86_64-pc-windows-msvc"
result = subprocess.run(
    ["cargo", "tree", "--locked", "-p", "illiumctl", "--target", target,
     "--edges", "normal,build", "--prefix", "none", "--format", "{p}"],
    cwd=root, check=True, text=True, capture_output=True,
)
names = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
forbidden = sorted(name for name in names if name in {"illium", "slint", "slint-build", "winit"}
                   or name.startswith("i-slint-"))
if forbidden:
    sys.exit("CLI unexpectedly depends on desktop code: " + ", ".join(forbidden))
print(f"CLI dependency boundary OK ({target}; {len(names)} package names)")
