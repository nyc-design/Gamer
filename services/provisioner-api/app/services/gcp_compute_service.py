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
        [x] Parse zone and instance name from provider instance ID
        [x] Start the instance using Google Cloud SDK
        [x] Wait for operation completion
        [x] Update database status to RUNNING or ERROR
        """
        # Parse zone and instance name from provider instance ID
        compute_client = compute_v1.InstancesClient()
        zone, instance_name = provider_instance_id.split('/', 1)

        try:
            # Start the instance using Google Cloud SDK
            operation = compute_client.start(
                project=self.project_id,
                zone=zone,
                instance=instance_name
            )
            # Wait for operation completion
            operation.result()
            # Update database status to RUNNING or ERROR
            set_instance_status(vm_id, VMStatus.RUNNING)
        except Exception as e:
            logger.error(f"Failed to start instance {instance_name}: {e}")
            set_instance_status(vm_id, VMStatus.ERROR)

    
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
        [x] Parse zone and instance name from provider instance ID
        [x] Delete the instance using Google Cloud SDK
        [x] Wait for operation completion
        [x] Update database status to DESTROYED or ERROR
        """
        # Parse zone and instance name from provider instance ID
        compute_client = compute_v1.InstancesClient()
        zone, instance_name = provider_instance_id.split('/', 1)

        try:
            # Delete the instance using Google Cloud SDK
            operation = compute_client.delete(
                project=self.project_id,
                zone=zone,
                instance=instance_name
            )
            # Wait for operation completion
            operation.result()
            # Update database status to DESTROYED or ERROR
            set_instance_status(vm_id, VMStatus.DESTROYED)
        except Exception as e:
            logger.error(f"Failed to destroy instance {instance_name}: {e}")
            set_instance_status(vm_id, VMStatus.ERROR)

    



    async def _get_instance_price(self, instance_type: str, region: str) -> float:
        """
        Get real-time pricing from Google Cloud Billing API for specific instance type and region

        Implementation checklist:
        [x] List services to find Compute Engine service
        [x] Get SKUs for the compute service filtered by machine type and region
        [x] Look for SKUs that match our instance type and region
        [x] Get preemptible/spot pricing from pricing info
        [x] Convert from nanos to dollars per hour
        [x] Return fallback price if no pricing found
        """
        try:
            # List services to find Compute Engine service
            services = self.billing_client.list_services()
            compute_service = None
            for service in services:
                if 'compute' in service.display_name.lower():
                    compute_service = service
                    break

            if not compute_service:
                return 0.1  # Fallback price

            # Get SKUs for the compute service filtered by machine type and region
            skus = self.billing_client.list_skus(parent=compute_service.name)

            for sku in skus:
                # Look for SKUs that match our instance type and region
                if (instance_type in sku.description.lower() and
                    region in sku.service_regions and
                    'preemptible' in sku.description.lower()):

                    # Get preemptible/spot pricing from pricing info
                    if sku.pricing_info:
                        pricing = sku.pricing_info[0]
                        if pricing.pricing_expression.tiered_rates:
                            rate = pricing.pricing_expression.tiered_rates[0]
                            # Convert from nanos to dollars per hour
                            hourly_price = float(rate.unit_price.nanos) / 1e9
                            return round(hourly_price, 4)

            # Return fallback price if no pricing found
            return 0.1

        except Exception as e:
            logger.warning(f"Failed to get pricing for {instance_type} in {region}: {e}")
            return 0.1