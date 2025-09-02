from fastapi import APIRouter, HTTPException
from typing import List
from app.models.gcp_region import GCPRegionDocument, GCPRegionRequest, GCPRegionResponse
from app.services.gcp_region_db_service import GCPRegionDatabaseService

router = APIRouter()

@router.get("/", response_model=List[GCPRegionResponse])
async def list_gcp_regions():
    """Get all GCP regions"""
    try:
        from app.core.sync_database import get_sync_database
        db = get_sync_database()
        regions_data = list(db.gcp_regions.find({"is_active": True}))
        regions = [GCPRegionDocument(**region_data) for region_data in regions_data]
        return [GCPRegionResponse(**region.dict()) for region in regions]
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

@router.get("/{region_code}", response_model=GCPRegionResponse)
async def get_gcp_region(region_code: str):
    """Get specific GCP region by code"""
    region_info = GCPRegionDatabaseService.get_region_info(region_code)
    if not region_info:
        raise HTTPException(status_code=404, detail=f"GCP region not found: {region_code}")
    
    try:
        from app.core.sync_database import get_sync_database
        db = get_sync_database()
        region_data = db.gcp_regions.find_one({"region_code": region_code})
        region = GCPRegionDocument(**region_data)
        return GCPRegionResponse(**region.dict())
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))

@router.post("/", response_model=GCPRegionResponse)
async def create_gcp_region(region_request: GCPRegionRequest):
    """Create or update GCP region"""
    try:
        region = GCPRegionDatabaseService.create_or_update_region(region_request.dict())
        return GCPRegionResponse(**region.dict())
    except Exception as e:
        raise HTTPException(status_code=400, detail=str(e))

@router.post("/seed-defaults")
async def seed_default_gcp_regions():
    """Initialize default GCP region coordinates"""
    
    # NOTE: These coordinates are approximate locations based on major cities near GCP data centers
    # Sources: Google Cloud documentation, public data center locations, and geographic approximations
    default_regions = [
        # US regions
        {"region_code": "us-central1", "display_name": "Iowa", "country": "US", "continent": "North America", "latitude": 39.0458, "longitude": -95.9980},
        {"region_code": "us-east1", "display_name": "South Carolina", "country": "US", "continent": "North America", "latitude": 33.1960, "longitude": -80.0131},
        {"region_code": "us-east4", "display_name": "Northern Virginia", "country": "US", "continent": "North America", "latitude": 39.0458, "longitude": -76.6413},
        {"region_code": "us-east5", "display_name": "Columbus, Ohio", "country": "US", "continent": "North America", "latitude": 39.1612, "longitude": -75.5264},
        {"region_code": "us-south1", "display_name": "Dallas, Texas", "country": "US", "continent": "North America", "latitude": 32.7767, "longitude": -96.7970},
        {"region_code": "us-west1", "display_name": "Oregon", "country": "US", "continent": "North America", "latitude": 45.5152, "longitude": -122.6784},
        {"region_code": "us-west2", "display_name": "Los Angeles", "country": "US", "continent": "North America", "latitude": 34.0522, "longitude": -118.2437},
        {"region_code": "us-west3", "display_name": "Salt Lake City", "country": "US", "continent": "North America", "latitude": 40.7589, "longitude": -111.8883},
        {"region_code": "us-west4", "display_name": "Las Vegas", "country": "US", "continent": "North America", "latitude": 36.1627, "longitude": -115.1200},
        
        # Canada
        {"region_code": "northamerica-northeast1", "display_name": "Montreal", "country": "Canada", "continent": "North America", "latitude": 45.5017, "longitude": -73.5673},
        {"region_code": "northamerica-northeast2", "display_name": "Toronto", "country": "Canada", "continent": "North America", "latitude": 43.6532, "longitude": -79.3832},
        
        # Europe
        {"region_code": "europe-west1", "display_name": "Belgium", "country": "Belgium", "continent": "Europe", "latitude": 50.8476, "longitude": 4.3572},
        {"region_code": "europe-west2", "display_name": "London", "country": "UK", "continent": "Europe", "latitude": 51.5074, "longitude": -0.1278},
        {"region_code": "europe-west3", "display_name": "Frankfurt", "country": "Germany", "continent": "Europe", "latitude": 50.1109, "longitude": 8.6821},
        {"region_code": "europe-west4", "display_name": "Netherlands", "country": "Netherlands", "continent": "Europe", "latitude": 53.3498, "longitude": -6.2603},
        {"region_code": "europe-west6", "display_name": "Zurich", "country": "Switzerland", "continent": "Europe", "latitude": 47.3769, "longitude": 8.5417},
        {"region_code": "europe-west8", "display_name": "Milan", "country": "Italy", "continent": "Europe", "latitude": 45.4642, "longitude": 9.1900},
        {"region_code": "europe-west9", "display_name": "Paris", "country": "France", "continent": "Europe", "latitude": 48.8566, "longitude": 2.3522},
        {"region_code": "europe-central2", "display_name": "Warsaw", "country": "Poland", "continent": "Europe", "latitude": 52.2297, "longitude": 21.0122},
        {"region_code": "europe-north1", "display_name": "Hamina", "country": "Finland", "continent": "Europe", "latitude": 60.5693, "longitude": 27.1878},
        {"region_code": "europe-southwest1", "display_name": "Madrid", "country": "Spain", "continent": "Europe", "latitude": 40.4168, "longitude": -3.7038},
        
        # Asia Pacific
        {"region_code": "asia-east1", "display_name": "Taiwan", "country": "Taiwan", "continent": "Asia", "latitude": 24.0518, "longitude": 120.5162},
        {"region_code": "asia-east2", "display_name": "Hong Kong", "country": "Hong Kong", "continent": "Asia", "latitude": 22.3193, "longitude": 114.1694},
        {"region_code": "asia-northeast1", "display_name": "Tokyo", "country": "Japan", "continent": "Asia", "latitude": 35.6762, "longitude": 139.6503},
        {"region_code": "asia-northeast2", "display_name": "Osaka", "country": "Japan", "continent": "Asia", "latitude": 34.6937, "longitude": 135.5023},
        {"region_code": "asia-northeast3", "display_name": "Seoul", "country": "South Korea", "continent": "Asia", "latitude": 37.5665, "longitude": 126.9780},
        {"region_code": "asia-south1", "display_name": "Mumbai", "country": "India", "continent": "Asia", "latitude": 19.0760, "longitude": 72.8777},
        {"region_code": "asia-south2", "display_name": "Delhi", "country": "India", "continent": "Asia", "latitude": 28.7041, "longitude": 77.1025},
        {"region_code": "asia-southeast1", "display_name": "Singapore", "country": "Singapore", "continent": "Asia", "latitude": 1.3521, "longitude": 103.8198},
        {"region_code": "asia-southeast2", "display_name": "Jakarta", "country": "Indonesia", "continent": "Asia", "latitude": -6.2088, "longitude": 106.8456},
        
        # Australia
        {"region_code": "australia-southeast1", "display_name": "Sydney", "country": "Australia", "continent": "Australia", "latitude": -33.8688, "longitude": 151.2093},
        {"region_code": "australia-southeast2", "display_name": "Melbourne", "country": "Australia", "continent": "Australia", "latitude": -37.8136, "longitude": 144.9631},
        
        # South America
        {"region_code": "southamerica-east1", "display_name": "São Paulo", "country": "Brazil", "continent": "South America", "latitude": -23.5505, "longitude": -46.6333},
        {"region_code": "southamerica-west1", "display_name": "Santiago", "country": "Chile", "continent": "South America", "latitude": -33.4489, "longitude": -70.6693},
        
        # Middle East
        {"region_code": "me-west1", "display_name": "Tel Aviv", "country": "Israel", "continent": "Middle East", "latitude": 31.0461, "longitude": 34.8516},
        {"region_code": "me-central1", "display_name": "Dammam", "country": "Saudi Arabia", "continent": "Middle East", "latitude": 26.0667, "longitude": 50.5577},
        
        # Africa
        {"region_code": "africa-south1", "display_name": "Johannesburg", "country": "South Africa", "continent": "Africa", "latitude": -26.2041, "longitude": 28.0473}
    ]
    
    created_count = 0
    for region_data in default_regions:
        try:
            GCPRegionDatabaseService.create_or_update_region(region_data)
            created_count += 1
        except Exception as e:
            print(f"Error creating region {region_data['region_code']}: {str(e)}")
            continue
    
    return {"message": f"Seeded {created_count} GCP region coordinates"}

@router.delete("/{region_code}")
async def delete_gcp_region(region_code: str):
    """Delete GCP region"""
    try:
        from app.core.sync_database import get_sync_database
        db = get_sync_database()
        result = db.gcp_regions.delete_one({"region_code": region_code})
        if result.deleted_count == 0:
            raise HTTPException(status_code=404, detail=f"GCP region not found: {region_code}")
        return {"message": f"GCP region {region_code} deleted"}
    except Exception as e:
        raise HTTPException(status_code=500, detail=str(e))