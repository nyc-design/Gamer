import asyncio
import logging
import httpx
from typing import Optional, Dict, Any, List, Tuple
from app.models.vm import (
    VMPreset, ConsoleType, ConsoleConfigDocument,
    VMDocument, TensorDockCreateRequest, VMAvailableResponse, CloudProvider, VMStatus
)
from app.services.geocoding_service import GeocodingService
from app.services.cloudypad_service import CloudyPadService
from app.core.database import update_instance_doc, set_instance_status
from app.core.config import settings

logger = logging.getLogger(__name__)

class TensorDockService:
    """Service for managing TensorDock VMs via their API"""
    
    def __init__(self):
        self.api_token = settings.tensordock_api_token
        self.base_url = "https://dashboard.tensordock.com/api/v2"
        self.geocoding_service = GeocodingService()

    
    async def list_available_hostnodes(self, console_config: ConsoleConfigDocument, user_location: Optional[Tuple[float, float]] = None):
        """
        List available TensorDock host nodes for console requirements

        Implementation checklist:
        [x] Call TensorDock API for list of available locations
        [x] Get supported GPU types from console config
        [x] Filter locations that support dedicated IP (required for gaming)
        [x] Handle GPU-less instances if no GPU required
        [x] Handle GPU instances and check resource requirements
        [x] Calculate hourly pricing from GPU, CPU, RAM, and disk costs
        [x] Get coordinates for each location using geocoding service
        [x] Create and return VMAvailableResponse list
        """
        # Call TensorDock API for list of available locations
        tensordock_gpus = console_config.supported_instance_types.get("tensordock", [])

        async with httpx.AsyncClient() as client:
            params = {"token": self.api_token}
            response = await client.get(f"{self.base_url}/locations", params=params)
            response.raise_for_status()
            locations_data = response.json()

        # Get supported GPU types from console config
        available_instances = []
        for location in locations_data.get("data", {}).get("locations", []):
            # Filter locations that support dedicated IP (required for gaming)
            has_dedicated_ip = any(
                gpu.get("network_features", {}).get("dedicated_ip_available", False)
                for gpu in location.get("gpus", [])
            )
            if not has_dedicated_ip:
                continue

            # Handle GPU-less instances if no GPU required
            if not tensordock_gpus:
                location_coords = await self.geocoding_service.get_coordinates(
                    location.get('city', ''),
                    location.get('country', '')
                )

                # Calculate hourly pricing from GPU, CPU, RAM, and disk costs
                base_gpu = location.get("gpus", [{}])[0] if location.get("gpus") else {}
                pricing = base_gpu.get("pricing", {})
                cpu_price = console_config.min_cpus * pricing.get("per_vcpu_hr", 0.003)
                ram_price = console_config.min_ram * pricing.get("per_gb_ram_hr", 0.002)
                disk_price = console_config.min_disk * pricing.get("per_gb_storage_hr", 0.00005)
                hourly_price = cpu_price + ram_price + disk_price

                available_instances.append(VMAvailableResponse(
                    provider=CloudProvider.TENSORDOCK,
                    instance_type="CPU-only",
                    provider_id=location.get('id'),
                    hourly_price=hourly_price,
                    instance_lat=location_coords[0] if location_coords else 0.0,
                    instance_long=location_coords[1] if location_coords else 0.0,
                    distance_to_user=0.0,
                    gpu="No GPU",
                    avail_cpus=128,  # Max available from location
                    avail_ram=300,   # Max available from location
                    avail_disk=1000  # Max available from location
                ))
            else:
                # Handle GPU instances and check resource requirements
                for gpu_config in location.get("gpus", []):
                    if not any(req_gpu in gpu_config.get("v0Name", "") for req_gpu in tensordock_gpus):
                        continue

                    resources = gpu_config.get("resources", {})
                    if (resources.get("max_vcpus", 0) >= console_config.min_cpus and
                        resources.get("max_ram_gb", 0) >= console_config.min_ram and
                        resources.get("max_storage_gb", 0) >= console_config.min_disk):

                        pricing = gpu_config.get("pricing", {})
                        gpu_price = gpu_config.get("price_per_hr", 0)
                        cpu_price = console_config.min_cpus * pricing.get("per_vcpu_hr", 0)
                        ram_price = console_config.min_ram * pricing.get("per_gb_ram_hr", 0)
                        disk_price = console_config.min_disk * pricing.get("per_gb_storage_hr", 0)
                        hourly_price = gpu_price + cpu_price + ram_price + disk_price

                        # Get coordinates for each location using geocoding service
                        location_coords = await self.geocoding_service.get_coordinates(
                            location.get('city', ''),
                            location.get('country', '')
                        )

                        # Create and return VMAvailableResponse list
                        available_instances.append(VMAvailableResponse(
                            provider=CloudProvider.TENSORDOCK,
                            instance_type=gpu_config.get("displayName", ""),
                            provider_id=f"{location.get('id')}_{gpu_config.get('v0Name')}",
                            hourly_price=hourly_price,
                            instance_lat=location_coords[0] if location_coords else 0.0,
                            instance_long=location_coords[1] if location_coords else 0.0,
                            distance_to_user=0.0,
                            gpu=gpu_config.get("displayName", "No GPU"),
                            avail_cpus=resources.get("max_vcpus", 0),
                            avail_ram=resources.get("max_ram_gb", 0),
                            avail_disk=resources.get("max_storage_gb", 0)
                        ))

        return available_instances


    async def create_vm(self, create_request: TensorDockCreateRequest, instance_doc: VMDocument):
        """
        Create a TensorDock VM instance with gaming optimizations

        Implementation checklist:
        [ ] Build GPU configuration from create request
        [ ] Construct API payload with VM specifications
        [ ] Call TensorDock API to create instance
        [ ] Update VM document with provider instance ID
        [ ] Set status to CONFIGURING in database
        [ ] Poll instance status until running
        [ ] Extract IP address from running instance
        [ ] Update database with final IP address
        [ ] Deploy CloudyPad image via SSH
        """
        pass


    async def start_vm(self, instance_id: str, vm_id: str):
        """
        Start a stopped TensorDock VM instance

        Implementation checklist:
        [ ] Call TensorDock API to start instance
        [ ] Poll instance status until running
        [ ] Update database status to RUNNING or ERROR
        """
        pass

    
    async def stop_vm(self, instance_id: str, vm_id: str):
        """
        Stop a running TensorDock VM instance

        Implementation checklist:
        [ ] Call TensorDock API to stop instance
        [ ] Poll instance status until stopped
        [ ] Update database status to STOPPED or ERROR
        """
        pass

    
    async def destroy_vm(self, instance_id: str, vm_id: str):
        """
        Permanently delete a TensorDock VM instance

        Implementation checklist:
        [ ] Call TensorDock API to delete instance
        [ ] Poll to verify instance is deleted (404 response)
        [ ] Update database status to DESTROYED or ERROR
        """
        pass