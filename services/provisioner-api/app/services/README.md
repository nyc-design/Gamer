# Business Logic Services

## Overview
The services directory contains the core business logic for the Provisioner API. These services handle complex operations like VM orchestration, location optimization, billing calculations, and cloud provider integrations. Each service is designed to be modular, testable, and reusable across different API endpoints.

## Architecture
- **Service Layer Pattern**: Business logic separated from API controllers
- **Async Operations**: All services use async/await for non-blocking I/O
- **Provider Abstraction**: Unified interfaces for different cloud providers
- **Error Handling**: Comprehensive exception handling with logging
- **Dependency Injection**: Services inject other services as needed

## Service Dependencies
```
VMOrchestrator
├── TensorDockService
├── CloudyPadService  
├── RegionService (deprecated)
└── GeocodingService

CloudyPadService
└── GCPRegionService

TensorDockService
└── GeocodingService

BillingService
├── TensorDockService
└── GCPRegionService (for cost calculation)
```

## Files

### vm_orchestrator.py
Central orchestration service that coordinates VM provisioning across multiple providers.

#### VMOrchestrator Class

#### __init__(self)
Initialize orchestrator with service dependencies.

**Dependencies Initialized**:
- `TensorDockService()`: Direct TensorDock API operations
- `CloudyPadService()`: CloudyPad CLI operations
- `RegionService()`: Legacy region mapping (deprecated)

#### provision_and_launch_game(vm_id: str, game_id: str, save_id: Optional[str] = None) -> None
Complete workflow for VM provisioning and game launch.

**Parameters**:
- `vm_id`: Unique VM identifier
- `game_id`: Target game identifier
- `save_id`: Optional save file identifier

**Process**:
1. **VM Lookup**: Find VM document in database
2. **Provider Selection**: Route to appropriate provider based on VM.provider
3. **VM Provisioning**: Create VM infrastructure
4. **Environment Setup**: Configure gaming environment (CloudyPad, Wolf)
5. **Game Launch**: Initiate game launch through Agent API
6. **Status Tracking**: Update VM status throughout process

**Error Handling**: 
- VM status set to ERROR on any failure
- Detailed logging for debugging
- Graceful degradation where possible

**Side Effects**: Updates VM document status and metadata

#### _provision_tensordock_vm(vm: VMDocument) -> bool
Provision VM using TensorDock with location optimization.

**Parameters**: `vm` - VM document with configuration
**Returns**: `bool` - Success/failure status

**Process**:
1. **Location Processing**: Extract user location from VM document
2. **Optimal Selection**: Find closest hostnode using coordinates
3. **VM Creation**: Call TensorDock API with optimal hostnode
4. **Metadata Update**: Store provider instance ID, IP address, metadata
5. **Readiness Wait**: Wait for VM to be SSH-accessible

**Location Logic**:
```python
if user_location:
    lat = vm.user_location.get('latitude')
    lon = vm.user_location.get('longitude') 
    user_coords = (lat, lon)
    # TensorDock service handles hostnode distance calculation
```

#### _provision_cloudypad_vm(vm: VMDocument) -> bool
Provision VM using CloudyPad with GCP region optimization.

**Parameters**: `vm` - VM document with configuration
**Returns**: `bool` - Success/failure status

**Process**:
1. **Location Processing**: Extract user coordinates
2. **GCP Region Selection**: Use Google Cloud Location Finder API
3. **CloudyPad Provisioning**: Call CloudyPad service with optimal region
4. **Status Tracking**: Monitor provisioning through CloudyPad CLI

#### _setup_gaming_environment(vm: VMDocument) -> bool
Configure gaming environment on provisioned VM.

**Provider-Specific Logic**:
- **TensorDock VMs**: Deploy CloudyPad via SSH provider
- **CloudyPad VMs**: Environment already configured

**Process for TensorDock**:
1. Call `_setup_cloudypad_via_ssh()`
2. Configure Wolf streaming server
3. Mount game library
4. Set up save file synchronization

#### _setup_cloudypad_via_ssh(vm: VMDocument) -> bool
Deploy CloudyPad gaming environment to TensorDock VM via SSH.

**Parameters**: `vm` - TensorDock VM document
**Returns**: `bool` - Success/failure status

**Process** (TODO - Implementation pending):
1. SSH connection to TensorDock VM
2. Deploy CloudyPad container stack
3. Configure Wolf streaming server  
4. Mount GCS game library
5. Set up save file sync
6. Configure auto-stop monitoring

**Current Implementation**: Simulated with 30-second delay

#### _wait_for_vm_ready(vm: VMDocument, max_wait_minutes: int = 10) -> bool
Wait for VM to be ready for SSH connections.

**Parameters**:
- `vm`: VM document with provider details
- `max_wait_minutes`: Maximum wait time (default 10)

**Process**:
1. Check VM status every 10 seconds
2. For TensorDock: Query API for "running" status
3. Timeout after max_wait_minutes
4. Return readiness status

#### launch_game_on_vm(vm_id: str, game_id: str, save_id: Optional[str] = None) -> None
Launch specific game on running VM.

**Parameters**:
- `vm_id`: Target VM identifier
- `game_id`: Game to launch
- `save_id`: Optional save file

**Process** (TODO - Agent API integration):
1. Validate VM is ready for game launch
2. Call Agent API with game launch request
3. Monitor launch success
4. Update activity tracking

**Current Implementation**: Logs launch initiation, updates activity timestamp

#### stop_vm(vm_id: str) -> bool
Stop running VM using appropriate provider.

**Process**:
1. Lookup VM document
2. Route to provider-specific stop method
3. Update VM status to STOPPED on success

#### terminate_vm(vm_id: str) -> bool
Terminate VM using appropriate provider.

**Process**:
1. Lookup VM document
2. Route to provider-specific terminate method
3. Update VM status to TERMINATED on success

### tensordock_service.py
TensorDock marketplace API integration with location-based hostnode selection.

#### TensorDockService Class

#### __init__(self)
Initialize TensorDock service with SDK and geocoding capabilities.

**Configuration**:
- `tensordock_api_key`: Primary API authentication
- `tensordock_api_token`: Secondary token (often same as key)
- `TensorDockAPI client`: Official Python SDK instance
- `GeocodingService`: For city/country to GPS conversion

#### create_vm(vm_id: str, console_type: ConsoleType, preset: VMPreset, user_location: Optional[Tuple[float, float]] = None) -> Dict[str, Any]
Create VM with optimal hostnode selection based on user location.

**Parameters**:
- `vm_id`: Unique VM identifier
- `console_type`: Target console for optimization
- `preset`: Resource configuration template
- `user_location`: GPS coordinates tuple (lat, lon)

**Process**:
1. **Spec Calculation**: Generate VM specifications from preset/console
2. **Hostnode Query**: Get available hostnodes with minimum GPU requirements
3. **Location Optimization**: Select optimal hostnode based on user location
4. **VM Deployment**: Deploy VM using TensorDock SDK

**Location Selection Logic**:
```python
if user_location:
    result = await geocoding_service.find_closest_hostnode(
        user_location, 
        available_hostnodes, 
        min_spec_requirements
    )
    selected_hostnode = result['hostnode']
```

**Returns**:
```python
{
    "success": bool,
    "instance_id": str,           # TensorDock server ID
    "ip_address": str,            # Public IP
    "ssh_port": 22,
    "status": "creating",
    "hostnode_location": str,     # "Dallas, Texas, US"  
    "distance_km": float,         # Distance to user
    "metadata": dict              # Full TensorDock response
}
```

#### _select_optimal_hostnode(hostnodes: List[Dict], user_location: Optional[Tuple], min_specs: Dict) -> Optional[Dict]
Select optimal hostnode based on location and specifications.

**Parameters**:
- `hostnodes`: Available TensorDock hostnodes
- `user_location`: User GPS coordinates
- `min_specs`: Minimum hardware requirements

**Logic**:
1. **Location-Based**: If user_location provided, use distance calculation
2. **Spec-Based**: If no location, use first hostnode meeting requirements
3. **Geocoding**: Convert hostnode city/country to coordinates
4. **Distance**: Calculate geodesic distance to user

#### _hostnode_meets_specs(hostnode: Dict, min_specs: Dict) -> bool
Validate hostnode meets minimum hardware requirements.

**Validation Checks**:
- **CPU**: `hostnode.specs.cpu >= min_specs.cpu`
- **RAM**: `hostnode.specs.ram >= min_specs.memory * 1024` (GB to MB)
- **GPU Count**: `len(hostnode.specs.gpu) >= min_specs.gpu_count`
- **Storage**: `hostnode.specs.storage >= min_specs.storage`

#### _get_vm_specs(preset: VMPreset, console_type: ConsoleType) -> Dict[str, Any]
Generate VM specifications from preset and console requirements.

**Base Specifications**:
```python
base_specs = {
    VMPreset.RETRO: {"cpu": 2, "memory": 4, "storage": 50, "gpu_count": 0},
    VMPreset.ADVANCED: {"cpu": 4, "memory": 8, "storage": 100, "gpu_count": 1},  
    VMPreset.PREMIUM: {"cpu": 8, "memory": 16, "storage": 200, "gpu_count": 1}
}
```

**Console Adjustments**:
```python
console_adjustments = {
    ConsoleType.SWITCH: {"cpu": 8, "memory": 16, "gpu_count": 1, "gpu_model": "RTX4090"},
    ConsoleType.N3DS: {"cpu": 4, "memory": 8, "gpu_count": 1, "gpu_model": "GTX1060"}
}
```

#### get_vm_status(instance_id: str) -> Optional[Dict[str, Any]]
Get VM status using TensorDock SDK.

**Process**:
1. Call `client.virtual_machines.get_vm_details(instance_id)`
2. Map TensorDock status to internal status codes
3. Return standardized status information

**Status Mapping**:
- `"active"` → `"running"`
- `"building"` → `"creating"`
- `"stopped"` → `"stopped"`
- `"error"` → `"error"`
- `"deleted"` → `"terminated"`

#### start_vm(instance_id: str) -> bool
Start stopped VM using TensorDock SDK.

#### stop_vm(instance_id: str) -> bool  
Stop running VM using TensorDock SDK.

#### terminate_vm(instance_id: str) -> bool
Terminate VM using TensorDock SDK.

#### list_available_hostnodes(min_gpu_count: int = 0) -> List[Dict[str, Any]]
Get list of available hostnodes from TensorDock.

**Parameters**: `min_gpu_count` - Minimum GPU requirement filter
**Returns**: List of hostnodes with specifications and location data

#### get_available_locations() -> Dict[str, Any]
Get geographic summary of all TensorDock locations.

**Process**:
1. Fetch all available hostnodes
2. Generate location summary using geocoding service
3. Group by country and city with hostnode counts

### cloudypad_service.py
CloudyPad CLI integration with GCP region optimization.

#### CloudyPadService Class

#### __init__(self)
Initialize CloudyPad service with CLI interface and GCP region optimization.

**Dependencies**:
- `_active_vms`: Dict tracking active VM states
- `GCPRegionService`: For optimal GCP region selection

#### provision_vm(vm_id: str, vm_request: VMCreateRequest, user_location: Optional[Tuple[float, float]] = None) -> bool
Provision VM using CloudyPad CLI with GCP region optimization.

**Parameters**:
- `vm_id`: Unique VM identifier
- `vm_request`: VM configuration request
- `user_location`: GPS coordinates for region selection

**Process**:
1. **Spec Generation**: Create VM specs from preset
2. **Region Selection**: Find optimal GCP region using Location Finder API
3. **Command Building**: Construct CloudyPad CLI command with region
4. **Execution**: Run CloudyPad CLI asynchronously
5. **Result Parsing**: Parse CLI output for VM details

**GCP Region Selection**:
```python
if user_location:
    region_info = await gcp_region_service.get_closest_region_via_api(user_location)
    gcp_region = region_info.get('region_code')
    # Region passed to CloudyPad via --region parameter
```

#### _build_create_command(vm_id: str, vm_request: VMCreateRequest, vm_specs: Dict, gcp_region: Optional[str] = None) -> list
Build CloudyPad CLI command with configuration parameters.

**Command Structure**:
```python
cmd = [
    "cloudypad", "create",
    "--name", vm_id,
    "--cpu", str(vm_specs["cpu"]),  
    "--memory", f"{vm_specs['memory']}GB",
    "--autostop-enable",
    "--autostop-timeout", str(timeout_minutes)
]

if gcp_region:
    cmd.extend(["--region", gcp_region])
```

#### get_vm_status(vm_id: str) -> Optional[VMStatusResponse]
Get VM status using CloudyPad CLI (Implementation pending).

#### stop_vm(vm_id: str) -> bool
Stop VM using CloudyPad CLI.

#### start_vm(vm_id: str) -> bool  
Start VM using CloudyPad CLI.

#### terminate_vm(vm_id: str) -> bool
Terminate VM using CloudyPad CLI.

### gcp_region_service.py
Google Cloud region optimization using Location Finder API and distance calculation.

#### GCPRegionService Class

#### __init__(self)
Initialize GCP region service with API configuration.

**Configuration**:
- `gcp_project_id`: Google Cloud project for Location Finder API
- `location_finder_url`: Google Cloud Location Finder API endpoint
- `gcp_regions`: Static mapping of GCP regions with coordinates

**Region Coverage**: 30+ GCP regions across 6 continents with precise data center coordinates.

#### get_closest_region_via_api(user_location: Tuple[float, float]) -> Optional[Dict[str, Any]]
Get closest GCP region using Google Cloud Location Finder API with fallback.

**Parameters**: `user_location` - GPS coordinates tuple
**Returns**: Region information with distance and method used

**Process**:
1. **API Attempt**: Call Google Cloud Location Finder API
2. **Request Format**:
   ```python
   url = f"{api_url}/projects/{project}/locations/global/cloudLocations:search"
   params = {
       "filter": "cloudProvider=GOOGLE_CLOUD",
       "orderBy": f"proximity({lat},{lon})"
   }
   ```
3. **Fallback**: If API fails, use distance calculation
4. **Response Format**:
   ```python
   {
       "region_code": "us-south1",
       "region_name": "Dallas, Texas, US", 
       "distance_km": 15.2,
       "source": "google_cloud_location_finder_api"
   }
   ```

#### get_closest_region(user_location: Tuple[float, float]) -> Optional[Dict[str, Any]]
Get closest GCP region using distance calculation (fallback method).

**Process**:
1. Calculate geodesic distance to all GCP regions
2. Sort by distance
3. Return closest region with distance

#### get_top_regions(user_location: Tuple[float, float], limit: int = 5) -> list
Get top N closest GCP regions with distance calculations.

#### get_all_regions() -> Dict[str, Any]
Get all GCP regions organized by continent.

**Response Structure**:
```python
{
    "total_regions": 30,
    "regions_by_continent": {
        "North America": [region_list],
        "Europe": [region_list],
        "Asia": [region_list]
    }
}
```

### geocoding_service.py  
Geographic coordinate conversion and distance calculation service.

#### GeocodingService Class

#### __init__(self)
Initialize geocoding service with Nominatim geocoder and caching.

**Configuration**:
- `Nominatim`: OpenStreetMap geocoding service
- `_coordinate_cache`: In-memory cache for repeated lookups
- `@lru_cache`: Function-level caching for performance

#### get_coordinates(city: str, region: str = None, country: str = None) -> Optional[Tuple[float, float]]
Convert city/region/country to GPS coordinates (async version).

**Parameters**:
- `city`: City name (e.g., "Dallas")
- `region`: State/region (e.g., "Texas") 
- `country`: Country name (e.g., "US")

**Process**:
1. **Cache Check**: Check coordinate cache first
2. **Query Build**: Construct geocoding query string
3. **API Call**: Call Nominatim geocoding service
4. **Caching**: Store result in cache for future use
5. **Return**: GPS coordinates tuple or None

#### calculate_distance(user_coords: Tuple[float, float], hostnode: Dict[str, Any]) -> Optional[float]
Calculate distance between user and hostnode in kilometers.

**Process**:
1. **Location Extraction**: Get city/country from hostnode
2. **Geocoding**: Convert hostnode location to coordinates  
3. **Distance Calculation**: Use geodesic distance formula
4. **Return**: Distance in kilometers

#### find_closest_hostnode(user_coords: Tuple[float, float], hostnodes: list, min_specs: Dict = None) -> Optional[Dict[str, Any]]
Find closest hostnode meeting specifications.

**Parameters**:
- `user_coords`: User GPS coordinates
- `hostnodes`: List of available hostnodes
- `min_specs`: Minimum hardware requirements

**Process**:
1. **Spec Filtering**: Filter hostnodes by minimum requirements
2. **Distance Calculation**: Calculate distance to each hostnode
3. **Sorting**: Sort by distance (closest first)
4. **Result**: Return closest hostnode with alternatives

**Return Format**:
```python
{
    'hostnode': hostnode_data,
    'distance_km': 15.2,
    'location': "Dallas, Texas, US",
    'alternatives': [top_5_alternatives]
}
```

### billing_service.py
Comprehensive billing and usage monitoring across all cloud providers.

#### BillingService Class

#### __init__(self)
Initialize billing service with provider integrations.

**Dependencies**:
- `TensorDockService`: For TensorDock cost calculations
- `gcp_project_id`, `gcp_billing_account`: GCP billing configuration

#### get_tensordock_usage(start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]
Get TensorDock usage and cost analysis.

**Parameters**:
- `start_date`: Analysis start date (default: 30 days ago)
- `end_date`: Analysis end date (default: now)

**Process**:
1. **VM Query**: Find all TensorDock VMs in date range
2. **Runtime Calculation**: Calculate hours between created_at and last_activity
3. **Cost Estimation**: Apply cost estimation algorithm per VM
4. **Aggregation**: Sum totals and generate detailed breakdown

**Cost Estimation Logic**:
```python
base_rates = {
    "retro": 0.15,      # $0.15/hour
    "advanced": 0.35,   # $0.35/hour  
    "premium": 1.20     # $1.20/hour
}

console_multipliers = {
    "switch": 1.3,      # Higher GPU requirements
    "3ds": 1.1,         # Moderate GPU requirements
}

final_cost = hours * base_rate * console_multiplier
```

#### get_gcp_usage(start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]
Get GCP CloudyPad usage and cost analysis.

**Similar process to TensorDock with GCP-specific cost rates and CloudyPad overhead multiplier (1.25x).**

#### get_combined_usage_report(start_date: datetime = None, end_date: datetime = None) -> Dict[str, Any]
Generate unified usage report across all providers.

**Process**:
1. **Parallel Queries**: Get usage from all providers simultaneously
2. **Error Handling**: Handle individual provider failures gracefully  
3. **Aggregation**: Combine totals across providers
4. **Breakdown**: Provide per-provider detailed breakdown

#### check_billing_alerts(daily_limit: float = 50.0, monthly_limit: float = 500.0) -> Dict[str, Any]
Monitor spending against configurable limits with alerting.

**Alert Types**:
- **Warning**: 80% of limit reached
- **Critical**: 100% of limit exceeded  

**Process**:
1. **Current Usage**: Calculate today and month-to-date spending
2. **Threshold Comparison**: Compare against warning/critical thresholds
3. **Alert Generation**: Create structured alerts with severity levels

### region_service.py (Deprecated)
Legacy region mapping service replaced by provider-specific location optimization.

**Status**: Maintained for backward compatibility but not actively used.

## Service Integration Patterns

### Async Service Communication
```python
# Service calls other services
result = await other_service.method(parameters)

# Error handling in service calls  
try:
    result = await external_service.call()
except ExternalServiceError:
    logger.warning("External service failed, using fallback")
    result = fallback_method()
```

### Provider Abstraction
```python
# Unified interface across providers
if vm.provider == CloudProvider.TENSORDOCK:
    result = await tensordock_service.create_vm(...)
elif vm.provider == CloudProvider.CLOUDYPAD_GCP:
    result = await cloudypad_service.provision_vm(...)
```

### Location Optimization
```python
# Location-aware service selection
if user_location:
    if provider == "tensordock":
        # Find closest hostnode by distance
        optimal = find_closest_hostnode(user_location, hostnodes)
    elif provider == "cloudypad":  
        # Find optimal GCP region via Location Finder API
        optimal = await gcp_region_service.get_closest_region_via_api(user_location)
```

### Error Handling and Fallbacks
```python
# Graceful degradation pattern
try:
    result = await primary_method()
except PrimaryServiceError:
    logger.warning("Primary service failed, using fallback")
    result = await fallback_method()
except Exception as e:
    logger.error(f"All methods failed: {e}")
    raise ServiceUnavailableError()
```

## Performance Considerations

### Caching Strategies
- **Geocoding Results**: LRU cache for city → coordinates conversion
- **Provider Data**: Cache hostnode and region data with TTL
- **Cost Calculations**: Cache rate tables and multipliers

### Connection Pooling
- **HTTP Clients**: Reuse httpx.AsyncClient instances
- **Database**: MongoDB connection pool via Motor
- **External APIs**: Connection pooling for provider APIs

### Background Processing
- **Long Operations**: VM provisioning, gaming environment setup
- **Monitoring**: Usage calculations, billing analysis
- **Cleanup**: Terminated VM cleanup, expired session handling