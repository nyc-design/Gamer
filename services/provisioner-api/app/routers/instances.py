from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import List, Optional
from app.models.vm import VMResponse, VMDocument, VMStatus, ConsoleType
from app.models.console_config import InstanceOption
from app.services.instance_options_service import InstanceOptionsService
from app.core.database import get_database
import uuid
from datetime import datetime

router = APIRouter()
instance_service = InstanceOptionsService()

@router.get("/", response_model=List[VMResponse])
async def list_existing_vms(
    console_type: Optional[ConsoleType] = None,
    user_id: Optional[str] = None
):
    """Get existing VMs, optionally filtered by console type and user"""
    
    db = get_database()
    query = {}
    
    if console_type:
        query["console_type"] = console_type
    if user_id:
        query["user_id"] = user_id
        
    vm_data_list = await db.vms.find(query).to_list(None)
    vms = [VMDocument(**vm_data) for vm_data in vm_data_list]
    
    return [VMResponse(**vm.dict()) for vm in vms]

@router.get("/available", response_model=List[InstanceOption])
async def get_available_instances(
    console_type: ConsoleType,
    user_latitude: Optional[float] = None,
    user_longitude: Optional[float] = None
):
    """Get available instances that can be created for a console type"""
    
    user_location = None
    if user_latitude and user_longitude:
        user_location = (user_latitude, user_longitude)
    
    try:
        options = await instance_service.get_available_instances(console_type, user_location)
        return options
    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error getting instances: {str(e)}")

@router.post("/create", response_model=VMResponse) 
async def create_vm_from_option(
    console_type: ConsoleType,
    provider: str,
    provider_data: dict,
    user_id: Optional[str] = None,
    background_tasks: BackgroundTasks = None
):
    """Create VM from user's selected instance option"""
    
    vm_id = str(uuid.uuid4())
    
    # Create simplified VM document
    vm = VMDocument(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_type=console_type,
        provider=provider,
        user_id=user_id
    )
    
    # Save to database
    db = get_database()
    await db.vms.insert_one(vm.dict(by_alias=True, exclude_none=True))
    
    # Start provisioning based on provider
    if provider == "tensordock":
        background_tasks.add_task(
            _provision_tensordock_vm, 
            vm_id, provider_data
        )
    elif provider == "gcp":
        background_tasks.add_task(
            _provision_gcp_vm,
            vm_id, provider_data  
        )
    else:
        raise HTTPException(status_code=400, detail=f"Unknown provider: {provider}")
    
    return VMResponse(**vm.dict())

async def _provision_tensordock_vm(vm_id: str, provider_data: dict):
    """Provision TensorDock VM using selected hostnode"""
    # TODO: Use TensorDock service to create VM with specific hostnode
    pass

async def _provision_gcp_vm(vm_id: str, provider_data: dict):  
    """Provision GCP VM using selected machine type and region"""
    # TODO: Use GCP compute service to create VM
    pass