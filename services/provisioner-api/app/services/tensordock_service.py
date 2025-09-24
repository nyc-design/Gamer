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
        [ ] Call TensorDock API for list of available locations
        [ ] Get supported GPU types from console config
        [ ] Filter locations that support dedicated IP (required for gaming)
        [ ] Handle GPU-less instances if no GPU required
        [ ] Handle GPU instances and check resource requirements
        [ ] Calculate hourly pricing from GPU, CPU, RAM, and disk costs
        [ ] Get coordinates for each location using geocoding service
        [ ] Create and return VMAvailableResponse list
        """
        pass


    async def create_vm(self, create_request: TensorDockCreateRequest, instance_doc: VMDocument):
        """
        Create a TensorDock VM instance with gaming optimizations

        Implementation checklist:
        [x] Build GPU configuration from create request
        [x] Construct API payload with VM specifications
        [x] Call TensorDock API to create instance
        [x] Update VM document with provider instance ID
        [x] Set status to CONFIGURING in database
        [x] Poll instance status until running
        [x] Extract IP address from running instance
        [x] Update database with final IP address
        [x] Deploy CloudyPad image via SSH
        """
        # Build GPU configuration from create request
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}

            gpus = {}
            if create_request.gpu_count > 0:
                gpus[create_request.gpu_model] = {
                    "count": create_request.gpu_count
                }

            # Construct API payload with VM specifications
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

            # Call TensorDock API to create instance
            response = await client.post(
                f"{self.base_url}/instances",
                headers=headers,
                json=payload
            )
            response.raise_for_status()
            response_data = response.json()

        # Update VM document with provider instance ID
        instance_doc.provider_instance_id = response_data.get('id')

        # Set status to CONFIGURING in database
        instance_doc.status = VMStatus.CONFIGURING
        update_instance_doc(instance_doc.vm_id, instance_doc)

        # Poll instance status until running
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
                    # Extract IP address from running instance
                    instance_doc.ip_address = status_data.get('ipAddress')
                    break
                elif instance_status in ['failed', 'error']:
                    raise Exception(f"Instance creation failed with status: {instance_status}")

        if not instance_doc.ip_address:
            raise Exception("Instance did not become running within timeout period")

        # Update database with final IP address
        update_instance_doc(instance_doc.vm_id, instance_doc)

        # Deploy CloudyPad image via SSH
        cloudypad_service = CloudyPadService()
        await cloudypad_service.ssh_deploy(instance_doc.instance_name, instance_doc.ssh_key)


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