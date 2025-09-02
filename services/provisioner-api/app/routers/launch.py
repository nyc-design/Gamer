from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import Optional
from app.models.vm import (
    VMDocument, VMCreateRequest, VMResponse, ConsoleType, 
    CloudProvider, VMStatus
)
from app.models.emulator_config import EmulatorConfigDocument
from app.services.vm_orchestrator import VMOrchestrator
from app.services.region_service import RegionService
from app.services.tensordock_service import TensorDockService
from app.services.gcp_region_service import GCPRegionService
from app.core.database import get_database
from datetime import datetime
import uuid
import logging

logger = logging.getLogger(__name__)

router = APIRouter()

vm_orchestrator = VMOrchestrator()
region_service = RegionService()
tensordock_service = TensorDockService()
gcp_region_service = GCPRegionService()

class GameLaunchRequest:
    console_type: ConsoleType
    game_id: str
    save_id: Optional[str] = None
    user_id: str

@router.post("/game", response_model=VMResponse)
async def launch_game(
    console_type: ConsoleType,
    game_id: str,
    user_id: str,
    save_id: Optional[str] = None,
    user_latitude: Optional[float] = None,
    user_longitude: Optional[float] = None,
    background_tasks: BackgroundTasks = None
):
    """Launch a game by automatically selecting and provisioning the optimal VM"""
    
    # 1. Get emulator configuration
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(
            status_code=404, 
            detail=f"No configuration found for {console_type}"
        )
    config = EmulatorConfigDocument(**config_data)
    
    # 2. Check if user already has a running VM for this console type
    existing_vm_data = await db.vms.find_one({
        "console_type": console_type,
        "user_id": user_id,
        "status": {"$in": [VMStatus.RUNNING, VMStatus.CONFIGURING]}
    })
    existing_vm = VMDocument(**existing_vm_data) if existing_vm_data else None
    
    if existing_vm:
        # Update activity and return existing VM
        existing_vm.last_activity = datetime.utcnow()
        await db.vms.replace_one(
            {"vm_id": existing_vm.vm_id}, 
            existing_vm.dict(by_alias=True, exclude_none=True)
        )
        
        # Queue game launch on existing VM
        background_tasks.add_task(
            vm_orchestrator.launch_game_on_vm,
            existing_vm.vm_id,
            game_id,
            save_id
        )
        
        return VMResponse(**existing_vm.dict())
    
    # 3. Select optimal provider
    optimal_provider = await _select_optimal_provider(config)
    if not optimal_provider:
        raise HTTPException(
            status_code=503,
            detail=f"No available providers for {console_type}"
        )
    
    # 4. Create VM with optimal provider
    vm_id = str(uuid.uuid4())
    
    # Determine preset (use provider override or config default)
    preset = optimal_provider.get("preset_override", config.default_preset)
    
    # Prepare user location data
    user_location = None
    if user_latitude is not None and user_longitude is not None:
        if region_service.validate_location(user_latitude, user_longitude):
            user_location = {"latitude": user_latitude, "longitude": user_longitude}
        else:
            logger.warning(f"Invalid user location: ({user_latitude}, {user_longitude})")
    
    # Create VM document
    vm_document = VMDocument(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        preset=preset,
        console_type=console_type,
        provider=optimal_provider["provider"],
        user_id=user_id,
        auto_stop_timeout=config.max_session_hours * 3600,
        last_activity=datetime.utcnow(),
        user_location=user_location
    )
    
    await db.vms.insert_one(vm_document.dict(by_alias=True, exclude_none=True))
    
    # 5. Start VM provisioning and game setup in background
    background_tasks.add_task(
        vm_orchestrator.provision_and_launch_game,
        vm_id,
        game_id,
        save_id
    )
    
    return VMResponse(**vm_document.dict())

@router.post("/vm/{vm_id}/game")
async def launch_game_on_vm(
    vm_id: str,
    game_id: str,
    save_id: Optional[str] = None,
    background_tasks: BackgroundTasks = None
):
    """Launch a specific game on an existing VM"""
    
    db = get_database()
    vm_data = await db.vms.find_one({"vm_id": vm_id})
    if not vm_data:
        raise HTTPException(status_code=404, detail="VM not found")
    vm = VMDocument(**vm_data)
    
    if vm.status != VMStatus.RUNNING:
        raise HTTPException(
            status_code=400, 
            detail=f"VM is not running (status: {vm.status})"
        )
    
    # Update activity
    vm.last_activity = datetime.utcnow()
    await db.vms.replace_one(
        {"vm_id": vm.vm_id}, 
        vm.dict(by_alias=True, exclude_none=True)
    )
    
    # Queue game launch
    background_tasks.add_task(
        vm_orchestrator.launch_game_on_vm,
        vm_id,
        game_id,
        save_id
    )
    
    return {"message": f"Game {game_id} launch queued on VM {vm_id}"}

@router.get("/vm/optimal-for/{console_type}")
async def get_optimal_vm_for_console(console_type: ConsoleType, user_id: str):
    """Get the optimal VM configuration for a console type"""
    
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": console_type})
    if not config_data:
        raise HTTPException(
            status_code=404, 
            detail=f"No configuration found for {console_type}"
        )
    config = EmulatorConfigDocument(**config_data)
    
    optimal_provider = await _select_optimal_provider(config)
    if not optimal_provider:
        raise HTTPException(
            status_code=503,
            detail=f"No available providers for {console_type}"
        )
    
    return {
        "console_type": console_type,
        "recommended_provider": optimal_provider["provider"],
        "preset": optimal_provider.get("preset_override", config.default_preset),
        "estimated_cost_per_hour": optimal_provider.get("cost_per_hour_limit"),
        "max_session_hours": config.max_session_hours,
        "requirements": {
            "min_cpu": config.min_cpu,
            "min_ram_gb": config.min_ram_gb,
            "requires_gpu": config.requires_gpu
        }
    }

@router.get("/hostnodes/available")
async def get_available_hostnodes(
    min_gpu_count: int = 0,
    console_type: Optional[ConsoleType] = None
):
    """Get all available TensorDock hostnodes with optional filtering"""
    
    try:
        hostnodes = await tensordock_service.list_available_hostnodes(min_gpu_count)
        
        if console_type:
            # Filter based on console requirements
            db = get_database()
            config_data = await db.emulator_configs.find_one({"console_type": console_type})
            if config_data:
                config = EmulatorConfigDocument(**config_data)
                # Filter hostnodes that meet console requirements
                filtered_hostnodes = []
                for hostnode in hostnodes:
                    specs = hostnode.get('specs', {})
                    if (specs.get('cpu', 0) >= config.min_cpu and 
                        specs.get('ram', 0) >= config.min_ram_gb * 1024 and
                        len(specs.get('gpu', [])) >= (1 if config.requires_gpu else 0)):
                        filtered_hostnodes.append(hostnode)
                hostnodes = filtered_hostnodes
        
        return {
            "total_hostnodes": len(hostnodes),
            "hostnodes": hostnodes
        }
        
    except Exception as e:
        logger.error(f"Error getting available hostnodes: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to get available hostnodes"
        )

@router.get("/locations/summary")
async def get_locations_summary():
    """Get a summary of all available locations"""
    
    try:
        locations = await tensordock_service.get_available_locations()
        return locations
        
    except Exception as e:
        logger.error(f"Error getting locations summary: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to get locations summary"
        )

@router.get("/hostnodes/closest")
async def get_closest_hostnodes(
    user_latitude: float,
    user_longitude: float,
    min_gpu_count: int = 0,
    console_type: Optional[ConsoleType] = None,
    limit: int = 10
):
    """Get the closest hostnodes to a user location"""
    
    if not region_service.validate_location(user_latitude, user_longitude):
        raise HTTPException(
            status_code=400,
            detail="Invalid location coordinates"
        )
    
    try:
        # Get all hostnodes
        hostnodes = await tensordock_service.list_available_hostnodes(min_gpu_count)
        
        if not hostnodes:
            raise HTTPException(
                status_code=404,
                detail="No hostnodes available"
            )
        
        # Calculate distances for all hostnodes
        user_coords = (user_latitude, user_longitude)
        hostnodes_with_distance = []
        
        for hostnode in hostnodes:
            distance = await tensordock_service.geocoding_service.calculate_distance(
                user_coords, hostnode
            )
            if distance is not None:
                hostnodes_with_distance.append({
                    "hostnode": hostnode,
                    "distance_km": round(distance, 1),
                    "location": f"{hostnode.get('city')}, {hostnode.get('region', '')}, {hostnode.get('country')}".replace(", ,", ",").strip(", ")
                })
        
        # Sort by distance and limit results
        hostnodes_with_distance.sort(key=lambda x: x['distance_km'])
        closest_hostnodes = hostnodes_with_distance[:limit]
        
        return {
            "user_location": {"latitude": user_latitude, "longitude": user_longitude},
            "total_found": len(closest_hostnodes),
            "closest_hostnodes": closest_hostnodes
        }
        
    except Exception as e:
        logger.error(f"Error getting closest hostnodes: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to get closest hostnodes"
        )

@router.get("/gcp/regions/closest")
async def get_closest_gcp_regions(
    user_latitude: float,
    user_longitude: float,
    limit: int = 5
):
    """Get the closest GCP regions for CloudyPad using MongoDB data"""
    
    if not region_service.validate_location(user_latitude, user_longitude):
        raise HTTPException(
            status_code=400,
            detail="Invalid location coordinates"
        )
    
    try:
        user_coords = (user_latitude, user_longitude)
        
        # Get regions using database lookup with hardcoded coordinates
        regions = gcp_region_service.get_top_regions(user_coords, limit)
        
        return {
            "user_location": {"latitude": user_latitude, "longitude": user_longitude},
            "method": "mongodb_distance_calculation",
            "closest_region": regions[0] if regions else None,
            "alternatives": regions[1:] if len(regions) > 1 else [],
            "total_found": len(regions)
        }
        
    except Exception as e:
        logger.error(f"Error getting closest GCP regions: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to get closest GCP regions"
        )

@router.get("/gcp/regions/all")
async def get_all_gcp_regions():
    """Get all available GCP regions grouped by continent"""
    
    try:
        regions = gcp_region_service.get_all_regions()
        return regions
        
    except Exception as e:
        logger.error(f"Error getting all GCP regions: {str(e)}")
        raise HTTPException(
            status_code=500,
            detail="Failed to get GCP regions"
        )

async def _select_optimal_provider(config: EmulatorConfigDocument) -> Optional[dict]:
    """Select the optimal provider from the configuration"""
    
    # Sort providers by priority and find first available
    providers = sorted(config.preferred_providers, key=lambda x: x.get("priority", 99))
    
    for provider_config in providers:
        if not provider_config.get("enabled", True):
            continue
        
        # TODO: Add provider availability checks here
        # For now, return first enabled provider
        return provider_config
    
    return None