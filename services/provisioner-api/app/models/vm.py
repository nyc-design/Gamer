from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List, Union
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
    DESTROYING = "destroying"
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

class VMAvailableResponse(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    hourly_price: decimal
    instance_lat: float
    instance_long: float
    gpu: str
    num_cpus: int
    num_ram: int
    num_disk: int = 20
    auto_stop_timeout: int = 9000

class VMCreateRequest(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    instance_lat: float
    instance_long: float
    os: str = "Ubuntu"
    gpu: str
    num_cpus: int
    num_ram: int
    num_disk: int = 20
    auto_stop_timeout: int = 9000
    user_id: Optional[str] = None

# Simplified VM model - only essential fields
class VMDocument(BaseModel):
    id: Optional[ObjectId] = Field(None, alias="_id")
    vm_id: str
    status: VMStatus
    console_types: List[ConsoleType]
    provider: CloudProvider
    provider_instance_id: Optional[str] = None
    instance_type: Union[TensorDockVMType, GCPVMType]
    instance_lat: float
    instance_long: float
    hourly_price: decimal
    os: str
    gpu: str
    num_cpus: int
    num_ram: int
    num_disk: int
    auto_stop_timeout: int
    ip_address: Optional[str] = None
    user_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    last_activity: Optional[datetime] = None

# Simplified response model
class VMResponse(BaseModel):
    vm_id: str
    status: VMStatus
    console_type: ConsoleType
    provider: CloudProvider
    instance_type: Union[TensorDockVMType, GCPVMType]
    hourly_price: decimal
    created_at: datetime
    instance_lat: float
    instance_long: float
    last_activity: Optional[datetime] = None

# Simple status response
class VMStatusResponse(BaseModel):
    vm_id: str
    status: VMStatus
    ip_address: Optional[str] = None
    last_activity: Optional[datetime] = None

class TensorDockVMType(str, Enum):
    RTX5090: "RTX5090"
    RTX4090: "RTX4090"
    RTX3090: "RTX3090"
    RTXA4000: "RTXA4000"
    NOGPU: ""

class GCPVMType(str, Enum):
    E2-STANDARD-4: "e2-standard-4"
    G2-STANDARD-4: "g2-standard-4"
    G2-STANDARD-8: "g2-standard-8"
    N1-STANDARD-4: "n1-standard-4"
