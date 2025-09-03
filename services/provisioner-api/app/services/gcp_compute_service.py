from google.cloud import compute_v1
from typing import List, Dict, Any, Optional, Tuple
import logging
from app.core.config import settings

logger = logging.getLogger(__name__)

class GCPComputeService:
    """Simple GCP compute service for listing regions and machine types"""
    
    def __init__(self):
        self.project_id = settings.gcp_project_id
        
    def get_all_regions_with_zones(self) -> List[Dict[str, Any]]:
        """Get all regions with their zones and approximate locations"""
        if not self.project_id:
            return []
            
        try:
            regions_client = compute_v1.RegionsClient()
            zones_client = compute_v1.ZonesClient()
            
            # Get regions
            regions = regions_client.list(project=self.project_id)
            result = []
            
            for region in regions:
                # Get zones in this region
                zones = zones_client.list(project=self.project_id, filter=f"name:{region.name}-*")
                zone_names = [zone.name for zone in zones]
                
                # Extract location info (region names often contain location hints)
                location_info = self._parse_region_location(region.name)
                
                result.append({
                    "region_code": region.name,
                    "region_name": region.description or region.name,
                    "zones": zone_names,
                    "status": region.status,
                    **location_info
                })
                
            return result
            
        except Exception as e:
            logger.error(f"Error getting GCP regions: {e}")
            return []
    
    def get_machine_types_for_gaming(self, region: str) -> List[Dict[str, Any]]:
        """Get suitable machine types for gaming in a specific region"""
        if not self.project_id:
            return []
            
        try:
            client = compute_v1.MachineTypesClient()
            
            # Get zone for this region (use first available zone)
            zones_client = compute_v1.ZonesClient()
            zones = zones_client.list(project=self.project_id, filter=f"name:{region}-*")
            zone = next(iter(zones), None)
            
            if not zone:
                return []
                
            machine_types = client.list(project=self.project_id, zone=zone.name)
            
            # Filter for gaming-suitable instances (4+ CPUs, 8+ GB RAM)
            suitable_types = []
            for mt in machine_types:
                if mt.guest_cpus >= 4 and mt.memory_mb >= 8192:  # 8GB+ RAM
                    suitable_types.append({
                        "name": mt.name,
                        "cpus": mt.guest_cpus,
                        "memory_gb": mt.memory_mb // 1024,
                        "zone": zone.name
                    })
            
            # Sort by performance (CPU count, then RAM)
            suitable_types.sort(key=lambda x: (x["cpus"], x["memory_gb"]))
            return suitable_types
            
        except Exception as e:
            logger.error(f"Error getting machine types for {region}: {e}")
            return []
    
    def _parse_region_location(self, region_name: str) -> Dict[str, Any]:
        """Extract location information from GCP region name"""
        # GCP region naming: continent-location-number
        parts = region_name.split('-')
        
        if len(parts) >= 2:
            continent_map = {
                'us': 'North America',
                'europe': 'Europe', 
                'asia': 'Asia',
                'australia': 'Australia',
                'southamerica': 'South America',
                'northamerica': 'North America',
                'me': 'Middle East',
                'africa': 'Africa'
            }
            
            location_map = {
                'us-central1': 'Iowa, US',
                'us-east1': 'South Carolina, US', 
                'us-east4': 'Northern Virginia, US',
                'us-west1': 'Oregon, US',
                'us-west2': 'Los Angeles, CA, US',
                'europe-west1': 'Belgium',
                'europe-west2': 'London, UK',
                'europe-west3': 'Frankfurt, Germany',
                'asia-east1': 'Taiwan',
                'asia-northeast1': 'Tokyo, Japan',
                'asia-south1': 'Mumbai, India'
            }
            
            continent = continent_map.get(parts[0], 'Unknown')
            location = location_map.get(region_name, f"{parts[1].title()}")
            
            return {
                "continent": continent,
                "location": location,
                "country": location.split(', ')[-1] if ', ' in location else location
            }
            
        return {"continent": "Unknown", "location": "Unknown", "country": "Unknown"}