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
    pass


@router.post("/instances/create", response_model=VMResponse)
async def create_instance(console_type: ConsoleType, create_request: VMCreateRequest, user_id: Optional[str] = None, background_tasks: BackgroundTasks = None):
    # Get console config for default values and compatibility check
    console_config = get_console_config(console_type)
    if not console_config:
        raise HTTPException(status_code=404, detail=f"Console config not found for {console_type}")

    # Create password for instance
    password = ''.join(secrets.choice(string.ascii_letters + string.digits) for _ in range(16))

    # Create ssh_key for instance
    private_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ssh_key = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    ).decode('utf-8')

    # Use console config defaults if values are missing from create request
    num_cpus = create_request.num_cpus or console_config.min_cpus
    num_ram = create_request.num_ram or console_config.min_ram
    num_disk = create_request.num_disk or console_config.min_disk

    # Check which other console types this instance configuration supports
    supported_console_types = [console_type]
    for other_console_type in ConsoleType:
        if other_console_type != console_type:
            other_config = get_console_config(other_console_type)
            if (other_config and
                num_cpus >= other_config.min_cpus and
                num_ram >= other_config.min_ram and
                num_disk >= other_config.min_disk):
                supported_console_types.append(other_console_type)

    # Add instance to MongoDB database with status "provisioning"
    vm_id = str(uuid.uuid4())
    vm_doc = VMDocument(
        **create_request.dict(),
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_types=supported_console_types,
        num_cpus=num_cpus,
        num_ram=num_ram,
        num_disk=num_disk,
        auto_stop_timeout=create_request.auto_stop_timeout,
        ssh_key=ssh_key,
        instance_password=password,
    )
    add_new_instance(vm_doc, VMStatus.CREATING)

    # Check provider in create request
    if create_request.provider == CloudProvider.TENSORDOCK:
        # If tensordock, map to TensorDockCreateRequest model and pass to tensordock create function as async
        td_request = TensorDockCreateRequest(
            password=password,
            ssh_key=ssh_key,
            **create_request.dict()
        )
        background_tasks.add_task(tensordock_service.create_vm, td_request, vm_doc)
    elif create_request.provider == CloudProvider.GCP:
        # If gcp, map to GCPCreateRequest model and pass to gcp service create function as async
        gcp_request = GCPCreateRequest(**create_request.dict())
        background_tasks.add_task(gcp_service.create_vm, gcp_request, vm_doc)
    else:
        # For other providers (AWS, Azure, etc), we can add their specific implementations later
        raise HTTPException(status_code=400, detail=f"Provider {create_request.provider} not yet supported")

    # pass back confirmation response to user
    return VMResponse(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_type=console_type,
        created_at=vm_doc.created_at,
        **create_request.dict(exclude={'provider_id', 'instance_name', 'os', 'num_cpus', 'num_ram', 'num_disk', 'auto_stop_timeout', 'user_id'})
    )


@router.get("/instances/{vm_id}/status", response_model=VMStatusResponse)
async def get_instance_status(vm_id: str):
    # Take VM_ID and check status in mongodb document
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")
    
    return VMStatusResponse(
        vm_id=instance['vm_id'],
        status=instance['status'],
        ip_address=instance.get('ip_address'),
        last_activity=instance.get('last_activity')
    )


@router.get("/instances", response_model=List[VMResponse])
async def list_existing_instances(console_type: ConsoleType, user_id: Optional[str] = None):
    # Call MongoDB for all existing instances
    instances = get_instance()  # Get all active instances
    
    # Filter by console type and user_id if provided
    filtered_instances = []
    for instance in instances:
        if console_type in instance.get('console_types', []):
            if user_id is None or instance.get('user_id') == user_id:
                filtered_instances.append(instance)

    # Pass back as list of VMResponse model
    return [
        VMResponse(
            console_type=console_type,
            **{k: v for k, v in instance.items() if k in ['vm_id', 'status', 'provider', 'instance_type', 'hourly_price', 'created_at', 'instance_lat', 'instance_long', 'last_activity']}
        )
        for instance in filtered_instances
    ]


@router.post("/instances/{vm_id}/start")
async def start_instance(vm_id: str, background_tasks: BackgroundTasks):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "starting"
    set_instance_status(vm_id, VMStatus.STARTING)

    # If provider is tensordock, pass to tensordock start function with tensordock vm id with async
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.start_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        # If provider is GCP, pass to gcp start function with instance name with async
        background_tasks.add_task(gcp_service.start_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for start operation")

    # Pass back confirmation response to user
    return {"status": VMStatus.STARTING, "vm_id": instance['vm_id']}


@router.post("/instances/{vm_id}/stop")
async def stop_instance(vm_id: str, background_tasks: BackgroundTasks):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "stopping"
    set_instance_status(vm_id, VMStatus.STOPPING)

    # If provider is tensordock, pass to tensordock start function with tensordock vm id with async
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.stop_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        # If provider is GCP, pass to gcp stop function with instance name with async
        background_tasks.add_task(gcp_service.stop_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for stop operation")

    # Pass back confirmation response to user
    return {"status": VMStatus.STOPPING, "vm_id": instance['vm_id']}


@router.delete("/instances/{vm_id}/destroy")
async def destroy_instance(vm_id: str, background_tasks: BackgroundTasks):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "destroying"
    set_instance_status(vm_id, VMStatus.DESTROYING)

    # If provider is tensordock, pass to tensordock start function with tensordock vm id with async
    if instance['provider'] == CloudProvider.TENSORDOCK:
        background_tasks.add_task(tensordock_service.destroy_vm, instance['provider_instance_id'], instance['vm_id'])
    elif instance['provider'] == CloudProvider.GCP:
        # If provider is GCP, pass to gcp destroy function with instance name with async
        background_tasks.add_task(gcp_service.destroy_vm, instance['provider_instance_id'], instance['vm_id'])
    else:
        raise HTTPException(status_code=400, detail=f"Provider {instance['provider']} not supported for destroy operation")

    # Pass back confirmation response to user
    return {"status": VMStatus.DESTROYING, "vm_id": instance['vm_id']}


@router.get("/billing")
async def get_billing(user_id: Optional[str] = None):
    # Get user's instances for billing calculation
    instances = get_instance()
    user_instances = [i for i in instances if user_id is None or i.get('user_id') == user_id]
    
    # Calculate billing from provider APIs
    tensordock_total = 0.0
    gcp_total = 0.0
    
    for instance in user_instances:
        if instance['provider'] == CloudProvider.TENSORDOCK:
            tensordock_total += float(instance.get('hourly_price', 0))
        else:
            gcp_total += float(instance.get('hourly_price', 0))
    
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