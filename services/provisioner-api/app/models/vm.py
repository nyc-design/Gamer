from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List, Union
from enum import Enum
from datetime import datetime
from decimal import Decimal
from bson import ObjectId

class VMPreset(str, Enum):
    RETRO = "retro"      # NES/SNES/GB/GBA: 2 vCPU, 4GB RAM, no GPU
    ADVANCED = "advanced" # DS/3DS: 4 vCPU, 8GB RAM, basic GPU  
    PREMIUM = "premium"   # GC/Wii/Switch: 8 vCPU, 16GB RAM, high-end GPU

class VMStatus(str, Enum):
    CREATING = "creating"
    CONFIGURING = "configuring"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    STOPPED = "stopped"
    DESTROYING = "destroying"
    DESTROYED = "destroyed"
    ERROR = "error"

class CloudProvider(str, Enum):
    TENSORDOCK = "tensordock"
    GCP = "gcp"
    AWS = "aws" 
    AZURE = "azure"
    PAPERSPACE = "paperspace"
    SCALEWAY = "scaleway"

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

class OperatingSystems(str, Enum):
    Windows = "Windows"
    Ubuntu = "Ubuntu"

class GPUTypes(str, Enum):
    RTX5090 = "rtx5090"
    RTX4090 = "rtx4090"
    RTX3090 = "rtx3090"
    RTXA4000 = "rtxa4000"
    NoGPU = ""

class ConsoleConfigDocument(BaseModel):
    console_type: ConsoleType
    supported_gpus: List[GPUTypes]
    min_cpus: int
    min_ram: int
    min_disk: int

class VMAvailableResponse(BaseModel):
    provider: CloudProvider
    provider_id: str
    hourly_price: Decimal
    gpu: GPUTypes
    avail_cpus: int
    avail_ram: int
    avail_disk: int
    instance_lat: float
    instance_long: float
    distance_to_user: float

class VMCreateRequest(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    provider_id: str
    instance_name: str
    hourly_price: Decimal
    instance_lat: float
    instance_long: float
    operating_system: OperatingSystems
    gpu: GPUTypes
    num_cpus: Optional[int] = None
    num_ram: Optional[int] = None
    num_disk: Optional[int] = None
    auto_stop_timeout: int = 9000
    user_id: Optional[str] = None

# Simplified VM model - only essential fields
class VMDocument(BaseModel):
    id: Optional[ObjectId] = Field(None, alias="_id")
    vm_id: str
    status: VMStatus
    console_types: List[ConsoleType]
    provider: CloudProvider
    provider_id: Optional[str] = None
    instance_name: str
    hourly_price: Decimal
    instance_lat: float
    instance_long: float
    operating_system: OperatingSystems
    gpu: GPUTypes
    num_cpus: int
    num_ram: int
    num_disk: int
    auto_stop_timeout: int
    ssh_key: str
    instance_password: Optional[str]
    ip_address: str
    user_id: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    last_activity: Optional[datetime] = None

# Simplified response model
class VMResponse(BaseModel):
    vm_id: str
    status: VMStatus
    console_type: ConsoleType
    provider: CloudProvider
    hourly_price: Decimal
    created_at: datetime
    instance_lat: float
    instance_long: float
    operating_system: OperatingSystems
    gpu: GPUTypes
    last_activity: Optional[datetime] = None

# Simple status response
class VMStatusResponse(BaseModel):
    vm_id: str
    status: VMStatus
    ip_address: Optional[str] = None
    last_activity: Optional[datetime] = None

class GCPVMType(str, Enum):
    E2_STANDARD_4 = "e2-standard-4"
    G2_STANDARD_4 = "g2-standard-4"
    G2_STANDARD_8 = "g2-standard-8"
    N1_STANDARD_4 = "n1-standard-4"

class TensorDockCreateRequest(BaseModel):
    password: str
    ssh_key: str
    location_id: str = Field(alias="provider_id")
    name: str = Field(alias="instance_name")
    gpu_count: int = 0
    gpu_model: GPUTypes
    vcpu_count: int = Field(alias="num_cpus")
    ram_gb: int = Field(alias="num_ram")
    storage_gb: int = Field(alias="num_disk", default=100)
    image: str = Field(default = "ubuntu2404")
    portforwards: List[int] = [47984, 47989, 48010, 47998, 47999, 22, 443]

class GCPCreateRequest(BaseModel):
    ssh_key: str
    zone: str
    machine_type: GCPVMType
    name: str = Field(alias="instance_name")
    gpu_count: int = 0
    gpu_type: GPUTypes
    disk_size_gb: int = Field(alias="num_disk", default=100)
    disk_type: str = "pd-ssd"
    source_image: str = Field(default = "projects/ubuntu-os-cloud/global/images/family/ubuntu-2404-lts")
    network: str = "global/networks/default"
    external_ip: bool = True
    preemptible: bool = True


