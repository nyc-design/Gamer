from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import List, Optional, Dict, Any
from app.models.vm import (
    VMResponse, VMDocument, VMStatus, ConsoleType, VMCreateRequest, 
    VMAvailableResponse, VMStatusResponse, TensorDockCreateRequest, 
    CloudyPadCreateRequest, ConsoleConfigDocument, CloudProvider
)
from app.services.tensordock_service import TensorDockService
from app.services.gcp_compute_service import GCPComputeService
from app.services.geocoding_service import GeocodingService
from app.services.cloudypad_service import CloudyPadService
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
cloudypad_service = CloudyPadService()


@router.get("/instances/available", response_model=List[VMAvailableResponse])
async def list_available_instances(console_type: ConsoleType, user_lat: Optional[float] = None, user_lng: Optional[float] = None):
    # Take console type, and call MongoDB database function to get config for that console
    console_config = get_console_config(console_type)
    if not console_config:
        raise HTTPException(status_code=404, detail=f"Console config not found for {console_type}")

    # Take config and user lat / long and pass it to tensordock service function to find all instances, return as list
    user_location = (user_lat, user_lng) if user_lat and user_lng else None
    tensordock_instances = await tensordock_service.list_available_hostnodes(console_config, user_location)

    # Take config and user lat / long and pass it to cloudypad service function to find all instances, return as list
    cloudypad_instances = await cloudypad_service.list_available_instances(console_config, user_location)

    # Combine all lists, then pass each instance to calculate_distance function from geocoding service to get distance to user
    all_instances = tensordock_instances + cloudypad_instances
    
    # Sort by distance if user location provided
    if user_location:
        all_instances.sort(key=lambda x: x.distance_to_user or float('inf'))

    # Pass back list of VM Responses as VMAvailableResponse models
    return all_instances


@router.post("/instances/create", response_model=VMResponse)
async def create_instance(console_type: ConsoleType, create_request: VMCreateRequest, user_id: Optional[str] = None, background_tasks: BackgroundTasks = None):
    # Create password for instance
    password = ''.join(secrets.choice(string.ascii_letters + string.digits) for _ in range(16))

    # Create ssh_key for instance
    private_key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    ssh_key = private_key.private_bytes(
        encoding=serialization.Encoding.PEM,
        format=serialization.PrivateFormat.PKCS8,
        encryption_algorithm=serialization.NoEncryption()
    ).decode('utf-8')

    # Add instance to MongoDB database with status "provisioning"
    vm_id = str(uuid.uuid4())
    vm_doc = VMDocument(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_types=[console_type],
        provider=create_request.provider,
        provider_instance_name=create_request.provider_instance_name,
        instance_type=create_request.instance_type,
        instance_lat=create_request.instance_lat,
        instance_long=create_request.instance_long,
        hourly_price=create_request.hourly_price,
        os=create_request.os,
        gpu=getattr(create_request, 'gpu', ''),
        num_cpus=create_request.num_cpus or 4,
        num_ram=create_request.num_ram or 8,
        num_disk=create_request.num_disk or 50,
        auto_stop_timeout=create_request.auto_stop_timeout,
        ssh_key=ssh_key,
        instance_password=password,
        user_id=user_id
    )
    add_new_instance(vm_doc, VMStatus.CREATING)

    # Check provider in create request
    if create_request.provider == CloudProvider.TENSORDOCK:
        # If tensordock, map to TensorDockCreateRequest model and pass to tensordock create function as async
        td_request = TensorDockCreateRequest(
            password=password,
            ssh_key=ssh_key,
            provider_instance_name=create_request.provider_instance_name,
            instance_type=create_request.instance_type,
            num_cpus=create_request.num_cpus or 4,
            num_ram=create_request.num_ram or 8,
            provider_instance_id=create_request.provider_id,
            num_disk=create_request.num_disk or 50,
            os=create_request.os
        )
        background_tasks.add_task(tensordock_service.create_vm, td_request, vm_doc)
    else:
        # If gcp or others, map to CloudyPadCreateRequest model and pass to cloudypad service create function as async
        cp_request = CloudyPadCreateRequest(
            provider_instance_name=create_request.provider_instance_name,
            instance_type=create_request.instance_type,
            num_disk=create_request.num_disk or 50,
            provider_instance_id=create_request.provider_id
        )
        background_tasks.add_task(cloudypad_service.create_vm, cp_request, vm_doc)

    # pass back confirmation response to user
    return VMResponse(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_type=console_type,
        provider=create_request.provider,
        instance_type=create_request.instance_type,
        hourly_price=create_request.hourly_price,
        created_at=vm_doc.created_at,
        instance_lat=create_request.instance_lat,
        instance_long=create_request.instance_long
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
            vm_id=instance['vm_id'],
            status=instance['status'],
            console_type=console_type,
            provider=instance['provider'],
            instance_type=instance['instance_type'],
            hourly_price=instance['hourly_price'],
            created_at=instance['created_at'],
            instance_lat=instance['instance_lat'],
            instance_long=instance['instance_long'],
            last_activity=instance.get('last_activity')
        )
        for instance in filtered_instances
    ]


@router.post("/instances/{vm_id}/start")
async def start_instance(vm_id: str):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "starting" and return updated doc
    updated_instance = set_instance_status(vm_id, VMStatus.RUNNING)

    # Grab provider instance ID from update doc
    provider = instance['provider']
    provider_instance_id = instance.get('provider_instance_id')

    # If provider is tensordock, pass to tensordock start function with tensordock vm id with async
    if provider == CloudProvider.TENSORDOCK:
        await tensordock_service.start_vm(provider_instance_id)
    else:
        # If provider is GCP or other, pass to cloudypad start function with instance name with async
        await cloudypad_service.start_vm(instance['provider_instance_name'])

    # Pass back confirmation response to user
    return {"status": "starting", "vm_id": vm_id}


@router.post("/instances/{vm_id}/stop")
async def stop_instance(vm_id: str):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "stopping"
    set_instance_status(vm_id, VMStatus.STOPPED)

    # Grab provider info for API calls
    provider = instance['provider']
    provider_instance_id = instance.get('provider_instance_id')

    # If provider is tensordock, pass to tensordock stop function with tensordock vm id with async
    if provider == CloudProvider.TENSORDOCK:
        await tensordock_service.stop_vm(provider_instance_id)
    else:
        # If provider is GCP or other, pass to cloudypad stop function with instance name with async
        await cloudypad_service.stop_vm(instance['provider_instance_name'])

    # Pass back confirmation response to user
    return {"status": "stopping", "vm_id": vm_id}


@router.delete("/instances/{vm_id}/destroy")
async def destroy_instance(vm_id: str):
    # Call MongoDB to get instance doc
    instance = get_instance(vm_id)
    if not instance:
        raise HTTPException(status_code=404, detail="VM instance not found")

    # Call MongoDB status update to update status to "destroying"
    set_instance_status(vm_id, VMStatus.DESTROYING)

    # Grab provider info for API calls
    provider = instance['provider']
    provider_instance_id = instance.get('provider_instance_id')

    # If provider is tensordock, pass to tensordock terminate function with tensordock vm id with async
    if provider == CloudProvider.TENSORDOCK:
        await tensordock_service.terminate_vm(provider_instance_id)
    else:
        # If provider is GCP or other, pass to cloudypad terminate function with instance name with async
        await cloudypad_service.terminate_vm(instance['provider_instance_name'])

    # Pass back confirmation response to user
    return {"status": "destroying", "vm_id": vm_id}


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