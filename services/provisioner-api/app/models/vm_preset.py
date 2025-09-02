from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List
from datetime import datetime
from bson import ObjectId
from app.models.vm import VMPreset

class VMSpecDocument(BaseModel):
    """VM specification document stored in MongoDB"""
    id: Optional[ObjectId] = Field(None, alias="_id")
    preset: VMPreset = Field(..., description="Preset identifier")
    display_name: str
    description: Optional[str] = None
    
    # Hardware specifications
    cpu_cores: int
    ram_gb: int
    storage_gb: int = 50
    requires_gpu: bool = False
    gpu_memory_gb: Optional[int] = None
    
    # Cost information
    estimated_cost_per_hour_usd: float
    
    # Performance characteristics
    suitable_consoles: List[str] = []  # Console types this preset works well for
    performance_tier: str = Field(..., description="low, medium, high, premium")
    
    # Provider-specific configurations
    tensordock_config: Dict[str, Any] = {}
    gcp_config: Dict[str, Any] = {}
    aws_config: Dict[str, Any] = {}
    azure_config: Dict[str, Any] = {}
    
    # Metadata
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    
    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True
        json_encoders = {
            ObjectId: str
        }

class VMSpecRequest(BaseModel):
    """Request model for creating/updating VM specs"""
    preset: VMPreset
    display_name: str
    description: Optional[str] = None
    cpu_cores: int
    ram_gb: int
    storage_gb: int = 50
    requires_gpu: bool = False
    gpu_memory_gb: Optional[int] = None
    estimated_cost_per_hour_usd: float
    suitable_consoles: List[str] = []
    performance_tier: str
    tensordock_config: Dict[str, Any] = {}
    gcp_config: Dict[str, Any] = {}
    aws_config: Dict[str, Any] = {}
    azure_config: Dict[str, Any] = {}

class VMSpecResponse(BaseModel):
    """Response model for VM specs"""
    preset: VMPreset
    display_name: str
    description: Optional[str] = None
    cpu_cores: int
    ram_gb: int
    storage_gb: int
    requires_gpu: bool
    gpu_memory_gb: Optional[int] = None
    estimated_cost_per_hour_usd: float
    suitable_consoles: List[str]
    performance_tier: str
    tensordock_config: Dict[str, Any]
    gcp_config: Dict[str, Any]
    aws_config: Dict[str, Any]
    azure_config: Dict[str, Any]
    created_at: datetime
    updated_at: datetime