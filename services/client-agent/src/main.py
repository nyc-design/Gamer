import json
import logging
import os
import platform
import shutil
import subprocess
from pathlib import Path
from typing import Any, Dict, Optional

from fastapi import FastAPI, HTTPException
from pydantic import BaseModel

logging.basicConfig(level=os.getenv("LOG_LEVEL", "INFO"))
logger = logging.getLogger("gamer-windows-agent")

APP = FastAPI(title="Gamer Client Agent", version="0.1.0")

DEFAULT_MANIFEST = {
    "session_id": "dev-windows-session-001",
    "user_id": "me",
    "rom_path": "roms/pokemon-alpha-sapphire.3ds",
    "save_path": "saves/pokemon-alpha-sapphire/slot-1/main.sav",
    "save_filename": "main.sav",
    "resolution": "1920x1080",
    "fps": 60,
    "codec": "h264",
    "dual_screen": {
        "enabled": True,
        "mode": "apollo_virtual_display",
        "auto_move_second_window": True,
        "screen_a": {"role": "top"},
        "screen_b": {"role": "bottom"},
    },
    "windows": {
        "apollo": {
            "enabled": True,
            "exe_path": "C:/Program Files/Apollo/Apollo.exe",
        },
        "shader_glass": {
            "enabled": True,
            "exe_path": "C:/Program Files/ShaderGlass/ShaderGlass.exe",
            "preset": "retroarch/default.slangp",
        },
        "emulator": {
            "name": "azahar",
            "exe_path": "C:/Emulators/Azahar/azahar.exe",
            "args": ["--fullscreen", "--layout", "separate"],
        },
    },
    "mounts": {
        "roms": "D:/gamer/roms",
        "saves": "D:/gamer/saves",
        "configs": "D:/gamer/configs",
        "firmware": "D:/gamer/firmware",
        "steam": "D:/gamer/steam",
    },
}


class StartResponse(BaseModel):
    ok: bool
    message: str
    manifest_path: Optional[str] = None


class AgentState:
    def __init__(self) -> None:
        self.started = False
        self.manifest: Dict[str, Any] = {}
        self.processes: Dict[str, subprocess.Popen] = {}


STATE = AgentState()


def _is_windows() -> bool:
    return platform.system().lower() == "windows"


def _run(cmd: list[str], check: bool = True) -> subprocess.CompletedProcess:
    logger.info("run: %s", " ".join(cmd))
    return subprocess.run(cmd, check=check, capture_output=True, text=True)


def load_manifest() -> Dict[str, Any]:
    manifest_path = os.getenv("SESSION_MANIFEST_PATH")
    if manifest_path and Path(manifest_path).exists():
        with open(manifest_path, "r", encoding="utf-8") as f:
            return json.load(f)

    local_manifest = Path(__file__).resolve().parents[1] / "manifests" / "session_manifest.windows.dev.json"
    if local_manifest.exists():
        return json.loads(local_manifest.read_text(encoding="utf-8"))

    return DEFAULT_MANIFEST


def ensure_dirs(manifest: Dict[str, Any]) -> None:
    for _, path in manifest.get("mounts", {}).items():
        Path(path).mkdir(parents=True, exist_ok=True)


def setup_storage(manifest: Dict[str, Any]) -> None:
    # Stubbed for now; keeps same contract/shape as linux side.
    # We assume rclone remotes are preconfigured by bootstrap script.
    roms = manifest["mounts"]["roms"]
    saves = manifest["mounts"]["saves"]
    logger.info("storage ready: roms=%s saves=%s", roms, saves)


def start_apollo(manifest: Dict[str, Any]) -> None:
    cfg = manifest.get("windows", {}).get("apollo", {})
    if not cfg.get("enabled"):
        return

    exe = cfg.get("exe_path")
    if not exe or not Path(exe).exists():
        logger.warning("Apollo not found at %s; skipping start", exe)
        return

    proc = subprocess.Popen([exe], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    STATE.processes["apollo"] = proc
    logger.info("Apollo started (pid=%s)", proc.pid)


def start_shader_glass(manifest: Dict[str, Any]) -> None:
    cfg = manifest.get("windows", {}).get("shader_glass", {})
    if not cfg.get("enabled"):
        return

    exe = cfg.get("exe_path")
    if not exe or not Path(exe).exists():
        logger.warning("ShaderGlass not found at %s; skipping start", exe)
        return

    args = [exe]
    preset = cfg.get("preset")
    if preset:
        args.extend(["--preset", preset])

    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    STATE.processes["shader_glass"] = proc
    logger.info("ShaderGlass started (pid=%s)", proc.pid)


def start_emulator(manifest: Dict[str, Any]) -> None:
    emu = manifest.get("windows", {}).get("emulator", {})
    exe = emu.get("exe_path")
    if not exe or not Path(exe).exists():
        logger.warning("Emulator not found at %s; skipping start", exe)
        return

    args = [exe] + emu.get("args", [])
    proc = subprocess.Popen(args, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    STATE.processes["emulator"] = proc
    logger.info("Emulator started (pid=%s)", proc.pid)


def maybe_install_window_hotkeys() -> None:
    if not _is_windows():
        logger.info("Non-windows environment: skipping hotkey helper")
        return

    ahk = shutil.which("AutoHotkey64.exe") or shutil.which("autohotkey")
    helper = Path(__file__).resolve().parents[2] / "infrastructure" / "windows" / "scripts" / "move-window-next-monitor.ahk"
    if not ahk or not helper.exists():
        logger.warning("AHK/helper missing; window move hotkey not installed")
        return

    proc = subprocess.Popen([ahk, str(helper)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    STATE.processes["window_hotkeys"] = proc
    logger.info("Window hotkeys helper started (pid=%s)", proc.pid)


def stop_all() -> None:
    for name, proc in list(STATE.processes.items()):
        try:
            proc.terminate()
        except Exception:
            pass
        logger.info("terminated process: %s", name)
    STATE.processes.clear()
    STATE.started = False


@APP.get("/health")
def health() -> Dict[str, Any]:
    return {"ok": True, "started": STATE.started, "processes": list(STATE.processes.keys())}


@APP.get("/manifest")
def manifest() -> Dict[str, Any]:
    if not STATE.manifest:
        STATE.manifest = load_manifest()
    return STATE.manifest


@APP.post("/start", response_model=StartResponse)
def start() -> StartResponse:
    if STATE.started:
        return StartResponse(ok=True, message="already started")

    STATE.manifest = load_manifest()
    manifest_path = os.getenv("SESSION_MANIFEST_PATH")

    ensure_dirs(STATE.manifest)
    setup_storage(STATE.manifest)
    start_apollo(STATE.manifest)
    maybe_install_window_hotkeys()
    start_shader_glass(STATE.manifest)
    start_emulator(STATE.manifest)

    STATE.started = True
    return StartResponse(ok=True, message="started", manifest_path=manifest_path)


@APP.post("/stop", response_model=StartResponse)
def stop() -> StartResponse:
    if not STATE.started:
        return StartResponse(ok=True, message="already stopped")
    stop_all()
    return StartResponse(ok=True, message="stopped")


@APP.post("/reload-manifest", response_model=StartResponse)
def reload_manifest() -> StartResponse:
    if STATE.started:
        raise HTTPException(status_code=409, detail="stop first")
    STATE.manifest = load_manifest()
    return StartResponse(ok=True, message="manifest reloaded")
