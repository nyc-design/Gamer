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
    
    async def create_vm(self, vm_id: str, console_type: ConsoleType, preset: VMPreset, user_location: Optional[Tuple[float, float]] = None) -> Dict[str, Any]:
        """Create a new TensorDock VM instance using optimal hostnode selection"""
        try:
            specs = self._get_vm_specs(preset, console_type)
            
            # Get all available hostnodes with minimum GPU requirements
            min_gpu_count = specs.get("gpu_count", 0)
            available_hostnodes = await asyncio.to_thread(
                self.client.virtual_machines.get_available_hostnodes,
                min_gpu_count=min_gpu_count
            )
            
            if not available_hostnodes:
                logger.error(f"No available hostnodes for VM {vm_id} with minimum {min_gpu_count} GPUs")
                return {
                    "success": False,
                    "error": "No available hostnodes with required specifications"
                }
            
            # Select optimal hostnode based on user location and specs
            selected_hostnode = await self._select_optimal_hostnode(
                available_hostnodes, user_location, specs
            )
            
            if not selected_hostnode:
                logger.error(f"No suitable hostnodes found for VM {vm_id}")
                return {
                    "success": False,
                    "error": "No hostnodes meet the requirements"
                }
            
            hostnode = selected_hostnode['hostnode']
            hostnode_id = hostnode.get("id")
            
            logger.info(f"Selected hostnode {hostnode_id} in {selected_hostnode.get('location')} ({selected_hostnode.get('distance_km', 'unknown')}km away)")
            
            # Deploy VM using SDK
            result = await asyncio.to_thread(
                self.client.virtual_machines.deploy_vm,
                name=vm_id,
                vcpu=specs["cpu"],
                ram=specs["memory"] * 1024,  # Convert GB to MB
                storage=specs["storage"],
                gpu_count=specs.get("gpu_count", 0),
                gpu_model=specs.get("gpu_model") if specs.get("gpu_count", 0) > 0 else None,
                hostnode=hostnode_id,
                image="ubuntu-22.04-cuda",  # CloudyPad compatible image
                password="gamer123!"  # Default password for VM access
            )
            
            if result.get("success"):
                server_info = result.get("server", {})
                logger.info(f"TensorDock VM {vm_id} created successfully on hostnode {hostnode_id}")
                return {
                    "success": True,
                    "instance_id": server_info.get("id"),
                    "ip_address": server_info.get("ip"),
                    "ssh_port": 22,
                    "status": "creating",
                    "hostnode_location": selected_hostnode.get('location'),
                    "distance_km": selected_hostnode.get('distance_km'),
                    "metadata": result
                }
            else:
                logger.error(f"TensorDock VM creation failed: {result.get('error')}")
                return {
                    "success": False,
                    "error": result.get("error", "Unknown error"),
                    "details": result
                }
                    
        except Exception as e:
            logger.error(f"Error creating TensorDock VM {vm_id}: {str(e)}")
            return {
                "success": False,
                "error": str(e)
            }
    
    async def get_vm_status(self, instance_id: str) -> Optional[Dict[str, Any]]:
        """Get VM status from TensorDock using SDK"""
        try:
            result = await asyncio.to_thread(
                self.client.virtual_machines.get_vm_details,
                server_id=instance_id
            )
            
            if result:
                server_info = result.get("server", {})
                
                # Map TensorDock status to our status
                status_mapping = {
                    "active": "running",
                    "building": "creating", 
                    "stopped": "stopped",
                    "error": "error",
                    "deleted": "terminated"
                }
                
                return {
                    "status": status_mapping.get(server_info.get("status"), "unknown"),
                    "ip_address": server_info.get("ip"),
                    "uptime": server_info.get("uptime"),
                    "metadata": result
                }
            else:
                logger.error(f"Failed to get VM status for {instance_id}")
                return None
                    
        except Exception as e:
            logger.error(f"Error getting VM status {instance_id}: {str(e)}")
            return None
    
    async def start_vm(self, instance_id: str) -> bool:
        """Start a stopped TensorDock VM using SDK"""
        try:
            result = await asyncio.to_thread(
                self.client.virtual_machines.start_vm,
                server_id=instance_id
            )
            
            if result.get("success", True):  # SDK might not return success field
                logger.info(f"TensorDock VM {instance_id} started successfully")
                return True
            else:
                logger.error(f"Failed to start VM: {result.get('error', 'Unknown error')}")
                return False
                    
        except Exception as e:
            logger.error(f"Error starting VM {instance_id}: {str(e)}")
            return False
    
    async def stop_vm(self, instance_id: str) -> bool:
        """Stop a running TensorDock VM using SDK"""
        try:
            result = await asyncio.to_thread(
                self.client.virtual_machines.stop_vm,
                server_id=instance_id
            )
            
            if result.get("success", True):  # SDK might not return success field
                logger.info(f"TensorDock VM {instance_id} stopped successfully")
                return True
            else:
                logger.error(f"Failed to stop VM: {result.get('error', 'Unknown error')}")
                return False
                    
        except Exception as e:
            logger.error(f"Error stopping VM {instance_id}: {str(e)}")
            return False
    
    async def terminate_vm(self, instance_id: str) -> bool:
        """Terminate a TensorDock VM using SDK"""
        try:
            result = await asyncio.to_thread(
                self.client.virtual_machines.delete_vm,
                server_id=instance_id
            )
            
            if result.get("success", True):  # SDK might not return success field
                logger.info(f"TensorDock VM {instance_id} terminated successfully")
                return True
            else:
                logger.error(f"Failed to terminate VM: {result.get('error', 'Unknown error')}")
                return False
                    
        except Exception as e:
            logger.error(f"Error terminating VM {instance_id}: {str(e)}")
            return False
    
    async def list_available_hostnodes(self, min_gpu_count: int = 0) -> List[Dict[str, Any]]:
        """Get list of available hostnodes from TensorDock"""
        try:
            result = await asyncio.to_thread(
                self.client.virtual_machines.get_available_hostnodes,
                min_gpu_count=min_gpu_count
            )
            
            if isinstance(result, list):
                return result
            else:
                logger.error(f"Unexpected response format: {result}")
                return []
                    
        except Exception as e:
            logger.error(f"Error getting available hostnodes: {str(e)}")
            return []
    
    async def _select_optimal_hostnode(
        self, 
        hostnodes: List[Dict[str, Any]], 
        user_location: Optional[Tuple[float, float]], 
        min_specs: Dict[str, Any]
    ) -> Optional[Dict[str, Any]]:
        """Select the optimal hostnode based on location and specifications"""
        
        try:
            # If user location is provided, find the closest hostnode
            if user_location:
                result = await self.geocoding_service.find_closest_hostnode(
                    user_location, hostnodes, {
                        'min_vcpu': min_specs['cpu'],
                        'min_ram': min_specs['memory'] * 1024,  # Convert GB to MB
                        'min_gpu_count': min_specs.get('gpu_count', 0),
                        'min_storage': min_specs['storage']
                    }
                )
                return result
            
            # If no user location, select first hostnode that meets specs
            for hostnode in hostnodes:
                if self._hostnode_meets_specs(hostnode, min_specs):
                    return {
                        'hostnode': hostnode,
                        'location': f"{hostnode.get('city', 'Unknown')}, {hostnode.get('country', 'Unknown')}",
                        'distance_km': None
                    }
            
            return None
            
        except Exception as e:
            logger.error(f"Error selecting optimal hostnode: {str(e)}")
            return None
    
    def _hostnode_meets_specs(self, hostnode: Dict[str, Any], min_specs: Dict[str, Any]) -> bool:
        """Check if a hostnode meets minimum specifications"""
        
        try:
            specs = hostnode.get('specs', {})
            
            # Check CPU
            if specs.get('cpu', 0) < min_specs['cpu']:
                return False
            
            # Check RAM (hostnode RAM is in MB)
            if specs.get('ram', 0) < min_specs['memory'] * 1024:
                return False
            
            # Check GPU count
            gpu_list = specs.get('gpu', [])
            if len(gpu_list) < min_specs.get('gpu_count', 0):
                return False
            
            # Check storage
            if specs.get('storage', 0) < min_specs['storage']:
                return False
            
            return True
            
        except Exception as e:
            logger.error(f"Error checking hostnode specs: {str(e)}")
            return False
    
    def _get_vm_specs(self, preset: VMPreset, console_type: ConsoleType) -> Dict[str, Any]:
        """Get VM specifications from MongoDB based on preset and console type"""
        return VMSpecService.get_vm_specs(preset, console_type)
    
    async def get_available_locations(self) -> Dict[str, Any]:
        """Get a summary of all available TensorDock locations"""
        
        try:
            # Get all hostnodes
            all_hostnodes = await asyncio.to_thread(
                self.client.virtual_machines.get_available_hostnodes
            )
            
            if not all_hostnodes:
                return {"error": "No hostnodes available"}
            
            # Get location summary
            location_summary = await self.geocoding_service.get_location_summary(all_hostnodes)
            
            return {
                "provider": "tensordock",
                "total_hostnodes": len(all_hostnodes),
                **location_summary
            }
            
        except Exception as e:
            logger.error(f"Error getting available locations: {str(e)}")
            return {"error": str(e)}