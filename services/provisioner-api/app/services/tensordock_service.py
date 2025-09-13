import asyncio
import logging
from typing import Optional, Dict, Any, List, Tuple
from tensordock import TensorDockAPI
from app.models.vm import VMPreset, ConsoleType
from app.services.geocoding_service import GeocodingService
from app.services.vm_spec_service import VMSpecService
from app.core.config import settings

logger = logging.getLogger(__name__)

class TensorDockService:
    """Service for managing TensorDock VMs via their API"""
    
    def __init__(self):
        self.api_key = settings.tensordock_api_key
        self.api_token = getattr(settings, 'tensordock_api_token', self.api_key)
        self.client = TensorDockAPI(self.api_key, self.api_token)
        self.geocoding_service = GeocodingService()
        self.cloudypad_service = CloudyPadService()

    
    async def list_available_hostnodes(self, console_config: ConsoleConfigDocument, user_location: Optional[Tuple[float, float]] = None):
        # call tensordock sdk for list available hostnodes with proper gpu count from console_config provider instance types for tensordock, 0th value in array

        # Filter out any nodes that don't meet min requirements

        # Calculate hourly price by combining gpu price, min_cpus, ram, and disk prices together

        # For remaining nodes, pass to geocoding service with city and country field to return lat and long for node

        # Pass back remaining nodes as list of VMAvailableResponse


    async def create_vm(self, create_request: TensorDockCreateRequest, instance_doc: VMDocument):
        # call deploy VM from tensordock sdk with create_request

        # Map response fields to update VMDocument

        # Set status of VMDocument to "CONFIGURING"

        # Call mongodb function to update VMDocument in database

        # Call async cloudypad ssh function to deploy image with ssh key for instance

        # Return VMDocument

    
    async def start_vm(self, instance_id: str):
        # call tensordock sdk to start vm with instance_id

    
    async def stop_vm(self, instance_id: str):
        # call tensordock sdk to stop vm with instance_id

    
    async def terminate_vm(self, instance_id: str):
        # call tensordock sdk to delet vm with instance_id