import asyncio
import logging
import httpx
from typing import Optional, Dict, Any, List, Tuple
from app.models.vm import (
    VMPreset, ConsoleType, ConsoleConfigDocument,
    VMDocument, TensorDockCreateRequest, VMAvailableResponse, CloudProvider, VMStatus
)
from app.services.geocoding_service import GeocodingService
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
        [x] Call TensorDock API to start instance
        [x] Poll instance status until running
        [x] Update database status to RUNNING or ERROR
        """
        # Call TensorDock API to start instance
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.post(
                f"{self.base_url}/instances/{instance_id}/start",
                headers=headers
            )
            response.raise_for_status()

            # Poll instance status until running
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
                    # Update database status to RUNNING or ERROR
                    set_instance_status(vm_id, VMStatus.RUNNING)
                    return status_data
                elif instance_status in ['failed', 'error']:
                    set_instance_status(vm_id, VMStatus.ERROR)
                    raise Exception(f"Instance start failed with status: {instance_status}")

            set_instance_status(vm_id, VMStatus.ERROR)
            raise Exception("Instance did not start within timeout period")

    
    async def stop_vm(self, instance_id: str, vm_id: str):
        """
        Stop a running TensorDock VM instance

        Implementation checklist:
        [x] Call TensorDock API to stop instance
        [x] Poll instance status until stopped
        [x] Update database status to STOPPED or ERROR
        """
        # Call TensorDock API to stop instance
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.post(
                f"{self.base_url}/instances/{instance_id}/stop",
                headers=headers
            )
            response.raise_for_status()

            # Poll instance status until stopped
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
                    # Update database status to STOPPED or ERROR
                    set_instance_status(vm_id, VMStatus.STOPPED)
                    return status_data
                elif instance_status in ['failed', 'error']:
                    set_instance_status(vm_id, VMStatus.ERROR)
                    raise Exception(f"Instance stop failed with status: {instance_status}")

            set_instance_status(vm_id, VMStatus.ERROR)
            raise Exception("Instance did not stop within timeout period")

    
    async def destroy_vm(self, instance_id: str, vm_id: str):
        """
        Permanently delete a TensorDock VM instance

        Implementation checklist:
        [x] Call TensorDock API to delete instance
        [x] Poll to verify instance is deleted (404 response)
        [x] Update database status to DESTROYED or ERROR
        """
        # Call TensorDock API to delete instance
        async with httpx.AsyncClient() as client:
            headers = {"Authorization": f"Bearer {self.api_token}"}
            response = await client.delete(
                f"{self.base_url}/instances/{instance_id}",
                headers=headers
            )
            response.raise_for_status()

            # Poll to verify instance is deleted (404 response)
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
                        set_instance_status(vm_id, VMStatus.ERROR)
                        raise Exception(f"Instance deletion failed with status: {instance_status}")
                except httpx.HTTPStatusError as e:
                    if e.response.status_code == 404:
                        # Instance successfully deleted
                        # Update database status to DESTROYED or ERROR
                        set_instance_status(vm_id, VMStatus.DESTROYED)
                        return {"message": "Instance deleted successfully"}
                    else:
                        raise

            set_instance_status(vm_id, VMStatus.ERROR)
            raise Exception("Instance was not deleted within timeout period")