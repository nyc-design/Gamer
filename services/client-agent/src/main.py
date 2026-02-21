import json
import logging
import os
import platform
import shutil
import subprocess
import time
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


class ClientEvent(BaseModel):
    connected_clients: Optional[int] = None
    client_id: Optional[str] = None


class ProcessInfo(BaseModel):
    name: str
    pid: Optional[int] = None


class ManifestSetRequest(BaseModel):
    manifest: Dict[str, Any]
    persist_path: Optional[str] = None


class AgentState:
    def __init__(self) -> None:
        self.started = False
        self.started_at: Optional[float] = None
        self.manifest: Dict[str, Any] = {}
        self.processes: Dict[str, subprocess.Popen] = {}
        self.connected_clients: int = 0
        self.connected_client_ids: set[str] = set()
        self.last_script_runs: Dict[str, Dict[str, Any]] = {}


STATE = AgentState()


def _is_windows() -> bool:
    return platform.system().lower() == "windows"


def _run(cmd: list[str], check: bool = True, timeout: Optional[int] = None) -> subprocess.CompletedProcess:
    logger.info("run: %s", " ".join(cmd))
    return subprocess.run(cmd, check=check, capture_output=True, text=True, timeout=timeout)


def _resolve_exe(configured: str | None, fallbacks: list[str]) -> Optional[Path]:
    candidates: list[Path] = []
    if configured:
        candidates.append(Path(configured))
    candidates.extend(Path(x) for x in fallbacks)
    for c in candidates:
        try:
            if c.exists():
                return c
        except Exception:
            continue
    return None


def _find_windows_script(script_name: str) -> Optional[Path]:
    # 1) Explicit override.
    base = os.getenv("WINDOWS_SETUP_DIR")
    if base:
        p = Path(base) / script_name
        if p.exists():
            return p

    # 2) Deployed setup path.
    deployed = Path("C:/ProgramData/gamer/setup")
    p = deployed / script_name
    if p.exists():
        return p

    # 3) Repo checkout relative path (dev).
    repo_rel = Path(__file__).resolve().parents[3] / "infrastructure" / "windows" / "scripts" / script_name
    if repo_rel.exists():
        return repo_rel

    return None


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
    mounts = manifest.get("mounts", {})
    try:
        has_d = Path("D:/").exists()
    except Exception:
        has_d = False
    for key, path in mounts.items():
        p = Path(path)
        if p.drive.lower() == "d:" and not has_d:
            raw = str(p)
            if raw.lower().startswith("d:/"):
                raw = raw.replace("D:/", "C:/", 1)
            elif raw.lower().startswith("d:\\"):
                raw = raw.replace("D:\\", "C:\\", 1)
            p = Path(raw)
            mounts[key] = str(p).replace("\\", "/")
        p.mkdir(parents=True, exist_ok=True)


def setup_storage(manifest: Dict[str, Any]) -> None:
    # Stubbed for now; keeps same contract/shape as linux side.
    # We assume rclone remotes are preconfigured by bootstrap script.
    roms = manifest["mounts"]["roms"]
    saves = manifest["mounts"]["saves"]
    logger.info("storage ready: roms=%s saves=%s", roms, saves)


def start_apollo(manifest: Dict[str, Any]) -> None:
    # Apollo must run in the interactive Windows user session (not SYSTEM/session0),
    # otherwise capture can bind to Microsoft Basic Render Driver (1Hz) and software encode.
    managed_externally = os.getenv("APOLLO_MANAGED_EXTERNALLY", "true").lower() in {"1", "true", "yes"}
    if managed_externally and _is_windows():
        logger.info("APOLLO_MANAGED_EXTERNALLY=true; skipping Apollo spawn from agent")
        return

    cfg = manifest.get("windows", {}).get("apollo", {})
    if not cfg.get("enabled"):
        return

    exe = _resolve_exe(
        cfg.get("exe_path"),
        [
            "C:/ProgramData/gamer/bin/Apollo/Apollo.exe",
            "C:/ProgramData/gamer/bin/Apollo/Apollo-installer.exe",
            "C:/Users/user/AppData/Local/Apollo/Apollo.exe",
        ],
    )
    if exe is None:
        logger.warning("Apollo not found; skipping start")
        return

    proc = subprocess.Popen([str(exe)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    STATE.processes["apollo"] = proc
    logger.info("Apollo started from %s (pid=%s)", exe, proc.pid)


def start_shader_glass(manifest: Dict[str, Any]) -> None:
    cfg = manifest.get("windows", {}).get("shader_glass", {})
    if not cfg.get("enabled"):
        return

    exe = _resolve_exe(
        cfg.get("exe_path"),
        [
            "C:/ProgramData/gamer/bin/ShaderGlass/ShaderGlass.exe",
            "C:/Program Files/ShaderGlass/ShaderGlass.exe",
        ],
    )
    if exe is None:
        logger.warning("ShaderGlass not found; skipping start")
        return

    args = [str(exe)]
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
    helper = _find_windows_script("move-window-next-monitor.ahk")
    if not ahk or helper is None or not helper.exists():
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
    STATE.connected_clients = 0
    STATE.connected_client_ids.clear()
    STATE.started = False
    STATE.started_at = None


def _run_powershell_script(script_name: str, extra_args: list[str]) -> Dict[str, Any]:
    timeout_s = int(os.getenv("POWERSHELL_SCRIPT_TIMEOUT_SEC", "20"))
    run_info: Dict[str, Any] = {
        "script": script_name,
        "timestamp": time.time(),
        "ok": False,
        "exit_code": None,
        "stdout": "",
        "stderr": "",
        "duration_ms": None,
    }
    script = _find_windows_script(script_name)
    if not script:
        logger.warning("script not found: %s", script_name)
        run_info["stderr"] = "script not found"
        STATE.last_script_runs[script_name] = run_info
        return run_info

    args = ["powershell", "-ExecutionPolicy", "Bypass", "-File", str(script)] + extra_args
    t0 = time.time()
    try:
        result = _run(args, check=False, timeout=timeout_s)
        run_info["exit_code"] = result.returncode
        run_info["ok"] = result.returncode == 0
        run_info["stdout"] = (result.stdout or "").strip()[:600]
        run_info["stderr"] = (result.stderr or "").strip()[:600]
        logger.info(
            "script %s exit=%s stdout=%s stderr=%s",
            script_name,
            result.returncode,
            run_info["stdout"],
            run_info["stderr"],
        )
    except subprocess.TimeoutExpired:
        run_info["stderr"] = f"script timed out after {timeout_s}s"
        run_info["exit_code"] = -1
        logger.warning("script %s timed out after %ss", script_name, timeout_s)
    except Exception as e:
        logger.warning("failed to run script %s: %s", script_name, e)
        run_info["stderr"] = str(e)
    run_info["duration_ms"] = int((time.time() - t0) * 1000)
    STATE.last_script_runs[script_name] = run_info
    return run_info


@APP.get("/health")
def health() -> Dict[str, Any]:
    proc_data = []
    alive_count = 0
    for name, proc in STATE.processes.items():
        pid = None
        alive = None
        try:
            pid = proc.pid
            alive = proc.poll() is None
            if alive:
                alive_count += 1
        except Exception:
            pass
        proc_data.append({"name": name, "pid": pid, "alive": alive})
    return {
        "ok": True,
        "started": STATE.started,
        "started_at": STATE.started_at,
        "connected_clients": STATE.connected_clients,
        "connected_client_ids": sorted(list(STATE.connected_client_ids)),
        "alive_processes": alive_count,
        "processes": proc_data,
        "last_script_runs": STATE.last_script_runs,
    }


@APP.get("/manifest")
def manifest() -> Dict[str, Any]:
    if not STATE.manifest:
        STATE.manifest = load_manifest()
    return STATE.manifest


@APP.post("/manifest-set", response_model=StartResponse)
def manifest_set(req: ManifestSetRequest) -> StartResponse:
    if STATE.started:
        raise HTTPException(status_code=409, detail="stop first")
    STATE.manifest = req.manifest
    if req.persist_path:
        out = Path(req.persist_path)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(req.manifest, indent=2), encoding="utf-8")
    return StartResponse(ok=True, message="manifest set", manifest_path=req.persist_path)


@APP.post("/manifest-clear", response_model=StartResponse)
def manifest_clear() -> StartResponse:
    if STATE.started:
        raise HTTPException(status_code=409, detail="stop first")
    STATE.manifest = {}
    return StartResponse(ok=True, message="manifest cleared")


@APP.post("/start", response_model=StartResponse)
def start() -> StartResponse:
    try:
        if STATE.started:
            return StartResponse(ok=True, message="already started")

        if not STATE.manifest:
            STATE.manifest = load_manifest()
        manifest_path = os.getenv("SESSION_MANIFEST_PATH")

        ensure_dirs(STATE.manifest)
        setup_storage(STATE.manifest)
        start_apollo(STATE.manifest)
        maybe_install_window_hotkeys()
        start_shader_glass(STATE.manifest)
        start_emulator(STATE.manifest)

        STATE.started = True
        STATE.started_at = time.time()
        return StartResponse(ok=True, message="started", manifest_path=manifest_path)
    except Exception as e:
        logger.exception("start failed")
        raise HTTPException(status_code=500, detail=str(e))


@APP.post("/stop", response_model=StartResponse)
def stop() -> StartResponse:
    if not STATE.started:
        return StartResponse(ok=True, message="already stopped")
    stop_all()
    return StartResponse(ok=True, message="stopped")


@APP.post("/cleanup-processes", response_model=StartResponse)
def cleanup_processes() -> StartResponse:
    removed: list[str] = []
    for name, proc in list(STATE.processes.items()):
        try:
            if proc.poll() is not None:
                removed.append(name)
                del STATE.processes[name]
        except Exception:
            removed.append(name)
            del STATE.processes[name]
    return StartResponse(ok=True, message=f"removed={','.join(removed) if removed else 'none'}")


@APP.post("/reload-manifest", response_model=StartResponse)
def reload_manifest() -> StartResponse:
    if STATE.started:
        raise HTTPException(status_code=409, detail="stop first")
    STATE.manifest = load_manifest()
    return StartResponse(ok=True, message="manifest reloaded")


@APP.post("/client-connected", response_model=StartResponse)
def client_connected(event: ClientEvent) -> StartResponse:
    if event.client_id:
        STATE.connected_client_ids.add(event.client_id)
        STATE.connected_clients = len(STATE.connected_client_ids)
    elif event.connected_clients is not None:
        incoming = max(0, event.connected_clients)
        # Be resilient to integrations that emit edge events but always send 1.
        if incoming <= STATE.connected_clients:
            STATE.connected_clients = STATE.connected_clients + 1
        else:
            STATE.connected_clients = incoming
    else:
        STATE.connected_clients = max(0, STATE.connected_clients + 1)
    _run_powershell_script("apollo-on-client-connect.ps1", ["-ConnectedClients", str(STATE.connected_clients)])
    return StartResponse(ok=True, message=f"client-connected={STATE.connected_clients}")


@APP.post("/client-disconnected", response_model=StartResponse)
def client_disconnected(event: ClientEvent) -> StartResponse:
    if event.client_id:
        STATE.connected_client_ids.discard(event.client_id)
        STATE.connected_clients = len(STATE.connected_client_ids)
    elif event.connected_clients is not None:
        incoming = max(0, event.connected_clients)
        # Be resilient to integrations that emit edge events but send stale counts.
        if incoming >= STATE.connected_clients and STATE.connected_clients > 0:
            STATE.connected_clients = STATE.connected_clients - 1
        else:
            STATE.connected_clients = incoming
    else:
        STATE.connected_clients = max(0, STATE.connected_clients - 1)
    if STATE.connected_clients == 0:
        STATE.connected_client_ids.clear()
    _run_powershell_script("apollo-on-client-disconnect.ps1", ["-ConnectedClients", str(STATE.connected_clients)])
    return StartResponse(ok=True, message=f"client-disconnected={STATE.connected_clients}")


@APP.post("/position-dual-now", response_model=StartResponse)
def position_dual_now() -> StartResponse:
    _run_powershell_script("position-azahar-dual.ps1", ["-MaxAttempts", "30", "-SleepMs", "300"])
    return StartResponse(ok=True, message="position script invoked")
