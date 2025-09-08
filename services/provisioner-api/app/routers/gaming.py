from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import List, Optional, Dict, Any
from app.models.vm import VMResponse, VMDocument, VMStatus, ConsoleType
from app.services.tensordock_service import TensorDockService
from app.services.gcp_compute_service import GCPComputeService
from app.services.geocoding_service import GeocodingService
from app.core.database import get_client
import uuid
from datetime import datetime

router = APIRouter()

# Console requirements
CONSOLE_CONFIGS = {
    ConsoleType.NES: {"tensordock_gpus": [], "gcp_types": ["e2-standard-2"], "min_cpu": 2, "min_ram": 4},
    ConsoleType.SNES: {"tensordock_gpus": [], "gcp_types": ["e2-standard-2"], "min_cpu": 2, "min_ram": 4},
    ConsoleType.GBA: {"tensordock_gpus": [], "gcp_types": ["e2-standard-2"], "min_cpu": 2, "min_ram": 4},
    ConsoleType.NDS: {"tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"], "gcp_types": ["n1-standard-4"], "min_cpu": 4, "min_ram": 8},
    ConsoleType.N3DS: {"tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"], "gcp_types": ["n1-standard-4"], "min_cpu": 4, "min_ram": 8},
    ConsoleType.SWITCH: {"tensordock_gpus": ["RTX3070", "RTX3080", "RTX4070", "RTX4080", "RTX4090"], "gcp_types": ["n1-standard-8"], "min_cpu": 8, "min_ram": 16},
    ConsoleType.GAMECUBE: {"tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"], "gcp_types": ["n1-standard-4"], "min_cpu": 4, "min_ram": 8},
    ConsoleType.WII: {"tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"], "gcp_types": ["n1-standard-4"], "min_cpu": 4, "min_ram": 8}
}

# List available instances across all providers
@router.get("/instances/available", response_model=VMResponse)
async def list_available_instances(console_type: ConsoleType, user_lat: Optional[float] = None, user_lng: Optional[float] = None):
# Take console type, and call MongoDB database function to get config for that console

# Take config and user lat / long and pass it to tensordock service function to find all instances, return as list

# Take config and user lat / long and pass it to cloudypad service function to find all instances, return as list

# Pass back list of VM Responses



@router.post("/instances/create", response_model=VMResponse)
async def create_instance(console_type: ConsoleType, config: VMCreateRequest, user_id: Optional[str] = None, background_tasks: BackgroundTasks = None):
# Add instance to MongoDB database with status "provisioning"

# Check provider in create request

# If tensordock, pass to tensordock create function as async

# If gcp or others, pass to cloudypad service create function as async

# pass back confirmation response to user



@router.get("/instances/{vm_id}/status", response_model = VMResponse)
async def get_instance_status(vm_id: str):
# Take VM_ID and check status in mongodb document


@router.get("/instances", response_model = List[VMResponse])
async def list_existing_instances(console_type: ConsoleType, user_id: Optional[str] = None):
# Call MongoDB for all existing instances

# Pass back as list of VMResponse model


@router.post("/instances/{vm_id}/start")
async def start_instance(vm_id: str):
# Call MongoDB to get instance doc

# Call MongoDB status update to update status to "starting"

# If provider is tensordock, pass to tensordock start function with tensordock vm id with async

# If provider is GCP or other, pass to cloudypad start function with instance name with async

# Pass back confirmation response to user


@router.post("/instances/{vm_id}/stop")
async def stop_instance(vm_id: str):
# Call MongoDB to get instance doc

# Call MongoDB status update to update status to "stopping"

# If provider is tensordock, pass to tensordock stop function with tensordock vm id with async

# If provider is GCP or other, pass to cloudypad stop function with instance name with async

# Pass back confirmation response to user


@router.delete("/instances/{vm_id}/destroy")
async def destroy_instance(vm_id: str):
# Call MongoDB to get instance doc

# Call MongoDB status update to update status to "destroying"

# If provider is tensordock, pass to tensordock stop function with tensordock vm id with async

# If provider is GCP or other, pass to cloudypad stop function with instance name with async

# Pass back confirmation response to user


@router.get("/billing")
async def get_billing(user_id: Optional[str] = None):