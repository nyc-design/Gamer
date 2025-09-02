from fastapi import APIRouter, HTTPException, BackgroundTasks
from typing import List, Optional
from app.models.vm import VMCreateRequest, VMResponse, VMStatusResponse, VMStatus, VMPreset, VMDocument, CloudProvider
from app.models.emulator_config import EmulatorConfigDocument
from app.services.cloudypad_service import CloudyPadService
from app.services.tensordock_service import TensorDockService
from app.services.vm_orchestrator import VMOrchestrator
from app.services.vm_store import VMStore
from app.core.database import get_database
import uuid
from datetime import datetime

router = APIRouter()

# In-memory VM store for now (replace with database later)
vm_store = VMStore()
cloudypad_service = CloudyPadService()
tensordock_service = TensorDockService()
vm_orchestrator = VMOrchestrator()

@router.post("/create", response_model=VMResponse)
async def create_vm(vm_request: VMCreateRequest, background_tasks: BackgroundTasks):
    """Create a new gaming VM with automatically determined preset"""
    
    # Get emulator configuration to determine preset
    db = get_database()
    config_data = await db.emulator_configs.find_one({"console_type": vm_request.console_type})
    if not config_data:
        raise HTTPException(
            status_code=404, 
            detail=f"No configuration found for {vm_request.console_type}"
        )
    config = EmulatorConfigDocument(**config_data)
    
    # Generate unique VM ID
    vm_id = str(uuid.uuid4())
    
    # Determine provider
    provider = vm_request.provider
    if not provider:
        # Use first enabled provider from config
        enabled_providers = [p for p in config.preferred_providers if p.get("enabled", True)]
        if not enabled_providers:
            raise HTTPException(
                status_code=503, 
                detail=f"No enabled providers available for {vm_request.console_type}"
            )
        provider = enabled_providers[0]["provider"]
    
    # Create VM record with preset from config
    vm_record = VMResponse(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        preset=config.default_preset,
        console_type=vm_request.console_type,
        provider=provider,
        created_at=datetime.utcnow(),
        updated_at=datetime.utcnow(),
        auto_stop_timeout=vm_request.auto_stop_timeout
    )
    
    # Store VM record in in-memory store
    vm_store.create_vm(vm_record)
    
    # Also create VMDocument in database for orchestrator
    vm_document = VMDocument(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        preset=config.default_preset,
        console_type=vm_request.console_type,
        provider=provider,
        user_id=vm_request.user_id,
        auto_stop_timeout=vm_request.auto_stop_timeout,
        user_location=vm_request.user_location
    )
    
    # Insert into database for orchestrator to use
    await db.vms.insert_one(vm_document.dict(by_alias=True, exclude_none=True))
    
    # Start VM provisioning using orchestrator (handles both TensorDock and CloudyPad)
    background_tasks.add_task(
        vm_orchestrator.provision_and_launch_game,
        vm_id,
        vm_request.game_id or "default",  # Use game_id if provided, otherwise default
        None  # No save_id for initial VM creation
    )
    
    return vm_record

@router.get("/{vm_id}/status", response_model=VMStatusResponse)
async def get_vm_status(vm_id: str):
    """Get current status of a VM"""
    
    vm = vm_store.get_vm(vm_id)
    if not vm:
        raise HTTPException(status_code=404, detail="VM not found")
    
    # Get live status from provider if VM is running
    if vm.status in [VMStatus.RUNNING, VMStatus.CREATING]:
        live_status = None
        if vm.provider == CloudProvider.TENSORDOCK:
            # For TensorDock, we need the provider_instance_id from database
            db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            if vm_data and vm_data.get("provider_instance_id"):
                live_status = await tensordock_service.get_vm_status(vm_data["provider_instance_id"])
        else:
            # For CloudyPad providers
            live_status = await cloudypad_service.get_vm_status(vm_id)
        
        if live_status:
            vm.status = live_status.get("status", vm.status)
            if "ip_address" in live_status:
                vm.ip_address = live_status["ip_address"]
            vm_store.update_vm(vm)
    
    return VMStatusResponse(
        vm_id=vm.vm_id,
        status=vm.status,
        ip_address=vm.ip_address,
        uptime_seconds=None,  # TODO: Calculate from created_at
        last_activity=vm.updated_at
    )

@router.post("/{vm_id}/stop")
async def stop_vm(vm_id: str):
    """Stop a running VM"""
    
    vm = vm_store.get_vm(vm_id)
    if not vm:
        raise HTTPException(status_code=404, detail="VM not found")
    
    if vm.status not in [VMStatus.RUNNING, VMStatus.CREATING]:
        raise HTTPException(status_code=400, detail=f"Cannot stop VM in {vm.status} state")
    
    # Stop VM via appropriate provider
    success = await vm_orchestrator.stop_vm(vm_id)
    if not success:
        raise HTTPException(status_code=500, detail="Failed to stop VM")
    
    # Update VM status
    vm.status = VMStatus.STOPPED
    vm.updated_at = datetime.utcnow()
    vm_store.update_vm(vm)
    
    return {"message": f"VM {vm_id} stopped successfully"}

@router.post("/{vm_id}/start")
async def start_vm(vm_id: str):
    """Start a stopped VM"""
    
    vm = vm_store.get_vm(vm_id)
    if not vm:
        raise HTTPException(status_code=404, detail="VM not found")
    
    if vm.status != VMStatus.STOPPED:
        raise HTTPException(status_code=400, detail=f"Cannot start VM in {vm.status} state")
    
    # Start VM via appropriate provider using orchestrator
    # Note: VMOrchestrator doesn't have a start_vm method, so we'll use provider services directly
    if vm.provider == CloudProvider.TENSORDOCK:
        # For TensorDock, we need the provider_instance_id from database
        db = get_database()
        vm_data = await db.vms.find_one({"vm_id": vm_id})
        if not vm_data or not vm_data.get("provider_instance_id"):
            raise HTTPException(status_code=400, detail="TensorDock instance ID not found")
        success = await tensordock_service.start_vm(vm_data["provider_instance_id"])
    else:
        # For CloudyPad providers
        success = await cloudypad_service.start_vm(vm_id)
    
    if not success:
        raise HTTPException(status_code=500, detail="Failed to start VM")
    
    # Update VM status
    vm.status = VMStatus.RUNNING
    vm.updated_at = datetime.utcnow()
    vm_store.update_vm(vm)
    
    return {"message": f"VM {vm_id} started successfully"}

@router.post("/{vm_id}/delete")
async def delete_vm(vm_id: str):
    """Terminate and delete a VM"""
    
    vm = vm_store.get_vm(vm_id)
    if not vm:
        raise HTTPException(status_code=404, detail="VM not found")
    
    # Terminate VM via appropriate provider
    success = await vm_orchestrator.terminate_vm(vm_id)
    if not success:
        raise HTTPException(status_code=500, detail="Failed to terminate VM")
    
    # Remove from store
    vm_store.delete_vm(vm_id)
    
    return {"message": f"VM {vm_id} terminated successfully"}

@router.get("/", response_model=List[VMResponse])
async def list_vms(status: Optional[VMStatus] = None):
    """List VMs, optionally filtered by status"""
    vms = vm_store.list_vms()
    
    if status:
        vms = [vm for vm in vms if vm.status == status]
    
    return vms

@router.get("/running", response_model=List[VMResponse])
async def list_running_vms():
    """List all running VMs"""
    vms = vm_store.list_vms()
    return [vm for vm in vms if vm.status == VMStatus.RUNNING]

@router.get("/stopped", response_model=List[VMResponse])
async def list_stopped_vms():
    """List all stopped VMs"""
    vms = vm_store.list_vms()
    return [vm for vm in vms if vm.status == VMStatus.STOPPED]