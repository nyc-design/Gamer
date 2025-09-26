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