from google.cloud import compute_v1, billing_v1
from typing import List, Dict, Any, Optional, Tuple
import logging
import subprocess
import asyncio
from app.core.config import settings
from app.models.vm import (
    ConsoleConfigDocument, VMDocument, CloudyPadCreateRequest,
    VMAvailableResponse, CloudProvider, VMStatus
)
from app.services.geocoding_service import GeocodingService
from app.core.database import update_instance_doc, set_instance_status

logger = logging.getLogger(__name__)

class CloudyPadService:
    
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
                        provider=CloudProvider.CLOUDYPAD_GCP,
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
        

    async def create_vm(self, create_request: CloudyPadCreateRequest, instance_doc: VMDocument):
        # call create instance from cloudypad cli with mapped cloudypadrequest fields to cli args, as well as transition from snake case "_" to "-", await
        request_dict = create_request.dict(by_alias=True)
        args = []
        for key, value in request_dict.items():
            cli_key = key.replace('_', '-')
            args.extend([f'--{cli_key}', str(value)])

        cmd = ['cloudypad', 'create', create_request.provider] + args
        result = await asyncio.create_subprocess_exec(*cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        stdout, stderr = await result.communicate()

        # map additional fields to VMDocument
        output = stdout.decode().strip()
        # Parse CloudyPad output for instance details (implementation depends on CloudyPad CLI output format)
        instance_doc.provider_instance_id = create_request.name
        instance_doc.status = VMStatus.RUNNING

        # Call mongodb function to update VMDocument in database
        update_instance_doc(instance_doc.vm_id, instance_doc)

    
    async def start_vm(self, instance_name: str, vm_id: str):
        # call cloudypad start CLI with instance id
        result = await asyncio.create_subprocess_exec('cloudypad', 'start', instance_name, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return await result.communicate()

        set_instance_status(vm_id, VMStatus.RUNNING)

    
    async def stop_vm(self, instance_name: str, vm_id: str):
        # call cloudypad stop CLI with instance id
        result = await asyncio.create_subprocess_exec('cloudypad', 'stop', instance_name, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return await result.communicate()

        set_instance_status(vm_id, VMStatus.STOPPED)
    

    async def destroy_vm(self, instance_name: str, vm_id: str):
        # call cloudypad destroy CLI with instance id
        result = await asyncio.create_subprocess_exec('cloudypad', 'destroy', instance_name, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        return await result.communicate()

        set_instance_status(vm_id, VMStatus.DESTROYED)

    
    async def ssh_deploy(self, instance_name: str, ssh_key: str):
        # Use cloudypad create ssh with ssh key and args to deploy image to new tensordock VM, await
        result = await asyncio.create_subprocess_exec(
            'cloudypad', 'create', 'ssh', instance_name, '--key', ssh_key,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE
        )
        stdout, stderr = await result.communicate()

        # set status for instance in database to "RUNNING"
        set_instance_status(instance_id, VMStatus.RUNNING)

        # Return success notification
        return {"success": result.returncode == 0, "output": stdout.decode().strip()}


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