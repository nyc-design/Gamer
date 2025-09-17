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
from app.core.database import update_instance_doc, set_instance_status

logger = logging.getLogger(__name__)

class GCPComputeService:
    
    def __init__(self):
        self.project_id = settings.gcp_project_id
        self.geocoding_service = GeocodingService()
        self.billing_client = billing_v1.CloudCatalogClient()
        
    async def list_available_regions(self, console_config: ConsoleConfigDocument, user_location: Optional[Tuple[float, float]] = None):
        # grab supported instance types for each provider from console_config
        supported_types = console_config.supported_instance_types.get("gcp", [])
        available_instances = []

        # use google cloud python sdk to get machine types for each supported instance type
        machine_types_client = compute_v1.MachineTypesClient()
        zones_client = compute_v1.ZonesClient()

        for instance_type in supported_types:
            # Get all zones first
            zones_request = compute_v1.ListZonesRequest(project=self.project_id)
            zones = zones_client.list(request=zones_request)

            for zone in zones:
                try:
                    # Get machine type details for this zone
                    request = compute_v1.GetMachineTypeRequest(
                        project=self.project_id,
                        zone=zone.name,
                        machine_type=instance_type
                    )
                    machine_type = machine_types_client.get(request=request)

                    region = zone.name.rsplit('-', 1)[0]  # Extract region from zone

                    # convert each gcloud region to a city, country pair
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

                    # Get hourly price for each instance type - region pair
                    hourly_price = await self._get_instance_price(instance_type, region)

                    # Parse city, country pair for each region using geocoding get_coordinates function to get lat, long
                    location = await self.geocoding_service.get_coordinates(city, country)

                    # Formulate list of VMAvailableResponse from each region and pass back
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
        # Create Compute Engine instance using Google Cloud Python SDK
        instances_client = compute_v1.InstancesClient()

        # Configure instance properties
        instance = compute_v1.Instance()
        instance.name = create_request.name
        instance.machine_type = f"zones/{create_request.zone}/machineTypes/{create_request.machine_type}"

        # Boot disk configuration
        disk = compute_v1.AttachedDisk()
        disk.boot = True
        disk.auto_delete = True
        disk.initialize_params = compute_v1.AttachedDiskInitializeParams()
        disk.initialize_params.source_image = create_request.source_image
        disk.initialize_params.disk_size_gb = str(create_request.disk_size_gb)
        disk.initialize_params.disk_type = f"zones/{create_request.zone}/diskTypes/{create_request.disk_type}"
        instance.disks = [disk]

        # Network configuration
        network_interface = compute_v1.NetworkInterface()
        network_interface.network = f"projects/{self.project_id}/global/networks/{create_request.network.split('/')[-1]}"
        if create_request.external_ip:
            access_config = compute_v1.AccessConfig()
            access_config.type_ = compute_v1.AccessConfig.Type.ONE_TO_ONE_NAT.name
            access_config.name = "External NAT"
            network_interface.access_configs = [access_config]
        instance.network_interfaces = [network_interface]

        # GPU configuration if specified
        if create_request.gpu_type and create_request.gpu_count > 0:
            accelerator = compute_v1.AcceleratorConfig()
            accelerator.accelerator_type = f"zones/{create_request.zone}/acceleratorTypes/{create_request.gpu_type}"
            accelerator.accelerator_count = create_request.gpu_count
            instance.guest_accelerators = [accelerator]

        # Scheduling for preemptible instances
        if create_request.preemptible:
            instance.scheduling = compute_v1.Scheduling()
            instance.scheduling.preemptible = True

        # Metadata for SSH keys and startup script
        instance.metadata = compute_v1.Metadata()
        metadata_items = []

        # Add SSH key
        ssh_key_item = compute_v1.Items()
        ssh_key_item.key = "ssh-keys"
        ssh_key_item.value = f"ubuntu:{instance_doc.ssh_key}"
        metadata_items.append(ssh_key_item)

        # Add startup script if provided
        if create_request.startup_script:
            startup_item = compute_v1.Items()
            startup_item.key = "startup-script"
            startup_item.value = create_request.startup_script
            metadata_items.append(startup_item)

        # Add Tailscale auth key if provided
        if create_request.tailscale_auth_key:
            tailscale_item = compute_v1.Items()
            tailscale_item.key = "tailscale-auth-key"
            tailscale_item.value = create_request.tailscale_auth_key
            metadata_items.append(tailscale_item)

        instance.metadata.items = metadata_items

        # Create the instance
        request = compute_v1.InsertInstanceRequest(
            project=self.project_id,
            zone=create_request.zone,
            instance_resource=instance
        )

        operation = instances_client.insert(request=request)

        # Wait for operation to complete
        zone_operations_client = compute_v1.ZoneOperationsClient()
        while operation.status != compute_v1.Operation.Status.DONE:
            await asyncio.sleep(2)
            operation = zone_operations_client.get(
                project=self.project_id,
                zone=create_request.zone,
                operation=operation.name
            )

        if operation.error:
            logger.error(f"Failed to create instance: {operation.error}")
            instance_doc.status = VMStatus.ERROR
        else:
            # Get instance details for IP address
            instance_result = instances_client.get(
                project=self.project_id,
                zone=create_request.zone,
                instance=create_request.name
            )

            # Extract IP address
            if instance_result.network_interfaces and instance_result.network_interfaces[0].access_configs:
                instance_doc.ip_address = instance_result.network_interfaces[0].access_configs[0].nat_i_p

            instance_doc.provider_instance_id = f"{create_request.zone}/{create_request.name}"
            instance_doc.status = VMStatus.RUNNING

        # Update database
        update_instance_doc(instance_doc.vm_id, instance_doc)

    
    async def start_vm(self, provider_instance_id: str, vm_id: str):
        # Start GCP instance using Python SDK
        instances_client = compute_v1.InstancesClient()

        # Parse zone and instance name from provider_instance_id (format: "zone/instance_name")
        zone, instance_name = provider_instance_id.split('/', 1)

        # Start the instance
        request = compute_v1.StartInstanceRequest(
            project=self.project_id,
            zone=zone,
            instance=instance_name
        )

        operation = instances_client.start(request=request)

        # Wait for operation to complete
        zone_operations_client = compute_v1.ZoneOperationsClient()
        while operation.status != compute_v1.Operation.Status.DONE:
            await asyncio.sleep(2)
            operation = zone_operations_client.get(
                project=self.project_id,
                zone=zone,
                operation=operation.name
            )

        if operation.error:
            logger.error(f"Failed to start instance: {operation.error}")
            set_instance_status(vm_id, VMStatus.ERROR)
        else:
            set_instance_status(vm_id, VMStatus.RUNNING)

    
    async def stop_vm(self, provider_instance_id: str, vm_id: str):
        # Stop GCP instance using Python SDK
        instances_client = compute_v1.InstancesClient()

        # Parse zone and instance name from provider_instance_id (format: "zone/instance_name")
        zone, instance_name = provider_instance_id.split('/', 1)

        # Stop the instance
        request = compute_v1.StopInstanceRequest(
            project=self.project_id,
            zone=zone,
            instance=instance_name
        )

        operation = instances_client.stop(request=request)

        # Wait for operation to complete
        zone_operations_client = compute_v1.ZoneOperationsClient()
        while operation.status != compute_v1.Operation.Status.DONE:
            await asyncio.sleep(2)
            operation = zone_operations_client.get(
                project=self.project_id,
                zone=zone,
                operation=operation.name
            )

        if operation.error:
            logger.error(f"Failed to stop instance: {operation.error}")
            set_instance_status(vm_id, VMStatus.ERROR)
        else:
            set_instance_status(vm_id, VMStatus.STOPPED)
    

    async def destroy_vm(self, provider_instance_id: str, vm_id: str):
        # Delete GCP instance using Python SDK
        instances_client = compute_v1.InstancesClient()

        # Parse zone and instance name from provider_instance_id (format: "zone/instance_name")
        zone, instance_name = provider_instance_id.split('/', 1)

        # Delete the instance
        request = compute_v1.DeleteInstanceRequest(
            project=self.project_id,
            zone=zone,
            instance=instance_name
        )

        operation = instances_client.delete(request=request)

        # Wait for operation to complete
        zone_operations_client = compute_v1.ZoneOperationsClient()
        while operation.status != compute_v1.Operation.Status.DONE:
            await asyncio.sleep(2)
            operation = zone_operations_client.get(
                project=self.project_id,
                zone=zone,
                operation=operation.name
            )

        if operation.error:
            logger.error(f"Failed to destroy instance: {operation.error}")
            set_instance_status(vm_id, VMStatus.ERROR)
        else:
            set_instance_status(vm_id, VMStatus.DESTROYED)

    


    async def _get_instance_price(self, instance_type: str, region: str) -> float:
        # Get real-time pricing from Google Cloud Billing API for specific instance type and region
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
                    'preemptible' in sku.description.lower()):  # Get preemptible/spot pricing

                    # Get the pricing info
                    if sku.pricing_info:
                        pricing = sku.pricing_info[0]
                        if pricing.pricing_expression.tiered_rates:
                            # Get the rate (usually in nanos per unit)
                            rate = pricing.pricing_expression.tiered_rates[0]
                            # Convert from nanos to dollars per hour
                            hourly_price = float(rate.unit_price.nanos) / 1e9
                            return round(hourly_price, 4)

            # Fallback if no pricing found
            return 0.1

        except Exception as e:
            logger.warning(f"Failed to get pricing for {instance_type} in {region}: {e}")
            # Fallback to simple calculation
            return 0.1