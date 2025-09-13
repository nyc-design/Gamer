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


@router.get("/instances/available", response_model=VMResponse)
async def list_available_instances(console_type: ConsoleType, user_lat: Optional[float] = None, user_lng: Optional[float] = None):
# Take console type, and call MongoDB database function to get config for that console

# Take config and user lat / long and pass it to tensordock service function to find all instances, return as list

# Take config and user lat / long and pass it to cloudypad service function to find all instances, return as list

# Combine all lists, then pass each instance to calculate_distance function from geocoding service to get distance to user

# Pass back list of VM Responses as VMAvailableResponse models


@router.post("/instances/create", response_model=VMResponse)
async def create_instance(console_type: ConsoleType, create_request: VMCreateRequest, user_id: Optional[str] = None, background_tasks: BackgroundTasks = None):
# Create password for instance

# Create ssh_key for instance

# Add instance to MongoDB database with status "provisioning"

# Check provider in create request

# If tensordock, map to TensorDockCreateRequest model and pass to tensordock create function as async

# If gcp or others, map to CloudyPadCreateRequest model and pass to cloudypad service create function as async

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

# Call MongoDB status update to update status to "starting" and return updated doc

# Grab provider instance ID from update doc

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