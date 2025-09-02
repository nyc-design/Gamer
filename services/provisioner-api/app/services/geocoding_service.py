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
    
    async def get_coordinates(self, city: str, region: str = None, country: str = None) -> Optional[Tuple[float, float]]:
        """Get GPS coordinates for a city/region/country combination"""
        
        try:
            # Create a cache key
            cache_key = f"{city}_{region or ''}_{country or ''}"
            if cache_key in self._coordinate_cache:
                return self._coordinate_cache[cache_key]
            
            # Build the query string
            query_parts = []
            if city:
                query_parts.append(city)
            if region:
                query_parts.append(region)
            if country:
                query_parts.append(country)
            
            query = ", ".join(query_parts)
            
            # Use asyncio.to_thread to make the geocoding call non-blocking
            location = await asyncio.to_thread(self.geocoder.geocode, query)
            
            if location:
                coordinates = (location.latitude, location.longitude)
                # Cache the result
                self._coordinate_cache[cache_key] = coordinates
                logger.info(f"Geocoded '{query}' -> {coordinates}")
                return coordinates
            else:
                logger.warning(f"Could not geocode location: {query}")
                return None
                
        except Exception as e:
            logger.error(f"Error geocoding '{city}, {region}, {country}': {str(e)}")
            return None
    
    @lru_cache(maxsize=1000)
    def get_coordinates_sync(self, city: str, region: str = None, country: str = None) -> Optional[Tuple[float, float]]:
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
    
    async def calculate_distance(self, user_coords: Tuple[float, float], hostnode: Dict[str, Any]) -> Optional[float]:
        """Calculate distance between user and hostnode in kilometers"""
        
        try:
            # Extract location info from hostnode
            city = hostnode.get('city')
            region = hostnode.get('region')
            country = hostnode.get('country')
            
            if not city or not country:
                logger.warning(f"Incomplete location data for hostnode {hostnode.get('id')}: city={city}, country={country}")
                return None
            
            # Get coordinates for the hostnode location
            hostnode_coords = await self.get_coordinates(city, region, country)
            if not hostnode_coords:
                return None
            
            # Calculate distance using geodesic
            distance = geodesic(user_coords, hostnode_coords).kilometers
            return distance
            
        except Exception as e:
            logger.error(f"Error calculating distance to hostnode: {str(e)}")
            return None
    
    def calculate_distance_sync(self, user_coords: Tuple[float, float], hostnode: Dict[str, Any]) -> Optional[float]:
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
    
    async def find_closest_hostnode(
        self, 
        user_coords: Tuple[float, float], 
        hostnodes: list,
        min_specs: Dict[str, Any] = None
    ) -> Optional[Dict[str, Any]]:
        """Find the closest hostnode to the user that meets minimum specifications"""
        
        try:
            suitable_hostnodes = []
            
            for hostnode in hostnodes:
                # Check if hostnode meets minimum specs
                if min_specs and not self._meets_specifications(hostnode, min_specs):
                    continue
                
                # Calculate distance
                distance = self.calculate_distance_sync(user_coords, hostnode)
                if distance is not None:
                    suitable_hostnodes.append({
                        'hostnode': hostnode,
                        'distance_km': distance,
                        'location_str': f"{hostnode.get('city')}, {hostnode.get('region', '')}, {hostnode.get('country')}"
                    })
            
            if not suitable_hostnodes:
                logger.warning("No suitable hostnodes found")
                return None
            
            # Sort by distance and return the closest
            suitable_hostnodes.sort(key=lambda x: x['distance_km'])
            closest = suitable_hostnodes[0]
            
            logger.info(f"Closest hostnode: {closest['location_str']} ({closest['distance_km']:.1f}km away)")
            
            return {
                'hostnode': closest['hostnode'],
                'distance_km': closest['distance_km'],
                'location': closest['location_str'],
                'alternatives': suitable_hostnodes[1:6]  # Return top 5 alternatives
            }
            
        except Exception as e:
            logger.error(f"Error finding closest hostnode: {str(e)}")
            return None
    
    def _meets_specifications(self, hostnode: Dict[str, Any], min_specs: Dict[str, Any]) -> bool:
        """Check if hostnode meets minimum specifications"""
        
        try:
            specs = hostnode.get('specs', {})
            
            # Check CPU
            min_cpu = min_specs.get('min_vcpu', 0)
            if specs.get('cpu', 0) < min_cpu:
                return False
            
            # Check RAM (convert GB to MB if needed)
            min_ram = min_specs.get('min_ram', 0)  # Assume in MB
            hostnode_ram = specs.get('ram', 0)
            if hostnode_ram < min_ram:
                return False
            
            # Check GPU count
            min_gpu_count = min_specs.get('min_gpu_count', 0)
            hostnode_gpu_count = len(specs.get('gpu', []))
            if hostnode_gpu_count < min_gpu_count:
                return False
            
            # Check storage
            min_storage = min_specs.get('min_storage', 0)  # Assume in GB
            hostnode_storage = specs.get('storage', 0)
            if hostnode_storage < min_storage:
                return False
            
            return True
            
        except Exception as e:
            logger.error(f"Error checking specifications: {str(e)}")
            return False
    
    async def get_location_summary(self, hostnodes: list) -> Dict[str, Any]:
        """Get a summary of available locations from hostnodes"""
        
        try:
            locations = {}
            
            for hostnode in hostnodes:
                country = hostnode.get('country', 'Unknown')
                city = hostnode.get('city', 'Unknown')
                region = hostnode.get('region', '')
                
                if country not in locations:
                    locations[country] = {}
                
                location_key = f"{city}, {region}".strip(', ')
                if location_key not in locations[country]:
                    locations[country][location_key] = {
                        'count': 0,
                        'specs': []
                    }
                
                locations[country][location_key]['count'] += 1
                
                # Add specs summary
                specs = hostnode.get('specs', {})
                spec_summary = {
                    'cpu': specs.get('cpu', 0),
                    'ram': specs.get('ram', 0),
                    'gpu_count': len(specs.get('gpu', [])),
                    'storage': specs.get('storage', 0)
                }
                locations[country][location_key]['specs'].append(spec_summary)
            
            return {
                'total_countries': len(locations),
                'total_locations': sum(len(cities) for cities in locations.values()),
                'locations': locations
            }
            
        except Exception as e:
            logger.error(f"Error creating location summary: {str(e)}")
            return {'error': str(e)}