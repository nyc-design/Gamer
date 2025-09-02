import logging
from typing import Dict, Any, Optional, Tuple
from app.services.gcp_region_db_service import GCPRegionDatabaseService

logger = logging.getLogger(__name__)

class GCPRegionService:
    """Service for mapping user location to optimal GCP regions for CloudyPad"""
    
    def __init__(self):
        # Remove Google Cloud Location Finder API - it doesn't provide lat/long data
        # Use MongoDB-based region lookup instead
        pass

    def get_closest_region(self, user_location: Tuple[float, float]) -> Optional[Dict[str, Any]]:
        """Get the closest GCP region to the user's location from MongoDB"""
        return GCPRegionDatabaseService.get_closest_region(user_location)
    
    def get_top_regions(self, user_location: Tuple[float, float], limit: int = 5) -> list:
        """Get the top N closest GCP regions to the user's location from MongoDB"""
        return GCPRegionDatabaseService.get_top_regions(user_location, limit)
    
    def get_all_regions(self) -> Dict[str, Any]:
        """Get all available GCP regions from MongoDB"""
        return GCPRegionDatabaseService.get_all_regions()
    
    def validate_region(self, region_code: str) -> bool:
        """Validate that a GCP region exists in MongoDB"""
        return GCPRegionDatabaseService.validate_region(region_code)
    
    def get_region_info(self, region_code: str) -> Optional[Dict[str, Any]]:
        """Get information about a specific GCP region from MongoDB"""
        return GCPRegionDatabaseService.get_region_info(region_code)
    
    def get_regions_by_country(self, country: str) -> list:
        """Get all GCP regions in a specific country from MongoDB"""
        try:
            all_regions = self.get_all_regions()
            matching_regions = []
            
            for continent_regions in all_regions.get("regions_by_continent", {}).values():
                for region in continent_regions:
                    if region["country"].lower() == country.lower():
                        matching_regions.append(region)
            
            return matching_regions
            
        except Exception as e:
            logger.error(f"Error getting regions for country {country}: {str(e)}")
            return []