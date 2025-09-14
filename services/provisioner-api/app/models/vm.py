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

class ConsoleConfigDocument(BaseModel):
    console_type: ConsoleType
    supported_instance_types: Dict[str, List[str]]
    min_cpus: int
    min_ram: int
    min_disk: int

class VMAvailableResponse(BaseModel):
    provider: CloudProvider
    instance_type: Union[TensorDockVMType, GCPVMType]
    provider_id: Optional[str]
    hourly_price: Decimal
    instance_lat: float
    instance_long: float
    distance_to_user: float
    gpu: str
    avail_cpus: int
    avail_ram: int
    avail_disk: int

class VMCreateRequest(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    instance_type: Union[TensorDockVMType, GCPVMType]
    provider_id: Optional[str]
    provider_instance_name: str
    instance_lat: float
    instance_long: float
    os: str = "Ubuntu"
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
    provider_instance_id: Optional[str] = None
    provider_instance_name: str
    instance_type: Union[TensorDockVMType, GCPVMType]
    instance_lat: float
    instance_long: float
    hourly_price: Decimal
    os: str
    gpu: str
    num_cpus: int
    num_ram: int
    num_disk: int
    auto_stop_timeout: int
    ssh_key: str
    instance_password: str
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
    hourly_price: Decimal
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
    RTX5090 = "RTX5090"
    RTX4090 = "RTX4090"
    RTX3090 = "RTX3090"
    RTXA4000 = "RTXA4000"
    NOGPU = ""

class GCPVMType(str, Enum):
    E2_STANDARD_4 = "e2-standard-4"
    G2_STANDARD_4 = "g2-standard-4"
    G2_STANDARD_8 = "g2-standard-8"
    N1_STANDARD_4 = "n1-standard-4"

class TensorDockCreateRequest(BaseModel):
    password: str
    ssh_key: str
    vm_name: str = Field(alias="provider_instance_name")
    gpu_count: int = 1
    gpu_model: TensorDockVMType = Field(alias="instance_type")
    vcpus: int = Field(alias="num_cpus")
    ram: int = Field(alias="num_ram")
    external_ports: List[int] = [47984, 47989, 48010, 47998, 47999, 22, 443]  # Wolf/Moonlight + SSH + HTTPS
    internal_ports: List[int] = [47984, 47989, 48010, 47998, 47999, 22, 443]  # Same as external for direct mapping
    hostnode: str = Field(alias="provider_instance_id")
    storage: int = Field(alias="num_disk")
    operating_system: str = Field(alias="os")

class CloudyPadCreateRequest(BaseModel):
    name: str = Field(alias="provider_instance_name")
    instance_type: str = Field(alias="instance_type")
    disk_size: int = Field(alias="num_disk")
    public_ip_type: str = "static"
    region: str = Field(alias="provider_instance_id")
    spot: bool = True
    streaming_server: str = "wolf"
    cost_alert: int = 10
    cost_limit: int = 40
    cost_notification_email: str = "neil@tapiavala.com"

