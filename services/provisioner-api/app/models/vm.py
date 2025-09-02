from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List
from enum import Enum
from datetime import datetime
from bson import ObjectId

class VMPreset(str, Enum):
    RETRO = "retro"      # NES/SNES/GB/GBA: 2 vCPU, 4GB RAM, no GPU
    ADVANCED = "advanced" # DS/3DS: 4 vCPU, 8GB RAM, basic GPU  
    PREMIUM = "premium"   # GC/Wii/Switch: 8 vCPU, 16GB RAM, high-end GPU

class VMStatus(str, Enum):
    CREATING = "creating"
    RUNNING = "running"
    STOPPED = "stopped"
    ERROR = "error"
    TERMINATED = "terminated"
    CONFIGURING = "configuring"  # Setting up gaming environment

class CloudProvider(str, Enum):
    TENSORDOCK = "tensordock"
    CLOUDYPAD_GCP = "cloudypad_gcp"
    CLOUDYPAD_AWS = "cloudypad_aws" 
    CLOUDYPAD_AZURE = "cloudypad_azure"
    CLOUDYPAD_PAPERSPACE = "cloudypad_paperspace"
    CLOUDYPAD_SCALEWAY = "cloudypad_scaleway"

class ConsoleType(str, Enum):
    NES = "nes"
    SNES = "snes"
    GB = "gb"
    GBC = "gbc" 
    GBA = "gba"
    NDS = "nds"
    N3DS = "3ds"
    GAMECUBE = "gamecube"
    WII = "wii"
    SWITCH = "switch"

class VMCreateRequest(BaseModel):
    console_type: ConsoleType
    game_id: Optional[str] = None  # For automatic VM selection
    provider: Optional[CloudProvider] = None  # Auto-select if None
    auto_stop_timeout: int = 900  # 15 minutes default
    user_id: Optional[str] = None
    user_location: Optional[Dict[str, float]] = None  # {"latitude": float, "longitude": float}

class VMDocument(BaseModel):
    id: Optional[ObjectId] = Field(None, alias="_id")
    vm_id: str = Field(..., description="Unique VM identifier")
    status: VMStatus
    preset: VMPreset
    console_type: ConsoleType
    provider: CloudProvider
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    
    # Connection details
    ip_address: Optional[str] = None
    wolf_port: int = 47999
    ssh_port: int = 22
    ssh_private_key: Optional[str] = None
    
    # Configuration
    auto_stop_timeout: int = 900
    user_id: Optional[str] = None
    
    # Provider-specific data
    provider_instance_id: Optional[str] = None  # TensorDock/CloudyPad instance ID
    provider_metadata: Dict[str, Any] = {}
    
    # Gaming setup
    gaming_environment_ready: bool = False
    cloudypad_configured: bool = False
    games_mounted: bool = False
    
    # Activity tracking
    last_activity: Optional[datetime] = None
    last_moonlight_connection: Optional[datetime] = None
    user_location: Optional[Dict[str, float]] = None  # {"latitude": float, "longitude": float}
    
    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True
        json_encoders = {
            ObjectId: str
        }

class VMResponse(BaseModel):
    vm_id: str
    status: VMStatus
    preset: VMPreset
    console_type: ConsoleType
    provider: CloudProvider
    created_at: datetime
    updated_at: datetime
    ip_address: Optional[str] = None
    wolf_port: int = 47999
    ssh_port: int = 22
    auto_stop_timeout: int
    gaming_environment_ready: bool = False
    last_activity: Optional[datetime] = None

class VMStatusResponse(BaseModel):
    vm_id: str
    status: VMStatus
    ip_address: Optional[str] = None
    uptime_seconds: Optional[int] = None
    last_activity: Optional[datetime] = None
    gaming_environment_ready: bool = False