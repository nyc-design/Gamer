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
        [x] Get supported instance types from console config
        [x] Use Google Cloud SDK to get machine types for each supported type
        [x] Get all zones and machine type details for each zone
        [x] Convert GCloud regions to city, country pairs
        [x] Get hourly pricing for each instance type and region
        [x] Use geocoding service to get coordinates for each region
        [x] Create and return VMAvailableResponse list
        """
        # Get supported instance types from console config
        supported_types = console_config.supported_instance_types.get("gcp", [])
        available_instances = []

        # Use Google Cloud SDK to get machine types for each supported type
        machine_types_client = compute_v1.MachineTypesClient()
        zones_client = compute_v1.ZonesClient()

        for instance_type in supported_types:
            # Get all zones and machine type details for each zone
            zones_request = compute_v1.ListZonesRequest(project=self.project_id)
            zones = zones_client.list(request=zones_request)

            for zone in zones:
                try:
                    request = compute_v1.GetMachineTypeRequest(
                        project=self.project_id,
                        zone=zone.name,
                        machine_type=instance_type
                    )
                    machine_type = machine_types_client.get(request=request)

                    region = zone.name.rsplit('-', 1)[0]  # Extract region from zone

                    # Convert GCloud regions to city, country pairs
                    region_map = {
                        'us-central1': ('Council Bluffs', 'USA'),
                        'us-east1': ('Moncks Corner', 'USA'),
                        'us-east4': ('Ashburn', 'USA'),
                        'us-west1': ('The Dalles', 'USA'),
                        'us-west2': ('Los Angeles', 'USA'),
                        'us-west3': ('Salt Lake City', 'USA'),
                        'us-west4': ('Las Vegas', 'USA'),
                        'europe-west1': ('St. Ghislain', 'Belgium'),
                        'europe-west2': ('London', 'UK'),
                        'europe-west3': ('Frankfurt', 'Germany'),
                        'europe-west4': ('Eemshaven', 'Netherlands'),
                        'europe-west6': ('Zurich', 'Switzerland'),
                        'asia-east1': ('Changhua County', 'Taiwan'),
                        'asia-northeast1': ('Tokyo', 'Japan'),
                        'asia-south1': ('Mumbai', 'India'),
                        'asia-southeast1': ('Jurong West', 'Singapore')
                    }
                    city, country = region_map.get(region, (region, 'Global'))

                    # Get hourly pricing for each instance type and region
                    hourly_price = await self._get_instance_price(instance_type, region)

                    # Use geocoding service to get coordinates for each region
                    location = await self.geocoding_service.get_coordinates(city, country)

                    # Create and return VMAvailableResponse list
                    available_instances.append(VMAvailableResponse(
                        provider=CloudProvider.GCP,
                        instance_type=instance_type,
                        provider_id=region,
                        hourly_price=hourly_price,
                        instance_lat=location[0] if location else 0.0,
                        instance_long=location[1] if location else 0.0,
                        distance_to_user=0.0,
                        gpu="Standard",
                        avail_cpus=machine_type.guest_cpus,
                        avail_ram=machine_type.memory_mb // 1024,
                        avail_disk=100  # Default disk size
                    ))

                except Exception as e:
                    # Machine type not available in this zone, continue
                    continue

        return available_instances
        

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
        [ ] Parse zone and instance name from provider instance ID
        [ ] Stop the instance using Google Cloud SDK
        [ ] Wait for operation completion
        [ ] Update database status to STOPPED or ERROR
        """
        pass
    

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