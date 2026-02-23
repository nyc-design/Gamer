#!/usr/bin/env python3
import os
import re
import subprocess
import tkinter as tk


DISPLAY = os.environ.get("DISPLAY", ":0")
MODE_FILE = os.environ.get("SCREEN_TOOL_MODE_FILE", "/home/gamer/.cache/screen-tool.mode")
BUTTON_SIZE = 42
MARGIN = 16
POLL_MS = 500


def run(cmd):
    return subprocess.check_output(cmd, stderr=subprocess.DEVNULL, text=True)


def bottom_stream_connected() -> bool:
    try:
        out = run(["ss", "-Htan"])
        for line in out.splitlines():
            if not line.startswith("ESTAB"):
                continue
            if ":48089" in line or ":48010" in line:
                return True
    except Exception:
        pass
    return False


def display_geom(name: str):
    try:
        out = run(["xrandr", "--current"])
    except Exception:
        return None
    pat = re.compile(rf"^{re.escape(name)}\s+.*?(\d+)x(\d+)\+(\d+)\+(\d+)", re.MULTILINE)
    m = pat.search(out)
    if not m:
        return None
    return tuple(int(v) for v in m.groups())


def target_rect():
    target = "DP-2" if bottom_stream_connected() else "DP-0"
    geom = display_geom(target)
    if geom:
        return geom
    # fallback
    return (1920, 1080, 0, 0)


def toggle_mode():
    os.makedirs(os.path.dirname(MODE_FILE), exist_ok=True)
    mode = "auto"
    try:
        with open(MODE_FILE, "r", encoding="utf-8") as f:
            mode = f.read().strip() or "auto"
    except Exception:
        mode = "auto"
    next_mode = "force_show" if mode != "force_show" else "auto"
    with open(MODE_FILE, "w", encoding="utf-8") as f:
        f.write(next_mode + "\n")
    return next_mode


def main():
    root = tk.Tk()
    root.title("ScreenToolToggle")
    root.overrideredirect(True)
    root.attributes("-topmost", True)
    root.configure(bg="#11141d")

    btn = tk.Button(
        root,
        text="◎",
        font=("Sans", 14, "bold"),
        width=2,
        height=1,
        relief="flat",
        bg="#2b3552",
        activebackground="#4b5f9e",
        fg="#e6ebff",
        command=toggle_mode,
    )
    btn.pack(fill="both", expand=True)

    def tick():
        w, h, x, y = target_rect()
        px = x + w - BUTTON_SIZE - MARGIN
        py = y + (h - BUTTON_SIZE) // 2
        root.geometry(f"{BUTTON_SIZE}x{BUTTON_SIZE}+{px}+{py}")
        root.after(POLL_MS, tick)

    tick()
    root.mainloop()


if __name__ == "__main__":
    main()
