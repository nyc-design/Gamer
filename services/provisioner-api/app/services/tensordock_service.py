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
        # call tensordock sdk for list available hostnodes with proper gpu count from console_config provider instance types for tensordock, 0th value in array
        tensordock_gpus = console_config.supported_instance_types.get("tensordock", [])

        async with httpx.AsyncClient() as client:
            params = {"token": self.api_token}
            response = await client.get(f"{self.base_url}/locations", params=params)
            response.raise_for_status()
            locations_data = response.json()

        # Filter out any nodes that don't meet min requirements
        available_instances = []
        for location in locations_data.get("data", {}).get("locations", []):
            # Check if any GPU in this location supports dedicated IP (required for gaming/Wolf)
            has_dedicated_ip = any(
                gpu.get("network_features", {}).get("dedicated_ip_available", False)
                for gpu in location.get("gpus", [])
            )
            if not has_dedicated_ip:
                continue

            # Handle GPU-less instances if no GPU is required
            if not tensordock_gpus:
                # For GPU-less instances, create entry based on location's base pricing
                location_coords = await self.geocoding_service.get_coordinates(
                    location.get('city', ''),
                    location.get('country', '')
                )

                # Use base pricing for CPU-only instances (estimate from first GPU's pricing structure)
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
                # Handle GPU instances
                for gpu_config in location.get("gpus", []):
                    # Check GPU requirements
                    if not any(req_gpu in gpu_config.get("v0Name", "") for req_gpu in tensordock_gpus):
                        continue

                    # Check if resources meet minimum requirements
                    resources = gpu_config.get("resources", {})
                    if (resources.get("max_vcpus", 0) >= console_config.min_cpus and
                        resources.get("max_ram_gb", 0) >= console_config.min_ram and
                        resources.get("max_storage_gb", 0) >= console_config.min_disk):

                        # Calculate hourly price by combining gpu price, min_cpus, ram, and disk prices together
                        pricing = gpu_config.get("pricing", {})
                        gpu_price = gpu_config.get("price_per_hr", 0)
                        cpu_price = console_config.min_cpus * pricing.get("per_vcpu_hr", 0)
                        ram_price = console_config.min_ram * pricing.get("per_gb_ram_hr", 0)
                        disk_price = console_config.min_disk * pricing.get("per_gb_storage_hr", 0)
                        hourly_price = gpu_price + cpu_price + ram_price + disk_price

                        # For remaining nodes, pass to geocoding service with city and country field to return lat and long for node
                        location_coords = await self.geocoding_service.get_coordinates(
                            location.get('city', ''),
                            location.get('country', '')
                        )

                        # Create VMAvailableResponse for each node
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

        # Pass back remaining nodes as list of VMAvailableResponse
        return available_instances


    async def create_vm(self, create_request: TensorDockCreateRequest, instance_doc: VMDocument):
        # call deploy VM from tensordock sdk with create_request, await
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}

            # Construct GPU configuration
            gpus = {}
            if create_request.gpu_count > 0:
                gpus[create_request.gpu_model] = {
                    "count": create_request.gpu_count
                }

            payload = {
                "data": {
                    "type": "virtualmachine",
                    "attributes": {
                        "name": create_request.name,
                        "type": "virtualmachine",
                        "image": create_request.image,
                        "resources": {
                            "vcpu_count": create_request.vcpu_count,
                            "ram_gb": create_request.ram_gb,
                            "storage_gb": max(create_request.storage_gb, 100),  # Minimum 100GB required
                            "gpus": gpus
                        },
                        "location_id": create_request.location_id,
                        "useDedicatedIp": True,  # Required for gaming/Wolf
                        "ssh_key": create_request.ssh_key
                    }
                }
            }
            response = await client.post(
                f"{self.base_url}/instances",
                headers=headers,
                json=payload
            )
            response.raise_for_status()
            response_data = response.json()

        # Map response fields to update VMDocument
        instance_doc.provider_instance_id = response_data.get('id')

        # Set status of VMDocument to "CONFIGURING"
        instance_doc.status = VMStatus.CONFIGURING

        # Call mongodb function to update VMDocument in database
        updated_doc = update_instance_doc(instance_doc.vm_id, instance_doc)

        # Poll instance status until it's running and get IP address
        max_attempts = 60  # 10 minutes with 10-second intervals
        for attempt in range(max_attempts):
            await asyncio.sleep(10)  # Wait 10 seconds between checks

            async with httpx.AsyncClient() as status_client:
                status_response = await status_client.get(
                    f"{self.base_url}/instances/{instance_doc.provider_instance_id}",
                    headers=headers
                )
                status_response.raise_for_status()
                status_data = status_response.json()

                instance_status = status_data.get('status')
                if instance_status == 'running':
                    instance_doc.ip_address = status_data.get('ipAddress')
                    break
                elif instance_status in ['failed', 'error']:
                    raise Exception(f"Instance creation failed with status: {instance_status}")

        if not instance_doc.ip_address:
            raise Exception("Instance did not become running within timeout period")

        # Update database with final IP address
        updated_doc = update_instance_doc(instance_doc.vm_id, instance_doc)

        # Call async cloudypad ssh function to deploy image with ssh key for instance, await
        cloudypad_service = CloudyPadService()
        await cloudypad_service.ssh_deploy(instance_doc.provider_instance_name, instance_doc.ssh_key)


    async def start_vm(self, instance_id: str, vm_id: str):
        # call tensordock sdk to start vm with instance_ids
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.post(
                f"{self.base_url}/instances/{instance_id}/start",
                headers=headers
            )
            response.raise_for_status()

            # Poll instance status until it's running
            max_attempts = 30  # 5 minutes with 10-second intervals
            for attempt in range(max_attempts):
                await asyncio.sleep(10)

                status_response = await client.get(
                    f"{self.base_url}/instances/{instance_id}",
                    headers=headers
                )
                status_response.raise_for_status()
                status_data = status_response.json()

                instance_status = status_data.get('status')
                if instance_status == 'running':
                    return status_data
                elif instance_status in ['failed', 'error']:
                    raise Exception(f"Instance start failed with status: {instance_status}")

            raise Exception("Instance did not start within timeout period")

        # Update database status to running
        set_instance_status(vm_id, VMStatus.RUNNING)

    
    async def stop_vm(self, instance_id: str, vm_id: str):
        # call tensordock sdk to stop vm with instance_id
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.post(
                f"{self.base_url}/instances/{instance_id}/stop",
                headers=headers
            )
            response.raise_for_status()

            # Poll instance status until it's stopped
            max_attempts = 30  # 5 minutes with 10-second intervals
            for attempt in range(max_attempts):
                await asyncio.sleep(10)

                status_response = await client.get(
                    f"{self.base_url}/instances/{instance_id}",
                    headers=headers
                )
                status_response.raise_for_status()
                status_data = status_response.json()

                instance_status = status_data.get('status')
                if instance_status == 'stopped':
                    return status_data
                elif instance_status in ['failed', 'error']:
                    raise Exception(f"Instance stop failed with status: {instance_status}")

            raise Exception("Instance did not stop within timeout period")

        # Update database status to stopped
        set_instance_status(vm_id, VMStatus.STOPPED)

    
    async def destroy_vm(self, instance_id: str, vm_id: str):
        # call tensordock sdk to delete vm with instance_id
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.delete(
                f"{self.base_url}/instances/{instance_id}",
                headers=headers
            )
            response.raise_for_status()

            # Poll to verify instance is deleted (it should return 404 when deleted)
            max_attempts = 30  # 5 minutes with 10-second intervals
            for attempt in range(max_attempts):
                await asyncio.sleep(10)

                try:
                    status_response = await client.get(
                        f"{self.base_url}/instances/{instance_id}",
                        headers=headers
                    )
                    status_response.raise_for_status()
                    # If we get here, instance still exists
                    status_data = status_response.json()
                    instance_status = status_data.get('status')
                    if instance_status in ['failed', 'error']:
                        raise Exception(f"Instance deletion failed with status: {instance_status}")
                except httpx.HTTPStatusError as e:
                    if e.response.status_code == 404:
                        # Instance successfully deleted
                        return {"message": "Instance deleted successfully"}
                    else:
                        raise

            raise Exception("Instance was not deleted within timeout period")

        # Update database status to destroyed
        set_instance_status(vm_id, VMStatus.DESTROYED)