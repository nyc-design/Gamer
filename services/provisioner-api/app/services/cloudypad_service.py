from google.cloud import compute_v1
from typing import List, Dict, Any, Optional, Tuple
import logging
from app.core.config import settings

logger = logging.getLogger(__name__)

class CloudyPadService:
    
    def __init__(self):
        self.project_id = settings.gcp_project_id
        self.geocoding_service = GeocodingService()
        
    async def list_available_regions(self, console_config: ConsoleConfigDocument, user_location: Optional[Tuple[float, float]] = None):
        # grab supported instance types for each provider from console_config

        # use gcloud command similar to following arg to get regions for each supported instance type and add to master list: gcloud compute machine-types list --filter="name=t2d-standard-4"

        # convert each gcloud region to a city, country pair

        # Get hourly price for each instance type - region pair

        # Parse city, country pair for each region using geocoding get_coordinates function to get lat, long

        # Formulate list of VMAvailableResponse from each region and pass back
        

    async def create_vm(self, create_request: CloudyPadCreateRequest, instance_doc: VMDocument):
        # call create instance from cloudypad cli with mapped cloudypadrequest fields to cli args, as well as transition from snake case "_" to "-", await

        # map additional fields to VMDocument

        # Call mongodb function to update VMDocument in database

        # Set status for instance in database to "RUNNING"

        # Return VMDocument

    
    async def start_vm(self, instance_id: str):
        # call cloudypad start CLI with instance id

    
    async def stop_vm(self, instance_id: str):
        # call cloudypad stop CLI with instance id
    

    async def terminate_vm(self, instance_id: str):
        # call cloudypad destroy CLI with instance id

    
    async def ssh_deploy(self, instance_id: str, ssh_key: str):
        # Use cloudypad create ssh with ssh key and args to deploy image to new tensordock VM, await

        # set status for instance in database to "RUNNING"

        # Return success notification