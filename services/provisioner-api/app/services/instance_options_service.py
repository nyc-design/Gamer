from typing import List, Tuple, Optional
from app.models.console_config import ConsoleConfig, InstanceOption
from app.models.vm import ConsoleType
from app.services.tensordock_service import TensorDockService
from app.services.gcp_compute_service import GCPComputeService
from app.core.sync_database import get_sync_database
import asyncio
import logging

logger = logging.getLogger(__name__)

class InstanceOptionsService:
    """Service for getting available VM instances for user selection"""
    
    def __init__(self):
        self.tensordock = TensorDockService()
        self.gcp = GCPComputeService()
    
    async def get_available_instances(
        self, 
        console_type: ConsoleType, 
        user_location: Optional[Tuple[float, float]] = None
    ) -> List[InstanceOption]:
        """Get all available instance options for a console type, sorted by distance"""
        
        # Get console config
        config = self._get_console_config(console_type)
        if not config:
            return []
        
        options = []
        
        # Get TensorDock options
        tensordock_options = await self._get_tensordock_options(config, user_location)
        options.extend(tensordock_options)
        
        # Get GCP options
        gcp_options = self._get_gcp_options(config, user_location)
        options.extend(gcp_options)
        
        # Sort by distance if location provided
        if user_location:
            options.sort(key=lambda x: x.distance_km or float('inf'))
        
        return options
    
    def _get_console_config(self, console_type: ConsoleType) -> Optional[ConsoleConfig]:
        """Get console configuration from database"""
        try:
            db = get_sync_database()
            config_data = db.console_configs.find_one({"console_type": console_type})
            return ConsoleConfig(**config_data) if config_data else None
        except Exception as e:
            logger.error(f"Error getting console config: {e}")
            return None
    
    async def _get_tensordock_options(
        self, 
        config: ConsoleConfig, 
        user_location: Optional[Tuple[float, float]]
    ) -> List[InstanceOption]:
        """Get available TensorDock instances"""
        try:
            # Get hostnodes with required GPUs
            min_gpu_count = 1 if config.tensordock_gpus else 0
            hostnodes = await asyncio.to_thread(
                self.tensordock.client.virtual_machines.get_available_hostnodes,
                min_gpu_count=min_gpu_count
            )
            
            options = []
            for hostnode in hostnodes:
                # Check if hostnode meets requirements
                if not self._hostnode_matches_config(hostnode, config):
                    continue
                    
                # Calculate distance
                distance = None
                if user_location:
                    distance = await self.tensordock.geocoding_service.calculate_distance(
                        user_location, hostnode
                    )
                
                # Get GPU info
                gpus = hostnode.get('specs', {}).get('gpu', [])
                gpu_str = gpus[0]['model'] if gpus else 'No GPU'
                
                options.append(InstanceOption(
                    provider="tensordock",
                    location=f"{hostnode.get('city', 'Unknown')}, {hostnode.get('country', '')}",
                    specs=f"{gpu_str}, {hostnode['specs']['cpu']} CPU, {hostnode['specs']['ram']//1024}GB RAM",
                    cost_per_hour=self._estimate_tensordock_cost(hostnode),
                    distance_km=distance,
                    provider_data={
                        "hostnode_id": hostnode["id"],
                        "hostnode_data": hostnode
                    }
                ))
            
            return options
            
        except Exception as e:
            logger.error(f"Error getting TensorDock options: {e}")
            return []
    
    def _get_gcp_options(
        self, 
        config: ConsoleConfig, 
        user_location: Optional[Tuple[float, float]]
    ) -> List[InstanceOption]:
        """Get available GCP instances"""
        try:
            regions = self.gcp.get_all_regions_with_zones()
            options = []
            
            for region_data in regions:
                machine_types = self.gcp.get_machine_types_for_gaming(region_data["region_code"])
                
                for mt in machine_types:
                    # Check if machine type is in allowed list
                    if config.gcp_machine_types and mt["name"] not in config.gcp_machine_types:
                        continue
                    
                    # Check minimum requirements
                    if mt["cpus"] < config.min_cpu or mt["memory_gb"] < config.min_ram_gb:
                        continue
                    
                    # Calculate distance (would need GCP region coordinates)
                    distance = None  # TODO: Calculate distance to GCP region
                    
                    options.append(InstanceOption(
                        provider="gcp",
                        location=f"{region_data.get('location', region_data['region_code'])}",
                        specs=f"{mt['cpus']} CPU, {mt['memory_gb']}GB RAM",
                        cost_per_hour=self._estimate_gcp_cost(mt),
                        distance_km=distance,
                        provider_data={
                            "region": region_data["region_code"],
                            "zone": mt["zone"],
                            "machine_type": mt["name"]
                        }
                    ))
            
            return options
            
        except Exception as e:
            logger.error(f"Error getting GCP options: {e}")
            return []
    
    def _hostnode_matches_config(self, hostnode: dict, config: ConsoleConfig) -> bool:
        """Check if hostnode meets console requirements"""
        specs = hostnode.get('specs', {})
        
        # Check CPU and RAM
        if specs.get('cpu', 0) < config.min_cpu:
            return False
        if specs.get('ram', 0) < config.min_ram_gb * 1024:  # Convert GB to MB
            return False
        
        # Check GPU if required
        if config.tensordock_gpus:
            gpus = specs.get('gpu', [])
            if not gpus:
                return False
            
            # Check if any GPU matches requirements
            for gpu in gpus:
                gpu_model = gpu.get('model', '')
                for required_gpu in config.tensordock_gpus:
                    if required_gpu in gpu_model:
                        return True
            return False
        
        return True
    
    def _estimate_tensordock_cost(self, hostnode: dict) -> float:
        """Estimate hourly cost for TensorDock hostnode"""
        # Simple cost estimation based on specs
        specs = hostnode.get('specs', {})
        base_cost = 0.1  # Base cost
        
        # Add cost per CPU
        base_cost += specs.get('cpu', 0) * 0.05
        
        # Add cost per GB RAM  
        base_cost += (specs.get('ram', 0) / 1024) * 0.02
        
        # Add GPU cost
        gpus = specs.get('gpu', [])
        if gpus:
            gpu_model = gpus[0].get('model', '')
            if 'RTX4090' in gpu_model:
                base_cost += 1.0
            elif 'RTX3080' in gpu_model:
                base_cost += 0.6
            else:
                base_cost += 0.3
        
        return round(base_cost, 2)
    
    def _estimate_gcp_cost(self, machine_type: dict) -> float:
        """Estimate hourly cost for GCP machine type"""
        # Rough GCP pricing estimates
        cpu_cost = machine_type["cpus"] * 0.05
        ram_cost = machine_type["memory_gb"] * 0.01
        return round(cpu_cost + ram_cost, 2)