from fastapi import APIRouter, HTTPException
from typing import List, Optional
from app.models.emulator_config import (
    EmulatorConfigDocument, EmulatorConfigRequest, EmulatorConfigResponse,
    ProviderConfigRequest, ProviderConfigResponse
)
from app.models.vm import ConsoleType, CloudProvider
from app.core.database import get_database
from datetime import datetime

router = APIRouter()

@router.get("/emulators", response_model=List[EmulatorConfigResponse])
async def list_emulator_configs():
    """Get all emulator configurations"""
    db = get_database()
    config_data_list = await db.emulator_configs.find({}).to_list(None)
    configs = [EmulatorConfigDocument(**config_data) for config_data in config_data_list]
    return [EmulatorConfigResponse(**config.dict()) for config in configs]

@router.get("/emulators/{console_type}", response_model=EmulatorConfigResponse)
async def get_emulator_config(console_type: ConsoleType):
    """Get specific emulator configuration"""
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(status_code=404, detail="Emulator configuration not found")
    config = EmulatorConfigDocument(**config_data)
    return EmulatorConfigResponse(**config.dict())

@router.post("/emulators", response_model=EmulatorConfigResponse)
async def create_emulator_config(config_request: EmulatorConfigRequest):
    """Create or update emulator configuration"""
    
    # Check if config already exists
    db = get_database()
    existing_config_data = await db.emulator_configs.find_one({"console_type": config_request.console_type})
    existing_config = EmulatorConfigDocument(**existing_config_data) if existing_config_data else None
    
    if existing_config:
        # Update existing config
        for field, value in config_request.dict(exclude_unset=True).items():
            setattr(existing_config, field, value)
        existing_config.updated_at = datetime.utcnow()
        await db.emulator_configs.replace_one(
            {"console_type": existing_config.console_type},
            existing_config.dict(by_alias=True, exclude_none=True)
        )
        return EmulatorConfigResponse(**existing_config.dict())
    else:
        # Create new config
        new_config = EmulatorConfigDocument(**config_request.dict())
        await db.emulator_configs.insert_one(new_config.dict(by_alias=True, exclude_none=True))
        return EmulatorConfigResponse(**new_config.dict())

@router.delete("/emulators/{console_type}")
async def delete_emulator_config(console_type: ConsoleType):
    """Delete emulator configuration"""
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(status_code=404, detail="Emulator configuration not found")
    config = EmulatorConfigDocument(**config_data)
    
    await db.emulator_configs.delete_one({"console_type": console_type})
    return {"message": f"Emulator configuration for {console_type} deleted"}

@router.post("/emulators/{console_type}/providers", response_model=EmulatorConfigResponse)
async def update_provider_config(
    console_type: ConsoleType, 
    provider_config: ProviderConfigRequest
):
    """Update provider configuration for a console type"""
    
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(status_code=404, detail="Emulator configuration not found")
    config = EmulatorConfigDocument(**config_data)
    
    # Update provider in preferred_providers list
    provider_dict = provider_config.dict()
    
    # Remove existing provider config if exists
    config.preferred_providers = [
        p for p in config.preferred_providers 
        if p.get('provider') != provider_config.provider
    ]
    
    # Add new provider config
    if provider_config.enabled:
        config.preferred_providers.append(provider_dict)
        # Sort by priority
        config.preferred_providers.sort(key=lambda x: x.get('priority', 99))
    
    config.updated_at = datetime.utcnow()
    await db.emulator_configs.replace_one(
        {"console_type": config.console_type},
        config.dict(by_alias=True, exclude_none=True)
    )
    
    return EmulatorConfigResponse(**config.dict())

@router.get("/emulators/{console_type}/optimal-provider")
async def get_optimal_provider(console_type: ConsoleType):
    """Get the optimal provider for a console type based on configuration"""
    
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(status_code=404, detail="Emulator configuration not found")
    config = EmulatorConfigDocument(**config_data)
    
    # Find first enabled provider with lowest priority
    for provider_config in config.preferred_providers:
        if provider_config.get('enabled', True):
            return {
                "console_type": console_type,
                "recommended_provider": provider_config.get('provider'),
                "preset": provider_config.get('preset_override', config.default_preset),
                "estimated_cost_per_hour": provider_config.get('cost_per_hour_limit'),
                "reasoning": f"Primary provider for {console_type}"
            }
    
    raise HTTPException(
        status_code=404, 
        detail=f"No enabled providers configured for {console_type}"
    )

@router.post("/seed-default-configs")
async def seed_default_configs():
    """Initialize default emulator configurations"""
    
    default_configs = [
        {
            "console_type": ConsoleType.NES,
            "display_name": "Nintendo Entertainment System",
            "default_preset": "retro",
            "min_cpu": 1,
            "min_ram_gb": 2,
            "requires_gpu": False,
            "default_emulator": "nestopia",
            "supported_file_extensions": [".nes", ".zip"],
            "save_file_extensions": [".sav", ".srm"],
            "preferred_providers": [
                {"provider": "tensordock", "priority": 1, "enabled": True},
                {"provider": "cloudypad_gcp", "priority": 2, "enabled": True}
            ]
        },
        {
            "console_type": ConsoleType.SWITCH,
            "display_name": "Nintendo Switch", 
            "default_preset": "premium",
            "min_cpu": 8,
            "min_ram_gb": 16,
            "requires_gpu": True,
            "default_emulator": "yuzu",
            "supported_file_extensions": [".xci", ".nsp"],
            "save_file_extensions": [".sav"],
            "cost_per_hour_limit": 2.00,
            "preferred_providers": [
                {"provider": "tensordock", "priority": 1, "enabled": True},
                {"provider": "cloudypad_gcp", "priority": 2, "enabled": True}
            ]
        }
        # Add more default configs...
    ]
    
    created_count = 0
    db = get_database()
    for config_data in default_configs:
        existing_data = await db.emulator_configs.find_one({"console_type": config_data["console_type"]})
        if not existing_data:
            new_config = EmulatorConfigDocument(**config_data)
            await db.emulator_configs.insert_one(new_config.dict(by_alias=True, exclude_none=True))
            created_count += 1
    
    return {"message": f"Created {created_count} default configurations"}