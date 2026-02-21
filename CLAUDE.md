# Gamer - Cloud Gaming Platform

A cloud-streamed emulator and PC gaming platform. User picks a game in the web app, we spin up a GPU VM, launch the emulator/Steam in a Docker container via Wolf (Games on Whales), and stream video to the user's browser via moonlight-web-stream (v1) or MoQ (v2). Dual-screen emulators (DS/3DS) stream to two separate browser clients with client-side crop.

**Canonical architecture reference**: `ARCHITECTURE.md` — always defer to it for detailed technical decisions. This CLAUDE.md is a working summary for agents.

## Core Flow

1. User logs into web app, browses game library
2. User selects game + save slot → hits Play
3. Main Server builds session manifest, provisions GPU VM via TensorDock/GCP API
4. VM boots → Gamer Agent starts (systemd), fetches session manifest
5. Gamer Agent sets up rclone mounts (R2 for ROMs, GCS for saves/configs/firmware)
6. Gamer Agent copies selected save slot to working dir, writes Wolf config.toml
7. Wolf starts → spawns emulator Docker container with GPU, Wayland compositor, virtual input
8. moonlight-web-stream bridges Wolf's Moonlight protocol to WebSocket
9. Browser connects via WebSocket, decodes H.264 with WebCodecs, renders to canvas
10. User plays; input flows back via WebSocket → moonlight-web-stream → Wolf → inputtino → emulator
11. On disconnect/idle → Gamer Agent copies save back, reports to Main Server → VM stopped

## Architecture

### Key Decisions (from ARCHITECTURE.md)

1. **Wolf replaces Sunshine + custom compositor** — includes Wayland compositor, Moonlight protocol, GStreamer+NVENC encoding, Docker app spawning, virtual input
2. **Dual-screen solved at the web client layer** — Wolf streams combined frame, two browser clients each crop to their assigned screen
3. **moonlight-web-stream (Helix fork) is the v1 web client** — WebSocket transport, WebCodecs decode, binary input protocol
4. **Split storage: Cloudflare R2 for ROMs, GCS for everything else** — zero egress fees for large ROM files on non-GCP providers
5. **MongoDB for metadata only** — games collection and saves collection; files live in R2/GCS
6. **One Docker image per emulator** — shared GOW base, separate images for melonDS, Dolphin, PPSSPP, Azahar, Ryujinx, Steam
7. **Gamer Agent (FastAPI) on each VM** — receives session manifest, sets up rclone mounts, configures Wolf, reports events
8. **Main Server owns VM lifecycle** — Agent pushes events; Server makes all stop/destroy decisions

### Services Overview

| Service | Tech | Runs On | Purpose |
|---------|------|---------|---------|
| **Web App** | Next.js + Tailwind + shadcn | Cloud Run | Game library, save slots, ROM upload, session launcher, embedded streaming player |
| **Main Server** | FastAPI + MongoDB | Cloud Run | Session lifecycle, VM provisioning (TensorDock/GCP), VM health polling, game/save CRUD |
| **Gamer Agent** | FastAPI | Each GPU VM | Session manifest handling, rclone mounts, Wolf config, save slot management, health endpoint |
| **Wolf** | C++ (GOW project) | Each GPU VM | Moonlight protocol, Wayland compositor, NVENC encoding, Docker orchestration, virtual input |
| **moonlight-web-stream** | Rust (Helix fork) | Each GPU VM | Bridges Wolf Moonlight protocol → WebSocket for browser clients |
| **Emulator Containers** | Docker (per-emulator) | Each GPU VM | melonDS, Dolphin, PPSSPP, Azahar, Ryujinx, Steam — each with baked configs |

### External Services

| Service | Purpose |
|---------|---------|
| **Cloudflare R2** | ROM storage (zero egress fees) |
| **Google Cloud Storage** | Saves, configs, firmware, Steam files (with object versioning on saves) |
| **MongoDB Atlas** | Game metadata and save slot metadata |
| **TensorDock** | Primary GPU VM provider |
| **GCP Compute** | Fallback GPU VM provider |

## Technology Stack

### Frontend
- **Framework**: Next.js with App Router
- **Styling**: Tailwind CSS + shadcn/ui components
- **Authentication**: Google OAuth (NextAuth)
- **Streaming Player**: moonlight-web-stream TypeScript client (WebCodecs + Canvas)
- **Deployment**: Google Cloud Run

### Backend (Main Server)
- **Language**: Python 3.11+
- **Framework**: FastAPI
- **Database**: MongoDB Atlas (metadata only)
- **Cloud APIs**: TensorDock API, GCP Compute Engine API
- **Deployment**: Google Cloud Run

### VM Stack (Gamer Agent + Wolf)
- **Gamer Agent**: Python 3.11+ / FastAPI (systemd service)
- **Wolf**: `ghcr.io/games-on-whales/wolf:stable`
- **moonlight-web-stream**: Rust WebSocket server (Docker)
- **Storage Mounts**: rclone (R2 + GCS)
- **Time Spoofing**: libfaketime (for games like Pokemon)

### Emulators
- **DS**: melonDS (`ghcr.io/gamer/melonds:latest`)
- **GameCube/Wii**: Dolphin (`ghcr.io/gamer/dolphin:latest`)
- **PSP**: PPSSPP (`ghcr.io/gamer/ppsspp:latest`)
- **3DS**: Azahar (`ghcr.io/gamer/azahar:latest`)
- **Switch**: Ryujinx (`ghcr.io/gamer/ryujinx:latest`)
- **PC**: Steam (`ghcr.io/gamer/steam:latest`)

### Docker Image Hierarchy
```
ghcr.io/games-on-whales/base-app:edge     ← GOW base (Wayland, PulseAudio, GPU libs)
    └── ghcr.io/gamer/base:latest          ← Our base (+ libfaketime, common utils)
        ├── ghcr.io/gamer/melonds:latest
        ├── ghcr.io/gamer/dolphin:latest
        ├── ghcr.io/gamer/ppsspp:latest
        ├── ghcr.io/gamer/azahar:latest
        ├── ghcr.io/gamer/ryujinx:latest
        └── ghcr.io/gamer/steam:latest
```

## Storage Architecture

### Cloudflare R2 — ROMs (read-only from VMs)
```
r2://gamer-roms/{user_id}/
    ├── pokemon-black.nds
    ├── smash-melee.iso
    └── tears-of-the-kingdom.xci
```
- Zero egress regardless of VM provider
- rclone mount with `--read-only` and aggressive VFS cache

### GCS — Saves, Configs, Firmware, Steam (read-write)
```
gs://gamer-data/{user_id}/
    ├── saves/{game}/{slot}/        ← Save files (GCS object versioning enabled)
    ├── configs/{emulator}/         ← User config overrides (persist between sessions)
    ├── firmware/{platform}/        ← BIOS, firmware, keys (user-provided)
    └── steam/                      ← Steam installs, login tokens, saves
```

### rclone Mount Strategy (on VM)
```
R2 (ROMs)        → /mnt/roms/      → container:/home/retro/roms/      (ro)
GCS (saves)      → /mnt/saves/     → agent copies selected slot to /mnt/emusaves/
GCS (configs)    → /mnt/configs/   → container:/home/retro/config/    (rw)
GCS (firmware)   → /mnt/firmware/  → container:/home/retro/firmware/  (ro)
GCS (steam)      → /mnt/steam/     → container:/home/retro/.steam/    (rw)
```

Save files get special handling: agent copies selected slot to working dir before session, copies back after session ends. See ARCHITECTURE.md for details.

## Data Model (MongoDB)

### `games` Collection
```json
{
  "_id": "pokemon-black",
  "title": "Pokemon Black",
  "emulator": "melonds",
  "r2_rom_path": "roms/pokemon-black.nds",
  "emulator_image": "ghcr.io/gamer/melonds:latest",
  "default_config": { "screen_layout": 1 },
  "controller_override": null,
  "requires_firmware": true,
  "firmware_dir": "ds"
}
```

### `saves` Collection
```json
{
  "_id": "trainer-1-pokemon-black",
  "game_id": "pokemon-black",
  "user_id": "me",
  "name": "Trainer 1",
  "gcs_save_path": "saves/pokemon-black/trainer-1/pokemon-black.sav",
  "save_filename": "pokemon-black.sav",
  "fake_time": { "epoch": "2011-03-06T08:00:00", "accumulated_seconds": 14400 },
  "play_time_total_seconds": 14400,
  "last_played": "2026-02-05T22:30:00Z"
}
```

## Session Manifest

When user hits Play, Main Server builds a manifest sent to the Gamer Agent:
```json
{
  "session_id": "abc123",
  "user_id": "me",
  "vm_token": "xyz789",
  "emulator_image": "ghcr.io/gamer/melonds:latest",
  "rom_path": "roms/pokemon-black.nds",
  "save_path": "saves/pokemon-black/trainer-1/pokemon-black.sav",
  "save_filename": "pokemon-black.sav",
  "firmware_dir": "ds",
  "fake_time": "2011-03-06T12:00:00",
  "emulator_config": { "screen_layout": 1 },
  "resolution": "1920x1080",
  "fps": 60,
  "codec": "hevc",
  "client_cert": "-----BEGIN CERTIFICATE-----\n...",
  "dual_screen": { "enabled": true, "top": {...}, "bottom": {...} }
}
```

## VM Lifecycle

### Provisioning
User hits Play → Main Server provisions GPU VM → VM boots with pre-baked image containing Docker, rclone, Gamer Agent, Wolf + emulator images pre-pulled.

### GPU Tier Selection
```
melonDS, PPSSPP       → T4 (cheap)
Dolphin, Azahar       → T4 or L4
Ryujinx               → L4 or A10G (Switch is heavy)
Steam                  → L4 or A10G (depends on game)
```

### Lifecycle: Agent Push + Server Poll
- **Normal flow**: Agent → POST /session/{id}/started, /save_event, /idle → Server stops VM
- **Safety net**: Server polls agent /health every 15min; 8h hard cap; 48h stopped → destroy

### Gamer Agent Boot Sequence
1. VM boots → Agent starts (systemd)
2. `GET /api/session/{vm_token}` → receives manifest
3. Set up rclone mounts
4. Copy selected save slot to `/mnt/emusaves/`
5. Write Wolf `config.toml`
6. Start Wolf + moonlight-web-stream (docker compose)
7. Report session started
8. Watch saves with inotify (if fake_time game)
9. Monitor for idle (no clients 10min → report)
10. On shutdown: copy save back, wait for rclone flush, report ended

## Repository Structure

```
gamer/
├── ARCHITECTURE.md              # Canonical architecture reference (v3)
├── CLAUDE.md                    # Agent working summary (this file)
├── README.md                    # Quick start
├── docker-compose.yml           # Local dev orchestration
├── .env.example                 # Environment variable template
├── .likec4/                     # C4 architecture diagrams
│   ├── spec.likec4              # Element kinds and styles
│   ├── model.likec4             # System model
│   └── views.likec4             # Architecture views
├── services/
│   ├── web-app/                 # Next.js frontend
│   │   ├── app/
│   │   ├── components/
│   │   ├── lib/
│   │   ├── Dockerfile
│   │   └── package.json
│   ├── main-server/             # FastAPI main server (was provisioner-api + agent-api)
│   │   ├── app/
│   │   │   ├── main.py
│   │   │   ├── models/          # MongoDB/Pydantic models
│   │   │   ├── routers/         # API endpoints
│   │   │   ├── services/        # Business logic (VM provisioning, session management)
│   │   │   └── core/            # Config, database
│   │   ├── Dockerfile
│   │   └── requirements.txt
│   └── gamer-agent/             # FastAPI agent running on each VM
│       ├── app/
│       │   ├── main.py
│       │   ├── rclone.py        # Mount setup
│       │   ├── wolf_config.py   # Wolf config.toml generation
│       │   ├── save_manager.py  # Save slot copy logic
│       │   └── session.py       # Session lifecycle + inotify
│       ├── Dockerfile
│       └── requirements.txt
├── images/                      # Emulator Docker images
│   ├── base/                    # Our base (GOW base + libfaketime)
│   │   └── Dockerfile
│   ├── melonds/                 # DS emulator
│   │   ├── Dockerfile
│   │   ├── melonds-config/
│   │   └── entrypoint.sh
│   ├── dolphin/                 # GameCube/Wii emulator
│   ├── ppsspp/                  # PSP emulator
│   ├── azahar/                  # 3DS emulator
│   ├── ryujinx/                 # Switch emulator
│   └── steam/                   # Steam + Proton
├── moonlight-web-stream/        # Forked Helix WebSocket streaming bridge
│   ├── server/                  # Rust WebSocket server
│   └── client/                  # TypeScript browser client
├── infrastructure/
│   ├── vm-image/                # Pre-baked VM image (Packer/scripts)
│   └── terraform/               # GCP infrastructure
├── .github/
│   └── workflows/
│       ├── deploy-web-app.yml
│       ├── deploy-main-server.yml
│       ├── build-emulator-images.yml
│       └── build-vm-image.yml
└── docs/
    └── DEVELOPMENT.md
```

**Note on current state**: The existing `services/provisioner-api/` has partial implementation (TensorDock/GCP service scaffolds, geocoding, gaming router). This will be consolidated into `services/main-server/` as part of the restructure. The existing `services/agent-api/` and `services/client-agent/` are skeleton-only and will be replaced by `services/gamer-agent/`.

## Component Specifications

### Web App (Next.js)
**Responsibilities**:
- User authentication (Google OAuth)
- Game library browsing (reads from Main Server → MongoDB)
- Save slot management (create, rename, select)
- ROM upload (to Cloudflare R2 via presigned URLs)
- Session launcher (hit Play → Main Server provisions VM)
- Embedded streaming player (moonlight-web-stream TypeScript client)
- Dual-screen mode (two browser windows, each cropping to assigned screen)
- Reconnection UI (if browser tab closes, VM still running)

### Main Server (FastAPI)
**Responsibilities**:
- Game CRUD (MongoDB `games` collection)
- Save slot CRUD (MongoDB `saves` collection)
- Session manifest generation
- VM provisioning via TensorDock API / GCP Compute Engine API
- VM lifecycle management (start, stop, destroy)
- Session event handling (started, save_event, idle, ended)
- VM health polling (safety net — every 15min)
- Cost controls (8h hard cap, 48h stopped → destroy)

**Key Endpoints**:
- `POST /api/sessions/start` — Build manifest, provision VM, return session info
- `GET /api/sessions/{session_id}` — Session status
- `POST /api/sessions/{session_id}/stop` — Signal VM shutdown
- `GET /api/games` — List games from MongoDB
- `POST /api/games` — Add game to library
- `GET /api/saves/{game_id}` — List save slots
- `POST /api/saves` — Create save slot
- `POST /api/roms/upload-url` — Generate R2 presigned upload URL
- **Agent callbacks** (called by Gamer Agent):
  - `GET /api/session/{vm_token}` — Agent fetches manifest
  - `POST /api/session/{session_id}/started`
  - `POST /api/session/{session_id}/save_event`
  - `POST /api/session/{session_id}/idle`
  - `POST /api/session/{session_id}/ended`

### Gamer Agent (FastAPI on VM)
**Responsibilities**:
- Fetch session manifest from Main Server on boot
- Set up rclone mounts (R2 for ROMs, GCS for saves/configs/firmware/steam)
- Copy selected save slot to emulator working directory
- Generate Wolf `config.toml` from manifest
- Start Wolf + moonlight-web-stream containers
- Watch save directory with inotify (for fake_time games only)
- Report session events to Main Server
- Expose `/health` endpoint for server polling
- Copy save back to slot on session end

**Key Endpoints**:
- `GET /health` — Returns status, connected clients, idle_since, GPU utilization

### moonlight-web-stream (Helix Fork)
**Responsibilities**:
- Bridge Wolf's Moonlight protocol (RTSP + RTP) to WebSocket
- Extract H.264 NAL units from RTP stream → binary WebSocket frames
- Multi-client support (for dual-screen: two browsers, same Wolf session)
- Translate browser input events → Moonlight input protocol → Wolf

**Our additions to Helix fork**:
- Multi-client per session
- Crop config per client (for dual-screen)
- Touch coordinate translation (DS bottom screen)
- Audio routing (one device only)

## Dual-Screen Streaming (DS/3DS)

Wolf streams a combined frame (both screens in one image). Two browser clients each decode the full frame with WebCodecs, then crop to their assigned screen:

```
melonDS (top+bottom in one window)
    → Wolf compositor → single framebuffer → NVENC encode
    → moonlight-web-stream → WebSocket
    ├── Browser A (iPad): decode full frame, crop top 768px, D-pad input
    └── Browser B (iPhone): decode full frame, crop bottom 768px, touch input
```

Crop config comes from the session manifest's `dual_screen` field.

## libfaketime — In-Game Clock Spoofing

For games using real-time clock (Pokemon seasons, day/night cycles):
- Server stores per save slot: `epoch` (game world start date) + `accumulated_seconds` (total play time)
- On session start: `fake_current = epoch + accumulated_seconds`
- Gamer Agent passes `FAKETIME` env var to container
- libfaketime intercepts time calls, returns spoofed value advancing normally from that point
- Save events tracked via inotify on save file → server updates `accumulated_seconds`

Games that don't need fake time: Agent simply doesn't set the `FAKETIME` env var.

## Environment Variables

```bash
# Web App
NEXT_PUBLIC_MAIN_SERVER_URL=
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
NEXTAUTH_SECRET=
NEXTAUTH_URL=

# Main Server
MONGODB_ATLAS_URI=
TENSORDOCK_API_KEY=
TENSORDOCK_API_TOKEN=
GCP_PROJECT_ID=
GCP_BILLING_ACCOUNT=
R2_ACCOUNT_ID=
R2_ACCESS_KEY_ID=
R2_SECRET_ACCESS_KEY=
R2_BUCKET_NAME=gamer-roms
GCS_BUCKET_NAME=gamer-data

# Gamer Agent (set by VM provisioning)
MAIN_SERVER_URL=
VM_TOKEN=
```

## Development Setup

### Prerequisites
- Docker and Docker Compose
- Node.js 18+ (web app)
- Python 3.11+ (Main Server, Gamer Agent)
- MongoDB Atlas account (or local MongoDB)
- Cloudflare R2 account
- GCP account with Cloud Storage

### Local Development
```bash
cp .env.example .env
# Edit .env with your values
docker-compose up -d

# Web App
cd services/web-app && npm install && npm run dev  # http://localhost:3000

# Main Server
cd services/main-server && pip install -r requirements.txt
uvicorn app.main:app --reload --port 8001
```

### Health Checks
- Web App: http://localhost:3000
- Main Server: http://localhost:8001/health

## Deployment

### Cloud Run Services
- **Web App**: Next.js → Docker → Cloud Run
- **Main Server**: FastAPI → Docker → Cloud Run

### VM Image (Pre-baked)
Ubuntu 22.04 + NVIDIA driver + Docker + NVIDIA Container Toolkit + rclone + Gamer Agent + Wolf + all emulator images pre-pulled + moonlight-web-stream

### Cost Optimization
- Agent reports idle → Server stops VM immediately
- Server polls every 15min as safety net
- 8h hard cap per session
- 48h stopped → auto-destroy
- GPU tier selection per emulator (cheap T4 for DS, expensive L4 for Switch)
- R2 zero egress for ROMs to non-GCP providers

## Security Considerations

- Google OAuth for user authentication
- VM token for agent ↔ server auth
- Pre-shared client certificates for Wolf pairing (no PIN exchange on ephemeral VMs)
- User-provided firmware/BIOS stored in their GCS space only
- Container isolation via Docker (network_mode: host for game functionality)
- No credential logging

## Future Enhancements

- **MoQ (v2 transport)**: GStreamer moqsink → MoQ relay → WebTransport → browser (eliminates moonlight-web-stream middleman)
- **Native Moonlight support**: For Apple TV, Android TV (secondary client path)
- **Server-side dual-screen crop**: Two Wolf instances with GStreamer videocrop (for native Moonlight)
- **VM pre-warming pool**: Reduce cold start time from 2+ minutes
- **Steam pre-download**: `steamcmd` headless download before Wolf starts

## Documentation Requirements

**CRITICAL**: Every service folder MUST have a comprehensive README.md file that documents:
- Purpose and overview of the folder/service
- Detailed explanation of each function in every file
- Input parameters, return values, and error handling for each function
- Interaction patterns between functions and services
- API endpoints with request/response examples
- Configuration requirements and environment variables
- Usage examples and common patterns

**When to update READMEs**:
- Immediately after adding, modifying, or removing functions
- When changing API signatures or behavior
- When adding new files or restructuring folders
- When updating configuration or environment requirements

## Code Implementation Guidelines

**CRITICAL**: When implementing scaffolded functions that contain detailed comment outlines, NEVER delete the comments. Instead, place each code implementation directly below its corresponding comment. This preserves the original scaffolding structure and makes the code self-documenting.

**Example of CORRECT implementation**:
```python
def example_function():
    # Step 1: Get data from database
    data = database.collection.find_one({"id": user_id})

    # Step 2: Process the data
    processed = process_data(data)

    # Step 3: Return result
    return processed
```

**Example of INCORRECT implementation** (DO NOT DO THIS):
```python
def example_function():
    data = database.collection.find_one({"id": user_id})
    processed = process_data(data)
    return processed
```

This guideline ensures that future agents can understand both the original intent (comments) and the implementation (code) when maintaining or modifying functions.

## Windows Host Implementation (Apollo-first) — 2026-02-21

Branch: `windows-gaming-server-impl`

Implemented artifacts:
- `services/client-agent/src/main.py` — Windows-capable client-agent scaffold with hardcoded session manifest loading, `/start` `/stop` `/health` APIs.
- `services/client-agent/manifests/session_manifest.windows.dev.json` — hardcoded manifest for current server-not-ready phase.
- `infrastructure/windows/provision-tensordock-windows.py` — create/status/delete TensorDock Windows GPU VMs.
- `infrastructure/windows/bootstrap-windows.ps1` — bootstrap Apollo/rclone/ShaderGlass/AutoHotkey and folder layout.
- `infrastructure/windows/install-agent-service.ps1` — install/start client-agent service on Windows.
- `infrastructure/windows/scripts/*` — dual-screen placement helper + Apollo connect/disconnect hooks.
- `docs/windows/*` runbooks.

Design alignment with Linux side:
- Same manifest shape and session semantics (temporary hardcoded source).
- Same storage split assumptions (R2 ROMs + GCS saves/config/firmware/steam) via rclone.
- Same client-agent contract intent (Windows agent still talks to same server API once ready).

### Windows Bootstrap Validation Update — 2026-02-21

- Added `infrastructure/windows/rdp_bootstrap.py`:
  - Pure-Python RDP automation using `aardwolf` (no manual RDP client).
  - Launches elevated PowerShell, enables OpenSSH + WinRM, and opens firewall.
  - Validated on active TensorDock Windows VM (`66.172.10.81`): TCP 22 and 5985 reachable after bootstrap.

- Added `infrastructure/windows/deploy_via_ssh.py`:
  - Uploads Windows setup scripts + agent files via SFTP.
  - Executes `bootstrap-windows.ps1` and `install-agent-service.ps1` over SSH.
  - Validated end-to-end: scheduled task `GamerClientAgent` created and agent process listening on 8081.

- Hardened Windows scripts:
  - `bootstrap-windows.ps1` now tolerates winget/package failures and continues setup.
  - `install-agent-service.ps1` now:
    - resolves Python robustly,
    - falls back to NuGet Python package extraction when winget unavailable,
    - creates venv reliably,
    - opens inbound firewall rule for agent port.

### Windows Reliability Iteration — 2026-02-21 (continued)

- Added `infrastructure/windows/orchestrate_windows_host.py`:
  - one-shot pure-Python workflow: create/reuse VM → status/IP wait → RDP bootstrap → SSH deploy → `/health` validation.
  - supports no-token mode when reusing an existing state file with known IP.

- Added `infrastructure/windows/validate_windows_host.py`:
  - automated smoke/reliability checks for agent APIs, connect/disconnect semantics, and optional reboot persistence.
  - reboot verification now uses TCP down/up observation plus Win32 last-boot-time change confirmation.

- `deploy_via_ssh.py` improvements:
  - added `--skip-bootstrap` for fast script/agent iteration without reinstalling Apollo/ShaderGlass.
  - added `--skip-agent-install` for selective deploy flows.

- Agent + hooks hardening:
  - `/client-connected` and `/client-disconnected` now handle both absolute-count payloads and edge/event payloads robustly.
  - `/health` now returns:
    - process liveness,
    - started timestamp,
    - last PowerShell hook execution details (exit code/stdout/stderr) for fast debugging.
  - `apollo-on-client-connect.ps1` now spawns dual-window placement asynchronously so API calls return quickly.
  - `position-dual-now` now uses retry args by default.
  - PowerShell hook execution now records duration and supports timeout (`POWERSHELL_SCRIPT_TIMEOUT_SEC`, default 20s).
  - Added `POST /cleanup-processes` to prune stale exited process handles from in-memory agent state.
  - Added `POST /manifest-set` and `POST /manifest-clear` to support manifest injection before server API integration is finalized.

- Boot persistence validated:
  - startup task now runs as `SYSTEM` with restart policy and survives reboot without user logon.
