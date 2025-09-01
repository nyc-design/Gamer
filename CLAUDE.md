# Gamer - Cloud Gaming Platform

A cloud-based emulation gaming platform that allows users to launch remote gaming VMs and stream Nintendo console games through Moonlight, with automatic VM provisioning, game management, and save file synchronization.

## Project Overview

**Purpose**: Enable users to play emulated Nintendo games on remote VMs using Moonlight streaming through a web interface.

**Core Flow**:
1. User logs into web app
2. User selects game/console → triggers VM provisioning with appropriate specs
3. Web app communicates with provisioner API to deploy VMs via TensorDock API or CloudyPad CLI
4. VM deploys with CloudyPad base image + Wolf + custom client agent
5. User pairs Moonlight with VM's Wolf instance
6. User selects games/saves through web app or streamed VM interface (RomM)
7. Agent API notifies client agent to launch selected game
8. User plays game through Moonlight stream

## Architecture

### Services Overview
- **Web App** (Next.js + Tailwind + shadcn): User interface and authentication
- **Provisioner API** (FastAPI): VM lifecycle management via TensorDock/CloudyPad
- **Agent API** (FastAPI): Game launch coordination and VM communication
- **Client Agent** (Python): On-VM game launcher and system coordinator
- **RomM**: ROM/save file management and metadata
- **Wolf**: Game streaming server on VMs
- **CloudyPad**: Base VM image with gaming optimizations

## Technology Stack

### Frontend
- **Framework**: Next.js with App Router
- **Styling**: Tailwind CSS with custom shadcn components
- **Authentication**: Google OAuth
- **Deployment**: Google Cloud Run

### Backend APIs
- **Language**: Python 3.11+
- **Framework**: FastAPI
- **Database**: Google Cloud Storage (mounted as needed)
- **Deployment**: Google Cloud Run (containerized)

### Infrastructure
- **Cloud Providers**: TensorDock (primary), GCP (fallback)
- **VM Base Image**: CloudyPad + Wolf + custom agent
- **Game Storage**: GCS bucket mounted to VMs
- **Save Storage**: RomM API + GCS backup
- **Streaming**: Wolf + Moonlight

### Emulation Support
- **NES/SNES**: Built into CloudyPad base
- **Game Boy/Color/Advance**: mGBA
- **Nintendo DS**: melonDS  
- **Nintendo 3DS**: Citra/Azahar
- **GameCube/Wii**: Dolphin
- **Nintendo Switch**: TBD emulator

## Repository Structure

```
gamer/
├── .github/
│   └── workflows/
│       ├── deploy-web-app.yml
│       ├── deploy-provisioner-api.yml
│       ├── deploy-agent-api.yml
│       └── build-vm-image.yml
├── services/
│   ├── web-app/                 # Next.js frontend
│   │   ├── app/
│   │   ├── components/
│   │   ├── lib/
│   │   ├── Dockerfile
│   │   └── package.json
│   ├── provisioner-api/         # VM provisioning service
│   │   ├── app/
│   │   ├── models/
│   │   ├── routers/
│   │   ├── Dockerfile
│   │   └── requirements.txt
│   ├── agent-api/              # Game coordination service
│   │   ├── app/
│   │   ├── models/
│   │   ├── routers/
│   │   ├── Dockerfile
│   │   └── requirements.txt
│   └── client-agent/           # On-VM game launcher
│       ├── src/
│       ├── config/
│       ├── Dockerfile
│       └── requirements.txt
├── infrastructure/
│   ├── vm-image/               # Custom Wolf app image
│   │   ├── Dockerfile
│   │   ├── emulators/
│   │   └── configs/
│   └── terraform/              # GCP infrastructure
├── docs/
└── CLAUDE.md
```

## Component Specifications

### Web App (Next.js)
**Responsibilities**:
- User authentication (Google OAuth)
- Game library browsing via RomM integration
- VM management dashboard
- Moonlight pairing instructions
- Save file management interface

**Key Features**:
- Dynamic VM preset selection based on chosen game/console
- Real-time VM status monitoring
- Save file upload/download/versioning
- Game metadata display from RomM

### Provisioner API (FastAPI)
**Responsibilities**:
- VM lifecycle management (create, start, stop, delete)
- TensorDock API integration
- CloudyPad CLI orchestration
- Cost monitoring and alerts

**Key Endpoints**:
- `POST /vms/create` - Provision new gaming VM
- `GET /vms/{vm_id}/status` - Check VM status
- `POST /vms/{vm_id}/stop` - Stop VM
- `DELETE /vms/{vm_id}` - Terminate VM

**VM Presets**:
- **Retro** (NES/SNES/GB/GBA): 2 vCPU, 4GB RAM, no GPU
- **Advanced** (DS/3DS): 4 vCPU, 8GB RAM, basic GPU
- **Premium** (GC/Wii/Switch): 8 vCPU, 16GB RAM, high-end GPU

### Agent API (FastAPI)
**Responsibilities**:
- Game launch coordination
- VM-to-backend communication
- Save file synchronization triggers
- RomM API integration for metadata

**Key Endpoints**:
- `POST /games/launch` - Launch game on specific VM
- `POST /saves/sync` - Sync save files
- `GET /games/library` - Get user's game library
- `POST /vms/register` - Register new VM agent

### Client Agent (Python)
**Responsibilities**:
- Emulator process management
- Save file monitoring and upload
- System resource monitoring for autostop
- Communication with Agent API

**Key Functions**:
- Monitor save file changes and trigger sync
- Launch games with correct emulator and settings
- Report VM activity to prevent premature shutdown
- Handle graceful game termination

## Storage Architecture

### Game Files
- **Storage**: GCS bucket mounted to VM at `/games`
- **Organization**: Follow RomM structure for metadata compatibility
- **Access**: Direct filesystem access for performance

### Save Files
- **Primary**: RomM API for organization and versioning
- **Backup**: GCS bucket for redundancy
- **Sync**: Real-time on save file modification
- **Versioning**: Automatic timestamped backups

### Directory Structure
```
/games/
├── nes/
├── snes/
├── gb/
├── gbc/
├── gba/
├── nds/
├── 3ds/
├── gamecube/
├── wii/
└── switch/

/saves/
├── nes/
├── snes/
└── [same structure as games]
```

## VM Configuration

### Base Image Components
- **CloudyPad Base**: Ubuntu + gaming optimizations
- **Wolf**: Streaming server configured for Moonlight
- **Emulators**: Pre-installed and configured
- **Client Agent**: Python service for game coordination
- **GCS Mount**: Automatic game library mounting

### Autostop Configuration
- **Trigger**: No Moonlight connections + no game processes
- **Timeout**: 15 minutes of inactivity
- **Implementation**: CloudyPad's built-in autostop service
- **Monitoring**: Network activity, process monitoring, Moonlight port 47999

## Integration Details

### CloudyPad Integration
- Use CLI commands for VM provisioning
- Leverage base image with Wolf pre-configured  
- Utilize autostop functionality (verify TensorDock compatibility)
- Custom app image with emulators and client agent

### Wolf Integration
- Streaming configuration for optimal Nintendo emulation
- Multi-user support for future expansion
- Custom app deployment with emulator collection
- Moonlight client pairing flow

### RomM Integration
- API authentication with generated tokens
- Game metadata and artwork fetching
- Save file organization and versioning
- Library sharing and permissions

## Development Setup

### Prerequisites
- Docker and Docker Compose
- Google Cloud SDK
- Node.js 18+ and Python 3.11+
- Access to TensorDock API and GCP

### Local Development
```bash
# Clone and setup
git clone <repo-url>
cd gamer

# Start local services
docker-compose up -d

# Install dependencies
cd services/web-app && npm install
cd ../provisioner-api && pip install -r requirements.txt
cd ../agent-api && pip install -r requirements.txt
```

### Environment Variables
```bash
# Web App
NEXT_PUBLIC_PROVISIONER_API_URL=
NEXT_PUBLIC_AGENT_API_URL=
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=

# APIs
GCS_BUCKET_NAME=
ROMM_API_URL=
ROMM_API_TOKEN=
TENSORDOCK_API_KEY=
CLOUDYPAD_CONFIG=
```

## Deployment

### Cloud Run Services
Each service deploys independently with GitHub Actions:

1. **Web App**: Build Next.js → Docker → Cloud Run
2. **APIs**: Build FastAPI → Docker → Cloud Run  
3. **VM Image**: Build custom Wolf app → Container Registry

### Cost Optimization
- VM autostop after 15min inactivity
- Spot instances where available (90% savings)
- Automated cost alerts and limits
- Efficient game storage with GCS lifecycle policies

## Security Considerations

- Google OAuth for authentication
- API key rotation for service communication
- No credential logging or exposure
- VM isolation and security groups
- Save file access scoped to authenticated users

## Monitoring & Observability

- Cloud Run service metrics
- VM resource utilization
- Game launch success rates
- Save file sync status
- Cost tracking and alerts

## Future Enhancements

- Multi-user session support
- Additional console support
- Save state management
- Achievement integration
- Social features and game sharing