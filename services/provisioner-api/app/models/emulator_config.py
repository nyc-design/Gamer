from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List
from enum import Enum
from datetime import datetime
from bson import ObjectId
from app.models.vm import ConsoleType, CloudProvider, VMPreset

class ProviderPriority(int, Enum):
    PRIMARY = 1
    SECONDARY = 2
    FALLBACK = 3
    DISABLED = 99

class EmulatorConfigDocument(BaseModel):
    id: Optional[ObjectId] = Field(None, alias="_id")
    console_type: ConsoleType = Field(..., description="Console type identifier")
    display_name: str
    description: Optional[str] = None
    
    # Provider configuration
    preferred_providers: List[Dict[str, Any]] = []  # Ordered by priority
    
    # VM specifications
    default_preset: VMPreset
    min_cpu: int = 2
    min_ram_gb: int = 4
    requires_gpu: bool = False
    
    # Cost optimization
    cost_per_hour_limit: Optional[float] = None  # Max cost per hour
    max_session_hours: int = 8  # Auto-stop after X hours
    
    # Gaming configuration
    default_emulator: str
    supported_file_extensions: List[str] = []
    save_file_extensions: List[str] = []
    
    # System requirements
    system_requirements: Dict[str, Any] = {}
    
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    
    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True
        json_encoders = {
            ObjectId: str
        }

class ProviderConfigRequest(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    priority: ProviderPriority
    preset_override: Optional[VMPreset] = None
    cost_per_hour_limit: Optional[float] = None
    enabled: bool = True

class ProviderConfigResponse(BaseModel):
    console_type: ConsoleType
    provider: CloudProvider
    priority: ProviderPriority
    preset: VMPreset
    cost_per_hour_limit: Optional[float] = None
    enabled: bool
    estimated_cost_per_hour: Optional[float] = None

class EmulatorConfigRequest(BaseModel):
    console_type: ConsoleType
    display_name: str
    description: Optional[str] = None
    default_preset: VMPreset
    min_cpu: int = 2
    min_ram_gb: int = 4
    requires_gpu: bool = False
    preferred_providers: List[ProviderConfigRequest] = []
    cost_per_hour_limit: Optional[float] = None
    max_session_hours: int = 8
    default_emulator: str
    supported_file_extensions: List[str] = []
    save_file_extensions: List[str] = []

class EmulatorConfigResponse(BaseModel):
    console_type: ConsoleType
    display_name: str
    description: Optional[str] = None
    default_preset: VMPreset
    min_cpu: int
    min_ram_gb: int
    requires_gpu: bool
    preferred_providers: List[ProviderConfigResponse] = []
    cost_per_hour_limit: Optional[float] = None
    max_session_hours: int
    default_emulator: str
    supported_file_extensions: List[str]
    save_file_extensions: List[str]
    created_at: datetime
    updated_at: datetime