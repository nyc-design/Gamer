from fastapi import APIRouter, HTTPException
from typing import List
from app.models.vm_preset import VMSpecDocument, VMSpecRequest, VMSpecResponse
from app.models.vm import VMPreset
from app.services.vm_spec_service import VMSpecService
from datetime import datetime

router = APIRouter()

@router.get("/", response_model=List[VMSpecResponse])
async def list_vm_specs():
    """Get all VM specifications"""
    specs = VMSpecService.list_all_specs()
    return [VMSpecResponse(**spec.dict()) for spec in specs]

@router.get("/{preset}", response_model=VMSpecResponse)
async def get_vm_spec(preset: VMPreset):
    """Get specific VM specification by preset"""
    try:
        specs = VMSpecService.get_vm_specs(preset)
        # Get the full document from MongoDB
        from app.core.sync_database import get_sync_database
        db = get_sync_database()
        spec_data = db.vm_specs.find_one({"preset": preset})
        if not spec_data:
            raise HTTPException(status_code=404, detail=f"VM spec not found for preset: {preset}")
        
        spec = VMSpecDocument(**spec_data)
        return VMSpecResponse(**spec.dict())
    except ValueError as e:
        raise HTTPException(status_code=404, detail=str(e))

@router.post("/", response_model=VMSpecResponse)
async def create_vm_spec(spec_request: VMSpecRequest):
    """Create or update VM specification"""
    try:
        spec = VMSpecService.create_or_update_spec(spec_request.dict())
        return VMSpecResponse(**spec.dict())
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))

@router.post("/seed-defaults")
async def seed_default_vm_specs():
    """Initialize default VM specifications"""
    
    default_specs = [
        {
            "preset": VMPreset.RETRO,
            "display_name": "Retro Gaming",
            "description": "Optimized for retro consoles like NES, SNES, Game Boy",
            "cpu_cores": 2,
            "ram_gb": 4,
            "storage_gb": 50,
            "requires_gpu": False,
            "gpu_memory_gb": None,
            "estimated_cost_per_hour_usd": 0.15,
            "suitable_consoles": ["nes", "snes", "gb", "gbc", "gba"],
            "performance_tier": "low",
            "tensordock_config": {
                "gpu_model": None,
                "min_hostnode_gpu": 0
            },
            "gcp_config": {
                "machine_type": "e2-standard-2",
                "gpu_type": None
            }
        },
        {
            "preset": VMPreset.ADVANCED,
            "display_name": "Advanced Gaming", 
            "description": "Optimized for DS, 3DS and similar consoles",
            "cpu_cores": 4,
            "ram_gb": 8,
            "storage_gb": 100,
            "requires_gpu": True,
            "gpu_memory_gb": 6,
            "estimated_cost_per_hour_usd": 0.35,
            "suitable_consoles": ["nds", "3ds"],
            "performance_tier": "medium",
            "tensordock_config": {
                "gpu_model": "GTX1060",
                "min_hostnode_gpu": 1
            },
            "gcp_config": {
                "machine_type": "n1-standard-4",
                "gpu_type": "nvidia-tesla-t4"
            }
        },
        {
            "preset": VMPreset.PREMIUM,
            "display_name": "Premium Gaming",
            "description": "Optimized for GameCube, Wii, Switch and demanding emulation",
            "cpu_cores": 8,
            "ram_gb": 16,
            "storage_gb": 200,
            "requires_gpu": True,
            "gpu_memory_gb": 24,
            "estimated_cost_per_hour_usd": 1.20,
            "suitable_consoles": ["gamecube", "wii", "switch"],
            "performance_tier": "high",
            "tensordock_config": {
                "gpu_model": "RTX4090",
                "min_hostnode_gpu": 1
            },
            "gcp_config": {
                "machine_type": "n1-standard-8",
                "gpu_type": "nvidia-tesla-t4"
            }
        }
    ]
    
    created_count = 0
    for spec_data in default_specs:
        try:
            VMSpecService.create_or_update_spec(spec_data)
            created_count += 1
        except Exception as e:
            print(f"Error creating spec for {spec_data['preset']}: {str(e)}")
            continue
    
    return {"message": f"Seeded {created_count} VM specifications"}

@router.delete("/{preset}")
async def delete_vm_spec(preset: VMPreset):
    """Delete VM specification"""
    try:
        from app.core.sync_database import get_sync_database
        db = get_sync_database()
        result = db.vm_specs.delete_one({"preset": preset})
        if result.deleted_count == 0:
            raise HTTPException(status_code=404, detail=f"VM spec not found for preset: {preset}")
        return {"message": f"VM specification for {preset} deleted"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))