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
        [x] Build metadata items with shared startup script
        [x] Configure instance with machine type, disks, and networking
        [x] Add GPU configuration if specified
        [x] Set up scheduling for preemptible instances
        [x] Create the instance using Google Cloud SDK
        [x] Wait for operation completion and get instance details
        [x] Extract IP address and update VM document
        [x] Update database with final instance information
        """
        # Build metadata items with shared startup script
        compute_client = compute_v1.InstancesClient()
        startup_script = StartupScriptService.get_gaming_vm_startup_script()
        metadata_items = [
            {'key': 'ssh-keys', 'value': f"ubuntu:{instance_doc.ssh_key}"},
            {'key': 'startup-script', 'value': startup_script}
        ]

        # Configure instance with machine type, disks, and networking
        config = {
            'name': create_request.name,
            'machine_type': f"zones/{create_request.zone}/machineTypes/{create_request.machine_type}",
            'disks': [{
                'boot': True,
                'auto_delete': True,
                'initialize_params': {
                    'source_image': create_request.source_image,
                    'disk_size_gb': str(create_request.disk_size_gb),
                    'disk_type': f"zones/{create_request.zone}/diskTypes/{create_request.disk_type}"
                }
            }],
            'network_interfaces': [{
                'network': f"projects/{self.project_id}/global/networks/default",
                'access_configs': [{
                    'type': 'ONE_TO_ONE_NAT',
                    'name': 'External NAT'
                }] if create_request.external_ip else []
            }],
            'metadata': {
                'items': metadata_items
            }
        }

        # Add GPU configuration if specified
        if create_request.gpu_type and create_request.gpu_count > 0:
            config['guest_accelerators'] = [{
                'accelerator_type': f"zones/{create_request.zone}/acceleratorTypes/{create_request.gpu_type}",
                'accelerator_count': create_request.gpu_count
            }]

        # Set up scheduling for preemptible instances
        if create_request.preemptible:
            config['scheduling'] = {'preemptible': True}

        # Create the instance using Google Cloud SDK
        operation = compute_client.insert(
            project=self.project_id,
            zone=create_request.zone,
            instance_resource=config
        )

        # Wait for operation completion and get instance details
        result = operation.result()

        if result:
            instance_result = compute_client.get(
                project=self.project_id,
                zone=create_request.zone,
                instance=create_request.name
            )

            # Extract IP address and update VM document
            if (instance_result.network_interfaces and
                instance_result.network_interfaces[0].access_configs):
                instance_doc.ip_address = instance_result.network_interfaces[0].access_configs[0].nat_i_p

            instance_doc.provider_instance_id = f"{create_request.zone}/{create_request.name}"
            instance_doc.status = VMStatus.RUNNING
        else:
            logger.error(f"Failed to create instance {create_request.name}")
            instance_doc.status = VMStatus.ERROR

        # Update database with final instance information
        update_instance_doc(instance_doc.vm_id, instance_doc)

    
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