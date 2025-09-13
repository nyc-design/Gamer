import logging
from typing import Dict, Any, Optional, Tuple
from geopy.geocoders import Nominatim
from geopy.distance import geodesic
import asyncio
from functools import lru_cache

logger = logging.getLogger(__name__)

class GeocodingService:
    """Service for converting city/country names to GPS coordinates"""
    
    def __init__(self):
        self.geocoder = Nominatim(user_agent="gamer-cloud-gaming-platform")
        
        # Cache common city/country combinations to avoid repeated API calls
        self._coordinate_cache = {}
    
    @lru_cache(maxsize=1000)
    def get_coordinates(self, city: str, region: str = None, country: str = None) -> Optional[Tuple[float, float]]:
        """Synchronous version with LRU cache for better performance"""
        
        try:
            # Build the query string
            query_parts = []
            if city:
                query_parts.append(city)
            if region:
                query_parts.append(region)
            if country:
                query_parts.append(country)
            
            query = ", ".join(query_parts)
            
            location = self.geocoder.geocode(query)
            
            if location:
                coordinates = (location.latitude, location.longitude)
                logger.debug(f"Geocoded '{query}' -> {coordinates}")
                return coordinates
            else:
                logger.warning(f"Could not geocode location: {query}")
                return None
                
        except Exception as e:
            logger.error(f"Error geocoding '{city}, {region}, {country}': {str(e)}")
            return None
    
    
    def calculate_distance(self, user_coords: Tuple[float, float], hostnode: Dict[str, Any]) -> Optional[float]:
        """Synchronous version for better performance in loops"""
        
        try:
            # Extract location info from hostnode
            city = hostnode.get('city')
            region = hostnode.get('region')
            country = hostnode.get('country')
            
            if not city or not country:
                logger.warning(f"Incomplete location data for hostnode {hostnode.get('id')}: city={city}, country={country}")
                return None
            
            # Get coordinates for the hostnode location
            hostnode_coords = self.get_coordinates_sync(city, region, country)
            if not hostnode_coords:
                return None
            
            # Calculate distance using geodesic
            distance = geodesic(user_coords, hostnode_coords).kilometers
            return distance
            
        except Exception as e:
            logger.error(f"Error calculating distance to hostnode: {str(e)}")
            return None