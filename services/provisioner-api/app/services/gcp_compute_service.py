from google.cloud import compute_v1, billing_v1
from typing import List, Dict, Any, Optional, Tuple
import logging
import time
import asyncio
from app.core.config import settings
from app.models.vm import (
    ConsoleConfigDocument, VMDocument, GCPCreateRequest,
    VMAvailableResponse, CloudProvider, VMStatus, GCPVMType
)
import subprocess
from app.services.geocoding_service import GeocodingService
from app.services.startup_script_service import StartupScriptService
from app.core.database import update_instance_doc, set_instance_status

logger = logging.getLogger(__name__)

class GCPComputeService:
    
    def __init__(self):
        self.project_id = settings.gcp_project_id
        self.geocoding_service = GeocodingService()
        self.billing_client = billing_v1.CloudCatalogClient()
        
    async def list_available_regions(self, console_config: ConsoleConfigDocument, user_location: Optional[Tuple[float, float]] = None):
        """
        List available GCP regions and machine types for console requirements

        Implementation checklist:
        [ ] Get supported instance types from console config
        [ ] Use Google Cloud SDK to get machine types for each supported type
        [ ] Get all zones and machine type details for each zone
        [ ] Convert GCloud regions to city, country pairs
        [ ] Get hourly pricing for each instance type and region
        [ ] Use geocoding service to get coordinates for each region
        [ ] Create and return VMAvailableResponse list
        """
        pass
        

    async def create_vm(self, create_request: GCPCreateRequest, instance_doc: VMDocument):
        """
        Create a GCP Compute Engine VM instance with gaming optimizations

        Implementation checklist:
        [ ] Build metadata items with shared startup script
        [ ] Configure instance with machine type, disks, and networking
        [ ] Add GPU configuration if specified
        [ ] Set up scheduling for preemptible instances
        [ ] Create the instance using Google Cloud SDK
        [ ] Wait for operation completion and get instance details
        [ ] Extract IP address and update VM document
        [ ] Update database with final instance information
        """
        pass

    
    async def start_vm(self, provider_instance_id: str, vm_id: str):
        """
        Start a stopped GCP VM instance

        Implementation checklist:
        [ ] Parse zone and instance name from provider instance ID
        [ ] Start the instance using Google Cloud SDK
        [ ] Wait for operation completion
        [ ] Update database status to RUNNING or ERROR
        """
        pass

    
    async def stop_vm(self, provider_instance_id: str, vm_id: str):
        """
        Stop a running GCP VM instance

        Implementation checklist:
        [x] Parse zone and instance name from provider instance ID
        [x] Stop the instance using Google Cloud SDK
        [x] Wait for operation completion
        [x] Update database status to STOPPED or ERROR
        """
        # Parse zone and instance name from provider instance ID
        compute_client = compute_v1.InstancesClient()
        zone, instance_name = provider_instance_id.split('/', 1)

        try:
            # Stop the instance using Google Cloud SDK
            operation = compute_client.stop(
                project=self.project_id,
                zone=zone,
                instance=instance_name
            )
            # Wait for operation completion
            operation.result()
            # Update database status to STOPPED or ERROR
            set_instance_status(vm_id, VMStatus.STOPPED)
        except Exception as e:
            logger.error(f"Failed to stop instance {instance_name}: {e}")
            set_instance_status(vm_id, VMStatus.ERROR)
    

    async def destroy_vm(self, provider_instance_id: str, vm_id: str):
        """
        Permanently delete a GCP VM instance

        Implementation checklist:
        [ ] Parse zone and instance name from provider instance ID
        [ ] Delete the instance using Google Cloud SDK
        [ ] Wait for operation completion
        [ ] Update database status to DESTROYED or ERROR
        """
        pass

    



    async def _get_instance_price(self, instance_type: str, region: str) -> float:
        """
        Get real-time pricing from Google Cloud Billing API for specific instance type and region

        Implementation checklist:
        [ ] List services to find Compute Engine service
        [ ] Get SKUs for the compute service filtered by machine type and region
        [ ] Look for SKUs that match our instance type and region
        [ ] Get preemptible/spot pricing from pricing info
        [ ] Convert from nanos to dollars per hour
        [ ] Return fallback price if no pricing found
        """
        return 0.1  # Fallback price