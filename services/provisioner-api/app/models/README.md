# Data Models and Schemas

## Overview
The models directory contains Pydantic models and Beanie documents that define the data structures used throughout the Provisioner API. These models handle data validation, serialization, and database operations with type safety and automatic validation.

## Architecture
- **Beanie Documents**: MongoDB document models with ODM functionality
- **Pydantic Models**: Request/response schemas for API validation
- **Enums**: Type-safe constants for VM configurations and statuses
- **Field Validation**: Automatic validation with custom validators

## Files

### vm.py
Defines virtual machine data structures, enums, and database documents.

#### Enums

#### VMPreset(str, Enum)
Predefined VM configuration templates for different gaming requirements.

- **RETRO = "retro"**: NES/SNES/GB/GBA consoles
  - 2 vCPU, 4GB RAM, no GPU required
  - Suitable for older, low-resource games
- **ADVANCED = "advanced"**: DS/3DS consoles  
  - 4 vCPU, 8GB RAM, basic GPU
  - Handles 3D graphics and dual screens
- **PREMIUM = "premium"**: GC/Wii/Switch consoles
  - 8 vCPU, 16GB RAM, high-end GPU
  - Required for modern console emulation

#### VMStatus(str, Enum)
VM lifecycle states for tracking provisioning and runtime status.

- **CREATING = "creating"**: Initial provisioning in progress
- **RUNNING = "running"**: VM active and ready for gaming
- **STOPPED = "stopped"**: VM powered down but preserving data
- **ERROR = "error"**: Provisioning or runtime failure state
- **TERMINATED = "terminated"**: VM destroyed and resources released
- **CONFIGURING = "configuring"**: Gaming environment setup in progress

#### CloudProvider(str, Enum)
Supported cloud infrastructure providers.

- **TENSORDOCK = "tensordock"**: TensorDock marketplace VMs
- **CLOUDYPAD_GCP = "cloudypad_gcp"**: CloudyPad on Google Cloud Platform
- **CLOUDYPAD_AWS = "cloudypad_aws"**: CloudyPad on Amazon Web Services
- **CLOUDYPAD_AZURE = "cloudypad_azure"**: CloudyPad on Microsoft Azure
- **CLOUDYPAD_PAPERSPACE = "cloudypad_paperspace"**: CloudyPad on Paperspace
- **CLOUDYPAD_SCALEWAY = "cloudypad_scaleway"**: CloudyPad on Scaleway

#### ConsoleType(str, Enum)
Nintendo console types supported by the gaming platform.

- **NES = "nes"**: Nintendo Entertainment System
- **SNES = "snes"**: Super Nintendo Entertainment System  
- **GB = "gb"**: Game Boy
- **GBC = "gbc"**: Game Boy Color
- **GBA = "gba"**: Game Boy Advance
- **NDS = "nds"**: Nintendo DS
- **N3DS = "3ds"**: Nintendo 3DS
- **GAMECUBE = "gamecube"**: Nintendo GameCube
- **WII = "wii"**: Nintendo Wii
- **SWITCH = "switch"**: Nintendo Switch

#### Request/Response Models

#### VMCreateRequest(BaseModel)
API request model for creating new VMs.

**Fields:**
- **console_type**: `ConsoleType` - Target console for emulation
- **game_id**: `Optional[str] = None` - Specific game identifier for auto-optimization
- **provider**: `Optional[CloudProvider] = None` - Cloud provider preference (auto-selected if None)
- **auto_stop_timeout**: `int = 900` - VM auto-stop timeout in seconds (default 15 minutes)
- **user_id**: `Optional[str] = None` - User identifier for VM ownership
- **user_location**: `Optional[Dict[str, float]] = None` - GPS coordinates as `{"latitude": float, "longitude": float}`

**Validation:**
- Ensures console_type is supported
- Validates user_location coordinates are within valid GPS ranges
- Auto-stop timeout must be positive integer

#### VMDocument(Document)
MongoDB document model for VM persistence using Beanie ODM.

**Fields:**

**Core Identification:**
- **vm_id**: `str` (unique) - Unique VM identifier
- **status**: `VMStatus` - Current VM state
- **preset**: `VMPreset` - VM resource configuration
- **console_type**: `ConsoleType` - Target console
- **provider**: `CloudProvider` - Cloud infrastructure provider

**Timestamps:**
- **created_at**: `datetime` (default: utcnow) - VM creation timestamp
- **updated_at**: `datetime` (default: utcnow) - Last modification timestamp

**Network Configuration:**
- **ip_address**: `Optional[str] = None` - VM public IP address
- **wolf_port**: `int = 47999` - Wolf streaming server port
- **ssh_port**: `int = 22` - SSH access port
- **ssh_private_key**: `Optional[str] = None` - SSH key for VM access

**VM Configuration:**
- **auto_stop_timeout**: `int = 900` - Inactivity timeout in seconds
- **user_id**: `Optional[str] = None` - Owning user identifier

**Provider Integration:**
- **provider_instance_id**: `Optional[str] = None` - Provider's internal VM identifier
- **provider_metadata**: `Dict[str, Any] = {}` - Provider-specific data storage

**Gaming Environment:**
- **gaming_environment_ready**: `bool = False` - CloudyPad/Wolf setup complete
- **cloudypad_configured**: `bool = False` - CloudyPad deployment status
- **games_mounted**: `bool = False` - Game library mount status

**Activity Tracking:**
- **last_activity**: `Optional[datetime] = None` - Last user interaction
- **last_moonlight_connection**: `Optional[datetime] = None` - Last streaming session
- **user_location**: `Optional[Dict[str, float]] = None` - User GPS coordinates for optimization

**Database Configuration:**
```python
class Settings:
    name = "vm_instances"  # MongoDB collection name
```

#### VMResponse(BaseModel)
API response model for VM data.

**Fields:** (Subset of VMDocument for API responses)
- **vm_id**: `str` - VM identifier
- **status**: `VMStatus` - Current state
- **preset**: `VMPreset` - Resource configuration
- **console_type**: `ConsoleType` - Target console
- **provider**: `CloudProvider` - Infrastructure provider
- **created_at**: `datetime` - Creation timestamp
- **updated_at**: `datetime` - Last update timestamp
- **ip_address**: `Optional[str] = None` - Public IP if available
- **wolf_port**: `int = 47999` - Streaming port
- **ssh_port**: `int = 22` - SSH port
- **auto_stop_timeout**: `int` - Timeout configuration
- **gaming_environment_ready**: `bool = False` - Ready status
- **last_activity**: `Optional[datetime] = None` - Activity timestamp

#### VMStatusResponse(BaseModel)
Lightweight status-only response model.

**Fields:**
- **vm_id**: `str` - VM identifier
- **status**: `VMStatus` - Current state
- **ip_address**: `Optional[str] = None` - Public IP
- **uptime_seconds**: `Optional[int] = None` - Runtime duration
- **last_activity**: `Optional[datetime] = None` - Activity timestamp
- **gaming_environment_ready**: `bool = False` - Environment status

### emulator_config.py
Console-specific configuration and provider preferences.

#### EmulatorConfigDocument(Document)
MongoDB document for managing console emulation requirements and provider preferences.

**Fields:**

**Console Configuration:**
- **console_type**: `ConsoleType` (unique) - Target console identifier
- **preferred_providers**: `List[Dict[str, Any]] = []` - Ordered list of provider configurations
- **default_preset**: `VMPreset` - Default VM configuration for this console
- **cost_per_hour_limit**: `Optional[float] = None` - Maximum acceptable cost per hour

**Hardware Requirements:**
- **min_cpu**: `int` - Minimum CPU cores required
- **min_ram_gb**: `int` - Minimum RAM in gigabytes
- **requires_gpu**: `bool` - GPU requirement flag
- **max_session_hours**: `int` - Maximum session duration

**Database Configuration:**
```python
class Settings:
    name = "emulator_configs"  # MongoDB collection name
```

**Provider Configuration Format:**
```python
preferred_providers = [
    {
        "provider": "tensordock",
        "priority": 1,
        "enabled": True,
        "preset_override": "premium",  # Optional override
        "cost_per_hour_limit": 2.50,
        "regions": ["us-central", "us-east"]  # Optional region restrictions
    },
    {
        "provider": "cloudypad_gcp", 
        "priority": 2,
        "enabled": True,
        "preset_override": None,
        "cost_per_hour_limit": 3.00
    }
]
```

## Data Flow Patterns

### VM Creation Flow
1. **API Request**: Client sends `VMCreateRequest` with console type and user location
2. **Config Lookup**: System queries `EmulatorConfigDocument` for console requirements
3. **Provider Selection**: Algorithm selects optimal provider based on priority and availability
4. **VM Creation**: `VMDocument` created with initial status `CREATING`
5. **Provisioning**: Background task provisions VM through selected provider
6. **Status Updates**: VM status progresses through `CREATING` → `CONFIGURING` → `RUNNING`

### Location Optimization
1. **User Location**: Browser provides GPS coordinates via geolocation API
2. **Provider Logic**:
   - **TensorDock**: Find closest hostnode using distance calculation
   - **CloudyPad**: Select optimal GCP region using Location Finder API
3. **Optimization**: VM created in geographically closest location for minimal latency

### Configuration Management
1. **Console Profiles**: Each console type has specific requirements in `EmulatorConfigDocument`
2. **Provider Fallback**: Multiple providers configured with priority ordering
3. **Cost Controls**: Per-provider and per-console cost limits enforced
4. **Dynamic Updates**: Configuration changes propagate to new VM creation without restart

## Database Relationships

### Collections
- **vm_instances**: VM lifecycle and metadata
- **emulator_configs**: Console-specific configuration

### Indexes (Recommended)
```python
# VMDocument indexes
vm_id (unique)
status + user_id (compound)
console_type + provider (compound)
created_at (TTL for cleanup)

# EmulatorConfigDocument indexes  
console_type (unique)
```

## Validation and Error Handling

### Field Validation
- **Enum Values**: Automatic validation of enum choices
- **GPS Coordinates**: Range validation for latitude/longitude
- **Timeouts**: Positive integer validation
- **URLs/IPs**: Format validation where applicable

### Custom Validators
```python
@validator('user_location')
def validate_coordinates(cls, v):
    if v:
        lat, lon = v.get('latitude'), v.get('longitude') 
        if not (-90 <= lat <= 90 and -180 <= lon <= 180):
            raise ValueError('Invalid GPS coordinates')
    return v
```

### Error Responses
- **ValidationError**: Invalid request data with field-specific messages
- **NotFound**: VM or configuration not found
- **ConflictError**: Duplicate VM IDs or constraint violations

## Usage Examples

### Creating VM Request
```python
request = VMCreateRequest(
    console_type=ConsoleType.SWITCH,
    game_id="mario-odyssey",
    user_id="user123",
    user_location={"latitude": 32.7767, "longitude": -96.7970},
    auto_stop_timeout=1800  # 30 minutes
)
```

### VM Document Operations
```python
# Create new VM
vm = VMDocument(
    vm_id=str(uuid.uuid4()),
    status=VMStatus.CREATING,
    preset=VMPreset.PREMIUM,
    console_type=ConsoleType.SWITCH,
    provider=CloudProvider.TENSORDOCK,
    user_id="user123"
)
await vm.insert()

# Query VMs
running_vms = await VMDocument.find(
    VMDocument.status == VMStatus.RUNNING
).to_list()

# Update status
await VMDocument.find_one(
    VMDocument.vm_id == "vm-123"
).update({"$set": {"status": VMStatus.RUNNING}})
```

### Configuration Management
```python
# Get console configuration
config = await EmulatorConfigDocument.find_one(
    EmulatorConfigDocument.console_type == ConsoleType.SWITCH
)

# Update provider preferences
await config.update({"$set": {
    "preferred_providers": updated_providers
}})
```