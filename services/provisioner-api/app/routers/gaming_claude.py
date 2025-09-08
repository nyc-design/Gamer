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

tensordock = TensorDockService()
gcp = GCPComputeService()
geocoding = GeocodingService()

@router.get("/instances", response_model=List[VMResponse])
async def list_existing_instances(console_type: ConsoleType, user_id: Optional[str] = None):
    """1. Get existing instances that meet console requirements"""
    instances_coll = get_client().gaming.existing_instances
    query = {"console_type": console_type}
    if user_id:
        query["user_id"] = user_id
    
    vms = instances_coll.find(query).to_list(None)
    return [VMResponse(**vm) for vm in vms]

@router.get("/instances/available")
async def list_available_instances(
    console_type: ConsoleType, 
    user_lat: Optional[float] = None, 
    user_lng: Optional[float] = None
):
    """2. Get available instances from both providers, sorted by distance"""
    config = CONSOLE_CONFIGS.get(console_type, {})
    options = []
    
    # TensorDock options
    try:
        min_gpu = 1 if config.get("tensordock_gpus") else 0
        hostnodes = tensordock.client.virtual_machines.get_available_hostnodes(min_gpu_count=min_gpu)
        for node in hostnodes:
            if _node_meets_requirements(node, config):
                distance = None
                if user_lat and user_lng:
                    distance = await geocoding.calculate_distance((user_lat, user_lng), node)
                
                gpus = node.get('specs', {}).get('gpu', [])
                gpu_name = gpus[0]['model'] if gpus else 'No GPU'
                
                options.append({
                    "provider": "tensordock",
                    "location": f"{node.get('city', 'Unknown')}, {node.get('country', '')}",
                    "specs": f"{gpu_name}, {node['specs']['cpu']} CPU, {node['specs']['ram']//1024}GB",
                    "cost_per_hour": _estimate_cost(node, "tensordock"),
                    "distance_km": distance,
                    "config": {"hostnode_id": node["id"]}
                })
    except Exception as e:
        pass  # Continue with GCP even if TensorDock fails
    
    # GCP options
    try:
        regions = gcp.get_all_regions_with_zones()
        for region in regions:
            machine_types = gcp.get_machine_types_for_gaming(region["region_code"])
            for mt in machine_types:
                if mt["name"] in config.get("gcp_types", []) and mt["cpus"] >= config["min_cpu"] and mt["memory_gb"] >= config["min_ram"]:
                    options.append({
                        "provider": "gcp",
                        "location": region.get("location", region["region_code"]),
                        "specs": f"{mt['cpus']} CPU, {mt['memory_gb']}GB RAM",
                        "cost_per_hour": _estimate_cost(mt, "gcp"),
                        "distance_km": None,  # TODO: Calculate GCP distance
                        "config": {"region": region["region_code"], "zone": mt["zone"], "machine_type": mt["name"]}
                    })
    except Exception as e:
        pass
    
    if user_lat and user_lng:
        options.sort(key=lambda x: x["distance_km"] or float('inf'))
    
    return options

@router.post("/instances", response_model=VMResponse)
async def create_instance(
    console_type: ConsoleType,
    provider: str,
    config: Dict[str, Any],
    user_id: Optional[str] = None,
    background_tasks: BackgroundTasks = None
):
    """3. Create instance with specified config"""
    vm_id = str(uuid.uuid4())
    vm = VMDocument(
        vm_id=vm_id,
        status=VMStatus.CREATING,
        console_type=console_type,
        provider=provider,
        user_id=user_id
    )
    
    db = get_database()
    await db.vms.insert_one(vm.dict(by_alias=True, exclude_none=True))
    
    if provider == "tensordock":
        background_tasks.add_task(_create_tensordock, vm_id, config)
    elif provider == "gcp":
        background_tasks.add_task(_create_gcp, vm_id, config)
    
    return VMResponse(**vm.dict())

@router.post("/instances/{vm_id}/start")
async def start_instance(vm_id: str):
    """4. Start existing instance"""
    db = get_database()
    vm = await db.vms.find_one({"vm_id": vm_id})
    if not vm:
        raise HTTPException(404, "VM not found")
    
    # TODO: Call provider API to start
    await db.vms.update_one({"vm_id": vm_id}, {"$set": {"status": VMStatus.RUNNING}})
    return {"status": "starting"}

@router.post("/instances/{vm_id}/stop")
async def stop_instance(vm_id: str):
    """5. Stop existing instance"""
    db = get_database()
    vm = await db.vms.find_one({"vm_id": vm_id})
    if not vm:
        raise HTTPException(404, "VM not found")
    
    # TODO: Call provider API to stop
    await db.vms.update_one({"vm_id": vm_id}, {"$set": {"status": VMStatus.STOPPED}})
    return {"status": "stopping"}

@router.delete("/instances/{vm_id}")
async def destroy_instance(vm_id: str):
    """6. Destroy existing instance"""
    db = get_database()
    vm = await db.vms.find_one({"vm_id": vm_id})
    if not vm:
        raise HTTPException(404, "VM not found")
    
    # TODO: Call provider API to destroy
    await db.vms.delete_one({"vm_id": vm_id})
    return {"status": "destroyed"}

@router.get("/billing")
async def get_billing(user_id: Optional[str] = None):
    """7. View usage and billing across providers"""
    # TODO: Get actual billing data from providers
    return {
        "tensordock": {"total_cost": 0.0, "current_month": 0.0},
        "gcp": {"total_cost": 0.0, "current_month": 0.0},
        "instances": []
    }

def _node_meets_requirements(node: dict, config: dict) -> bool:
    specs = node.get('specs', {})
    if specs.get('cpu', 0) < config["min_cpu"]:
        return False
    if specs.get('ram', 0) < config["min_ram"] * 1024:
        return False
    
    if config.get("tensordock_gpus"):
        gpus = specs.get('gpu', [])
        if not gpus:
            return False
        gpu_model = gpus[0].get('model', '')
        return any(req_gpu in gpu_model for req_gpu in config["tensordock_gpus"])
    
    return True

def _estimate_cost(resource: dict, provider: str) -> float:
    if provider == "tensordock":
        specs = resource.get('specs', {})
        cost = 0.1 + specs.get('cpu', 0) * 0.05 + (specs.get('ram', 0) / 1024) * 0.02
        gpus = specs.get('gpu', [])
        if gpus and 'RTX4090' in gpus[0].get('model', ''):
            cost += 1.0
        return round(cost, 2)
    else:  # gcp
        return round(resource["cpus"] * 0.05 + resource["memory_gb"] * 0.01, 2)

async def _create_tensordock(vm_id: str, config: dict):
    # TODO: Use tensordock API to create VM
    pass

async def _create_gcp(vm_id: str, config: dict):
    # TODO: Use GCP API to create VM
    pass