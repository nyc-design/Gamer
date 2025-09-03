from pydantic import BaseModel
from typing import List, Dict, Any, Optional
from datetime import datetime
from bson import ObjectId
from app.models.vm import ConsoleType

class ConsoleConfig(BaseModel):
    """Simplified console configuration - what providers/specs work for each console"""
    id: Optional[ObjectId] = None
    console_type: ConsoleType
    
    # TensorDock requirements
    tensordock_gpus: List[str] = []  # GPU models that work: ["RTX4090", "RTX3080"]
    
    # GCP requirements  
    gcp_machine_types: List[str] = []  # Machine types: ["n1-standard-8", "n2-standard-4"]
    
    # Minimum specs (for filtering)
    min_cpu: int = 2
    min_ram_gb: int = 4
    
    class Config:
        arbitrary_types_allowed = True
        json_encoders = {ObjectId: str}

# Available instance option for user selection
class InstanceOption(BaseModel):
    """User-facing instance option"""
    provider: str  # "tensordock" or "gcp"
    location: str  # "Dallas, TX" or "us-south1"
    specs: str     # "RTX4090, 8 CPU, 16GB" 
    cost_per_hour: float
    distance_km: Optional[float] = None
    
    # Internal data for creation
    provider_data: Dict[str, Any] = {}  # Hostnode ID, machine type, etc.