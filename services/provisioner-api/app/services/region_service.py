import logging
from typing import Optional, Dict, Any, List, Tuple
from geopy.distance import geodesic
import asyncio

logger = logging.getLogger(__name__)

class RegionService:
    """Service for selecting optimal regions based on user location"""
    
    def __init__(self):
        # TensorDock region coordinates (approximate data center locations)
        self.tensordock_regions = {
            "us-central": {
                "name": "US Central",
                "location": (39.7392, -104.9903),  # Denver, CO
                "code": "us-central"
            },
            "us-east": {
                "name": "US East",
                "location": (40.7589, -73.9851),  # New York, NY
                "code": "us-east"
            },
            "us-west": {
                "name": "US West", 
                "location": (37.7749, -122.4194),  # San Francisco, CA
                "code": "us-west"
            },
            "eu-west": {
                "name": "Europe West",
                "location": (53.3498, -6.2603),  # Dublin, Ireland
                "code": "eu-west"
            },
            "ap-southeast": {
                "name": "Asia Pacific Southeast",
                "location": (1.3521, 103.8198),  # Singapore
                "code": "ap-southeast"
            }
        }
        
        # CloudyPad region coordinates
        self.cloudypad_regions = {
            "us-central": {
                "name": "US Central",
                "location": (41.2524, -95.9980),  # Nebraska
                "code": "us-central"
            },
            "us-east": {
                "name": "US East",
                "location": (39.0458, -76.6413),  # Maryland
                "code": "us-east"
            },
            "us-west": {
                "name": "US West",
                "location": (45.5152, -122.6784),  # Portland, OR
                "code": "us-west"
            },
            "eu-central": {
                "name": "Europe Central",
                "location": (50.1109, 8.6821),  # Frankfurt, Germany
                "code": "eu-central"
            }
        }
    
    def get_optimal_region(
        self, 
        user_lat: float, 
        user_lon: float, 
        provider: str = "tensordock"
    ) -> Optional[Dict[str, Any]]:
        """Get the optimal region for a user based on their location"""
        
        try:
            user_location = (user_lat, user_lon)
            
            # Select the appropriate region mapping
            regions = self.tensordock_regions if provider == "tensordock" else self.cloudypad_regions
            
            # Calculate distances to all regions
            distances = []
            for region_code, region_info in regions.items():
                distance_km = geodesic(user_location, region_info["location"]).kilometers
                distances.append({
                    "region_code": region_code,
                    "region_name": region_info["name"],
                    "distance_km": distance_km,
                    "provider": provider
                })
            
            # Sort by distance and return closest
            distances.sort(key=lambda x: x["distance_km"])
            closest = distances[0]
            
            logger.info(f"Optimal region for user at ({user_lat}, {user_lon}): "
                       f"{closest['region_name']} ({closest['distance_km']:.0f}km away)")
            
            return {
                "region_code": closest["region_code"],
                "region_name": closest["region_name"], 
                "distance_km": closest["distance_km"],
                "provider": provider,
                "all_options": distances[:3]  # Return top 3 options
            }
            
        except Exception as e:
            logger.error(f"Error calculating optimal region: {str(e)}")
            # Fallback to default region
            default_region = "us-central"
            return {
                "region_code": default_region,
                "region_name": regions.get(default_region, {}).get("name", "US Central"),
                "distance_km": None,
                "provider": provider,
                "fallback": True
            }
    
    def get_region_for_static_location(self, location_name: str, provider: str = "tensordock") -> Optional[Dict[str, Any]]:
        """Get optimal region for a known location (e.g., Dallas)"""
        
        # Known city coordinates
        known_locations = {
            "dallas": (32.7767, -96.7970),
            "new_york": (40.7589, -73.9851),
            "los_angeles": (34.0522, -118.2437),
            "chicago": (41.8781, -87.6298),
            "london": (51.5074, -0.1278),
            "singapore": (1.3521, 103.8198),
            "frankfurt": (50.1109, 8.6821),
            "denver": (39.7392, -104.9903)
        }
        
        location_key = location_name.lower().replace(" ", "_")
        if location_key in known_locations:
            lat, lon = known_locations[location_key]
            return self.get_optimal_region(lat, lon, provider)
        
        logger.warning(f"Unknown location: {location_name}, using default region")
        return self.get_optimal_region(32.7767, -96.7970, provider)  # Default to Dallas
    
    async def get_region_recommendations(
        self, 
        user_lat: float, 
        user_lon: float
    ) -> Dict[str, Any]:
        """Get region recommendations for all providers"""
        
        try:
            # Get recommendations for both providers
            tensordock_region = self.get_optimal_region(user_lat, user_lon, "tensordock")
            cloudypad_region = self.get_optimal_region(user_lat, user_lon, "cloudypad")
            
            return {
                "user_location": {
                    "latitude": user_lat,
                    "longitude": user_lon
                },
                "recommendations": {
                    "tensordock": tensordock_region,
                    "cloudypad": cloudypad_region
                },
                "optimal_provider": self._select_optimal_provider(tensordock_region, cloudypad_region)
            }
            
        except Exception as e:
            logger.error(f"Error getting region recommendations: {str(e)}")
            return {
                "user_location": {"latitude": user_lat, "longitude": user_lon},
                "recommendations": {},
                "error": str(e)
            }
    
    def _select_optimal_provider(self, tensordock_region: Dict, cloudypad_region: Dict) -> Dict[str, Any]:
        """Select the optimal provider based on distance and other factors"""
        
        try:
            td_distance = tensordock_region.get("distance_km", float('inf'))
            cp_distance = cloudypad_region.get("distance_km", float('inf'))
            
            # For now, simple distance-based selection
            # In the future, could factor in cost, availability, etc.
            if td_distance <= cp_distance:
                return {
                    "provider": "tensordock",
                    "region": tensordock_region,
                    "reason": f"Closer by {cp_distance - td_distance:.0f}km" if cp_distance != float('inf') else "Available"
                }
            else:
                return {
                    "provider": "cloudypad", 
                    "region": cloudypad_region,
                    "reason": f"Closer by {td_distance - cp_distance:.0f}km"
                }
                
        except Exception as e:
            logger.error(f"Error selecting optimal provider: {str(e)}")
            return {
                "provider": "tensordock",
                "region": tensordock_region or {"region_code": "us-central"},
                "reason": "Fallback selection"
            }
    
    def validate_location(self, latitude: float, longitude: float) -> bool:
        """Validate that location coordinates are reasonable"""
        return -90 <= latitude <= 90 and -180 <= longitude <= 180