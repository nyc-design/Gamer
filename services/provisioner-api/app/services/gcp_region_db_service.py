from typing import Dict, Any, Optional, List, Tuple
from app.models.gcp_region import GCPRegionDocument
from app.core.sync_database import get_sync_database
from geopy.distance import geodesic
import logging

logger = logging.getLogger(__name__)

class GCPRegionDatabaseService:
    """Service for managing GCP region data from MongoDB"""
    
    @staticmethod
    def get_closest_region(user_location: Tuple[float, float]) -> Optional[Dict[str, Any]]:
        """Get the closest GCP region to the user's location from MongoDB"""
        
        try:
            db = get_sync_database()
            user_lat, user_lon = user_location
            
            # Get all active regions from MongoDB
            regions_data = list(db.gcp_regions.find({"is_active": True}))
            if not regions_data:
                logger.error("No GCP regions found in database")
                return None
            
            closest_region = None
            min_distance = float('inf')
            
            for region_data in regions_data:
                region = GCPRegionDocument(**region_data)
                region_location = (region.latitude, region.longitude)
                distance = geodesic(user_location, region_location).kilometers
                
                if distance < min_distance:
                    min_distance = distance
                    closest_region = {
                        "region_code": region.region_code,
                        "region_name": f"{region.display_name}, {region.country}",
                        "location": {
                            "latitude": region.latitude,
                            "longitude": region.longitude
                        },
                        "distance_km": round(distance, 1),
                        "country": region.country,
                        "continent": region.continent
                    }
            
            if closest_region:
                logger.info(f"Closest GCP region for ({user_lat}, {user_lon}): {closest_region['region_name']} ({closest_region['distance_km']}km)")
            
            return closest_region
            
        except Exception as e:
            logger.error(f"Error finding closest GCP region from database: {str(e)}")
            return None
    
    @staticmethod
    def get_top_regions(user_location: Tuple[float, float], limit: int = 5) -> List[Dict[str, Any]]:
        """Get the top N closest GCP regions to the user's location from MongoDB"""
        
        try:
            db = get_sync_database()
            regions_data = list(db.gcp_regions.find({"is_active": True}))
            
            regions_with_distance = []
            
            for region_data in regions_data:
                region = GCPRegionDocument(**region_data)
                region_location = (region.latitude, region.longitude)
                distance = geodesic(user_location, region_location).kilometers
                
                regions_with_distance.append({
                    "region_code": region.region_code,
                    "region_name": f"{region.display_name}, {region.country}",
                    "location": {
                        "latitude": region.latitude,
                        "longitude": region.longitude
                    },
                    "distance_km": round(distance, 1),
                    "country": region.country,
                    "continent": region.continent
                })
            
            # Sort by distance and return top N
            regions_with_distance.sort(key=lambda x: x['distance_km'])
            return regions_with_distance[:limit]
            
        except Exception as e:
            logger.error(f"Error getting top GCP regions from database: {str(e)}")
            return []
    
    @staticmethod
    def get_all_regions() -> Dict[str, Any]:
        """Get all available GCP regions from MongoDB"""
        
        try:
            db = get_sync_database()
            regions_data = list(db.gcp_regions.find({"is_active": True}))
            
            regions_by_continent = {}
            
            for region_data in regions_data:
                region = GCPRegionDocument(**region_data)
                continent = region.continent
                
                if continent not in regions_by_continent:
                    regions_by_continent[continent] = []
                
                regions_by_continent[continent].append({
                    "region_code": region.region_code,
                    "region_name": f"{region.display_name}, {region.country}",
                    "location": {
                        "latitude": region.latitude,
                        "longitude": region.longitude
                    },
                    "country": region.country
                })
            
            return {
                "total_regions": len(regions_data),
                "regions_by_continent": regions_by_continent
            }
            
        except Exception as e:
            logger.error(f"Error getting all GCP regions from database: {str(e)}")
            return {"error": str(e)}
    
    @staticmethod
    def validate_region(region_code: str) -> bool:
        """Validate that a GCP region exists in MongoDB"""
        try:
            db = get_sync_database()
            region = db.gcp_regions.find_one({"region_code": region_code, "is_active": True})
            return region is not None
        except Exception as e:
            logger.error(f"Error validating region {region_code}: {str(e)}")
            return False
    
    @staticmethod
    def get_region_info(region_code: str) -> Optional[Dict[str, Any]]:
        """Get information about a specific GCP region from MongoDB"""
        
        try:
            db = get_sync_database()
            region_data = db.gcp_regions.find_one({"region_code": region_code, "is_active": True})
            if not region_data:
                return None
            
            region = GCPRegionDocument(**region_data)
            return {
                "region_code": region.region_code,
                "region_name": f"{region.display_name}, {region.country}",
                "location": {
                    "latitude": region.latitude,
                    "longitude": region.longitude
                },
                "country": region.country,
                "continent": region.continent
            }
            
        except Exception as e:
            logger.error(f"Error getting info for region {region_code}: {str(e)}")
            return None
    
    @staticmethod
    def create_or_update_region(region_data: Dict[str, Any]) -> GCPRegionDocument:
        """Create or update a GCP region in MongoDB"""
        try:
            db = get_sync_database()
            
            # Check if region exists
            existing = db.gcp_regions.find_one({"region_code": region_data["region_code"]})
            
            region = GCPRegionDocument(**region_data)
            
            if existing:
                # Update existing
                result = db.gcp_regions.replace_one(
                    {"region_code": region.region_code},
                    region.dict(by_alias=True, exclude_none=True)
                )
                logger.info(f"Updated GCP region: {region.region_code}")
            else:
                # Create new
                result = db.gcp_regions.insert_one(
                    region.dict(by_alias=True, exclude_none=True)
                )
                logger.info(f"Created new GCP region: {region.region_code}")
                
            return region
            
        except Exception as e:
            logger.error(f"Error creating/updating GCP region: {str(e)}")
            raise