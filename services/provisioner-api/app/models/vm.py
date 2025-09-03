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

# Simplified VM model - only essential fields
class VMDocument(BaseModel):
    id: Optional[ObjectId] = Field(None, alias="_id")
    vm_id: str
    status: VMStatus
    console_type: ConsoleType
    provider: CloudProvider
    provider_instance_id: Optional[str] = None  # Provider's internal VM ID
    ip_address: Optional[str] = None
    user_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    last_activity: Optional[datetime] = None
    
    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True
        json_encoders = {ObjectId: str}

# Simplified response model
class VMResponse(BaseModel):
    vm_id: str
    status: VMStatus
    console_type: ConsoleType
    provider: CloudProvider
    ip_address: Optional[str] = None
    created_at: datetime
    last_activity: Optional[datetime] = None

# Simple status response
class VMStatusResponse(BaseModel):
    vm_id: str
    status: VMStatus
    ip_address: Optional[str] = None
    last_activity: Optional[datetime] = None