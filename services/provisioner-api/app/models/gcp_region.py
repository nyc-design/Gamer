from pydantic import BaseModel, Field
from typing import Optional, Dict, Any, List
from datetime import datetime
from bson import ObjectId

class GCPRegionDocument(BaseModel):
    """GCP Region document stored in MongoDB"""
    id: Optional[ObjectId] = Field(None, alias="_id")
    region_code: str = Field(..., description="GCP region identifier (e.g., us-central1)")
    display_name: str = Field(..., description="Human-readable name (e.g., Iowa)")
    country: str = Field(..., description="Country name")
    continent: str = Field(..., description="Continent grouping")
    
    # Geographic coordinates (approximate data center location)
    latitude: float
    longitude: float
    
    # Metadata
    is_active: bool = Field(default=True, description="Whether region is available")
    notes: Optional[str] = None
    created_at: datetime = Field(default_factory=datetime.utcnow)
    updated_at: datetime = Field(default_factory=datetime.utcnow)
    
    class Config:
        populate_by_name = True
        arbitrary_types_allowed = True
        json_encoders = {
            ObjectId: str
        }

class GCPRegionRequest(BaseModel):
    """Request model for creating/updating GCP regions"""
    region_code: str
    display_name: str
    country: str
    continent: str
    latitude: float
    longitude: float
    is_active: bool = True
    notes: Optional[str] = None

class GCPRegionResponse(BaseModel):
    """Response model for GCP regions"""
    region_code: str
    display_name: str
    country: str
    continent: str
    latitude: float
    longitude: float
    is_active: bool
    notes: Optional[str] = None
    created_at: datetime
    updated_at: datetime