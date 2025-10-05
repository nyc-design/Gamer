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