from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import List, Optional, Dict, Any
from app.models.vm import (
    VMResponse, VMDocument, VMStatus, ConsoleType, VMCreateRequest,
    VMAvailableResponse, VMStatusResponse, TensorDockCreateRequest,
    GCPCreateRequest, ConsoleConfigDocument, CloudProvider
)
from app.services.tensordock_service import TensorDockService
from app.services.gcp_compute_service import GCPComputeService
from app.services.geocoding_service import GeocodingService
from app.core.database import get_console_config, get_instance, set_instance_status, add_new_instance, update_instance_doc
import uuid
import secrets
import string
from datetime import datetime
from cryptography.hazmat.primitives import serialization
from cryptography.hazmat.primitives.asymmetric import rsa

router = APIRouter()

# Initialize services
tensordock_service = TensorDockService()
gcp_service = GCPComputeService()
geocoding_service = GeocodingService()


@router.get("/instances/available", response_model=List[VMAvailableResponse])
async def list_available_instances(console_type: ConsoleType, user_lat: Optional[float] = None, user_lng: Optional[float] = None):
    """
    List available VM instances for a specific console type

    Implementation checklist:
    [x] Take console type and get config from MongoDB
    [x] Get available instances from TensorDock service
    [x] Get available instances from GCP service
    [x] Combine all instance lists
    [x] Calculate distance to user if location provided
    [x] Sort by distance and return as VMAvailableResponse models
    """
    # Take console type and get config from MongoDB
    console_config = get_console_config(console_type)
    if not console_config:
        raise HTTPException(status_code=404, detail=f"Console config not found for {console_type}")

    # Get available instances from TensorDock service
    user_location = (user_lat, user_lng) if user_lat is not None and user_lng is not None else None
    tensordock_instances = await tensordock_service.list_available_hostnodes(console_config, user_location)

    # Get available instances from GCP service
    gcp_instances = await gcp_service.list_available_regions(console_config, user_location)

    # Combine all instance lists
    all_instances = tensordock_instances + gcp_instances

    # Calculate distance to user if location provided
    if user_location:
        for instance in all_instances:
            distance = geocoding_service.calculate_distance(
                user_location[0], user_location[1],
                instance.instance_lat, instance.instance_long
            )
            instance.distance_to_user = distance

    # Sort by distance and return as VMAvailableResponse models
    if user_location:
        all_instances.sort(key=lambda x: x.distance_to_user)

    return all_instances


@router.post("/instances/create", response_model=VMResponse)
async def create_instance(console_type: ConsoleType, create_request: VMCreateRequest, user_id: Optional[str] = None, background_tasks: BackgroundTasks = None):
    """
    Create a new gaming VM instance

    Implementation checklist:
    [ ] Get console config for validation and defaults
    [ ] Generate secure credentials (password and SSH key)
    [ ] Apply console config defaults for missing values
    [ ] Determine compatible console types for this configuration
    [ ] Create VM document in database with CREATING status
    [ ] Route to appropriate provider service (TensorDock/GCP)
    [ ] Return confirmation response to user
    """
    pass


@router.get("/instances/{vm_id}/status", response_model=VMStatusResponse)
async def get_instance_status(vm_id: str):
    """
    Get status of a specific VM instance

    Implementation checklist:
    [ ] Take VM ID and check status in MongoDB document
    [ ] Return status response or 404 if not found
    """
    pass


@router.get("/instances", response_model=List[VMResponse])
async def list_existing_instances(console_type: ConsoleType, user_id: Optional[str] = None):
    """
    List existing VM instances for a console type and optional user

    Implementation checklist:
    [ ] Call MongoDB to get all existing instances
    [ ] Filter by console type and user ID if provided
    [ ] Return as list of VMResponse models
    """
    pass


@router.post("/instances/{vm_id}/start")
async def start_instance(vm_id: str, background_tasks: BackgroundTasks):
    """
    Start a stopped VM instance

    Implementation checklist:
    [x] Get instance document from MongoDB
    [x] Update status to STARTING in database
    [x] Route to appropriate provider service based on provider type
    [x] Return confirmation response to user
    """
    # Get instance document from MongoDB
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Update status to STARTING in database
    set_instance_status(vm_id, VMStatus.STARTING)

    # Route to appropriate provider service based on provider type
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.start_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        background_tasks.add_task(gcp_service.start_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for start operation")

    # Return confirmation response to user
    return {"status": VMStatus.STARTING, "vm_id": instance['vm_id']}


@router.post("/instances/{vm_id}/stop")
async def stop_instance(vm_id: str, background_tasks: BackgroundTasks):
    """
    Stop a running VM instance

    Implementation checklist:
    [x] Get instance document from MongoDB
    [x] Update status to STOPPING in database
    [x] Route to appropriate provider service based on provider type
    [x] Return confirmation response to user
    """
    # Get instance document from MongoDB
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Update status to STOPPING in database
    set_instance_status(vm_id, VMStatus.STOPPING)

    # Route to appropriate provider service based on provider type
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.stop_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        background_tasks.add_task(gcp_service.stop_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for stop operation")

    # Return confirmation response to user
    return {"status": VMStatus.STOPPING, "vm_id": instance['vm_id']}


@router.delete("/instances/{vm_id}/destroy")
async def destroy_instance(vm_id: str, background_tasks: BackgroundTasks):
    """
    Permanently destroy a VM instance

    Implementation checklist:
    [x] Get instance document from MongoDB
    [x] Update status to DESTROYING in database
    [x] Route to appropriate provider service based on provider type
    [x] Return confirmation response to user
    """
    # Get instance document from MongoDB
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Update status to DESTROYING in database
    set_instance_status(vm_id, VMStatus.DESTROYING)

    # Route to appropriate provider service based on provider type
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.destroy_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        background_tasks.add_task(gcp_service.destroy_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for destroy operation")

    # Return confirmation response to user
    return {"status": VMStatus.DESTROYING, "vm_id": instance['vm_id']}


@router.get("/billing")
async def get_billing(user_id: Optional[str] = None):
    """
    Calculate billing information for user's instances

    Implementation checklist:
    [x] Get user's instances from MongoDB for billing calculation
    [x] Calculate total costs from provider pricing
    [x] Return billing breakdown by provider and instance list
    """
    # Get user's instances from MongoDB for billing calculation
    instances = get_instance()
    user_instances = [i for i in instances if user_id is None or i.get('user_id') == user_id]

    # Calculate total costs from provider pricing
    tensordock_total = 0.0
    gcp_total = 0.0

    for instance in user_instances:
        if instance['provider'] == CloudProvider.TENSORDOCK:
            tensordock_total += float(instance.get('hourly_price', 0))
        else:
            gcp_total += float(instance.get('hourly_price', 0))

    # Return billing breakdown by provider and instance list
    return {
        "tensordock": {"total_cost": tensordock_total, "current_month": tensordock_total},
        "gcp": {"total_cost": gcp_total, "current_month": gcp_total},
        "instances": [
            {
                "vm_id": i['vm_id'],
                "provider": i['provider'],
                "hourly_price": i['hourly_price'],
                "status": i['status']
            }
            for i in user_instances
        ]
    }