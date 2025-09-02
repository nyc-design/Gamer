# API Routers and Endpoints

## Overview
The routers directory contains FastAPI router modules that define REST API endpoints for the Provisioner API. Each router handles a specific domain of functionality with comprehensive validation, error handling, and response formatting.

## Architecture
- **FastAPI Routers**: Modular endpoint organization
- **Async Operations**: Non-blocking request handling
- **Background Tasks**: Long-running operations handled asynchronously  
- **Validation**: Automatic request/response validation with Pydantic
- **Error Handling**: Consistent HTTP error responses

## Router Organization
- **vms.py**: Direct VM lifecycle operations
- **config.py**: Emulator configuration management
- **launch.py**: Intelligent game launching with location optimization
- **billing.py**: Usage tracking and cost monitoring

## Files

### vms.py
Direct VM management operations with basic CRUD functionality.

#### Endpoints

#### POST /vms
Create a new VM with specified configuration.

**Request Body**: `VMCreateRequest`
```json
{
  "console_type": "switch",
  "provider": "tensordock",
  "auto_stop_timeout": 900,
  "user_id": "user123",
  "user_location": {"latitude": 32.7767, "longitude": -96.7970}
}
```

**Response**: `VMResponse` - VM details with initial status `CREATING`
**Background Task**: VM provisioning initiated automatically
**Error Codes**:
- `400`: Invalid request data
- `409`: VM ID conflict
- `503`: No providers available

#### GET /vms
List VMs with optional filtering.

**Query Parameters**:
- `user_id`: Filter by user
- `status`: Filter by VM status
- `console_type`: Filter by console
- `limit`: Maximum results (default 50)
- `skip`: Pagination offset

**Response**: List of `VMResponse` objects
**Error Codes**:
- `400`: Invalid query parameters

#### GET /vms/{vm_id}
Get specific VM details.

**Path Parameters**: `vm_id` - Unique VM identifier
**Response**: `VMResponse` - Complete VM information
**Error Codes**:
- `404`: VM not found

#### PUT /vms/{vm_id}/start
Start a stopped VM.

**Path Parameters**: `vm_id` - VM identifier
**Response**: `{"message": "VM start initiated"}`
**Background Task**: VM startup through provider API
**Error Codes**:
- `404`: VM not found
- `400`: VM not in startable state

#### PUT /vms/{vm_id}/stop  
Stop a running VM.

**Path Parameters**: `vm_id` - VM identifier
**Response**: `{"message": "VM stop initiated"}`
**Background Task**: VM shutdown through provider API
**Error Codes**:
- `404`: VM not found  
- `400`: VM not in stoppable state

#### DELETE /vms/{vm_id}
Terminate and destroy VM.

**Path Parameters**: `vm_id` - VM identifier
**Response**: `{"message": "VM termination initiated"}`
**Background Task**: VM destruction through provider API
**Error Codes**:
- `404`: VM not found
- `400`: VM termination not allowed in current state

### config.py  
Emulator configuration and provider preference management.

#### Endpoints

#### GET /config/emulators
List all emulator configurations.

**Response**: List of emulator configurations with console types and provider preferences
**Use Case**: Admin dashboard and system configuration display

#### GET /config/emulators/{console_type}
Get configuration for specific console type.

**Path Parameters**: `console_type` - Console identifier (e.g., "switch", "3ds")
**Response**: Detailed configuration including provider preferences, hardware requirements
**Error Codes**:
- `404`: Console type not configured

#### PUT /config/emulators/{console_type}
Update emulator configuration.

**Path Parameters**: `console_type` - Console identifier  
**Request Body**: Configuration update with provider preferences, requirements
**Response**: Updated configuration
**Error Codes**:
- `400`: Invalid configuration data
- `404`: Console type not found

#### POST /config/emulators
Create new emulator configuration.

**Request Body**: Complete emulator configuration
**Response**: Created configuration
**Error Codes**:
- `400`: Invalid configuration data
- `409`: Console type already configured

#### GET /config/providers
List available cloud providers and their capabilities.

**Response**: Provider information including regions, instance types, pricing estimates

### launch.py
Intelligent game launching with automatic VM selection and location optimization.

#### Core Game Launch Endpoints

#### POST /launch/game
Launch a game with automatic VM provisioning and location optimization.

**Request Parameters**:
- `console_type`: Target console (query param)
- `game_id`: Game identifier (query param)
- `user_id`: User identifier (query param)
- `save_id`: Optional save file (query param)
- `user_latitude`: GPS latitude (query param)
- `user_longitude`: GPS longitude (query param)

**Process**:
1. Lookup emulator configuration for console type
2. Check for existing running VM for user/console combination
3. If found, launch game on existing VM
4. If not found, select optimal provider based on configuration and location
5. Create new VM with location optimization
6. Begin provisioning and game setup in background

**Response**: `VMResponse` - VM details (existing or newly created)
**Background Tasks**: 
- VM provisioning (if new)
- Gaming environment setup
- Game launch initiation

**Error Codes**:
- `404`: No configuration found for console type
- `503`: No available providers
- `400`: Invalid location coordinates

#### POST /launch/vm/{vm_id}/game
Launch specific game on existing VM.

**Path Parameters**: `vm_id` - Target VM identifier
**Request Parameters**:
- `game_id`: Game identifier
- `save_id`: Optional save file

**Response**: `{"message": "Game launch queued"}`
**Background Task**: Game launch coordination through Agent API
**Error Codes**:
- `404`: VM not found
- `400`: VM not in running state

#### VM Recommendation Endpoints

#### GET /launch/vm/optimal-for/{console_type}
Get optimal VM configuration recommendation for console type.

**Path Parameters**: `console_type` - Console identifier
**Query Parameters**: `user_id` - User identifier

**Response**:
```json
{
  "console_type": "switch",
  "recommended_provider": "tensordock", 
  "preset": "premium",
  "estimated_cost_per_hour": 1.50,
  "max_session_hours": 6,
  "requirements": {
    "min_cpu": 8,
    "min_ram_gb": 16, 
    "requires_gpu": true
  }
}
```

#### Location and Hostnode Endpoints

#### GET /launch/hostnodes/available
List available TensorDock hostnodes with optional filtering.

**Query Parameters**:
- `min_gpu_count`: Minimum GPU requirement
- `console_type`: Filter by console requirements

**Response**:
```json
{
  "total_hostnodes": 25,
  "hostnodes": [
    {
      "id": "hostnode123",
      "city": "Dallas", 
      "region": "Texas",
      "country": "US",
      "specs": {
        "cpu": 8,
        "ram": 32768,
        "gpu": [{"model": "RTX4090", "count": 1}],
        "storage": 500
      },
      "status": "online"
    }
  ]
}
```

#### GET /launch/hostnodes/closest
Find closest TensorDock hostnodes to user location.

**Query Parameters**:
- `user_latitude`: GPS latitude coordinate  
- `user_longitude`: GPS longitude coordinate
- `min_gpu_count`: Minimum GPU requirement
- `console_type`: Filter by console requirements
- `limit`: Maximum results (default 10)

**Process**:
1. Validate GPS coordinates
2. Get all available hostnodes matching requirements
3. Calculate distance to each hostnode using geocoding
4. Sort by distance and return closest matches

**Response**:
```json
{
  "user_location": {"latitude": 32.7767, "longitude": -96.7970},
  "total_found": 8,
  "closest_hostnodes": [
    {
      "hostnode": { /* hostnode details */ },
      "distance_km": 15.2,
      "location": "Dallas, Texas, US"
    }
  ]
}
```

#### GET /launch/locations/summary  
Get summary of all available TensorDock locations.

**Response**: Geographic breakdown of hostnodes by country and city with counts and specifications.

#### GCP Region Endpoints

#### GET /launch/gcp/regions/closest
Get closest GCP regions for CloudyPad using Google Cloud Location Finder API.

**Query Parameters**:
- `user_latitude`: GPS latitude
- `user_longitude`: GPS longitude  
- `use_location_finder`: Use Google API (default true)
- `limit`: Maximum results (default 5)

**Process**:
1. If `use_location_finder=true`: Call Google Cloud Location Finder API
2. If API fails or disabled: Fall back to distance calculation
3. Return closest regions with method used

**Response**:
```json
{
  "user_location": {"latitude": 32.7767, "longitude": -96.7970},
  "method": "google_cloud_location_finder_api",
  "closest_region": {
    "region_code": "us-south1",
    "region_name": "Dallas, Texas, US",
    "distance_km": 12.5,
    "source": "google_cloud_location_finder_api"
  },
  "alternatives": [ /* other close regions */ ]
}
```

#### GET /launch/gcp/regions/all
List all available GCP regions grouped by continent.

**Response**:
```json
{
  "total_regions": 30,
  "regions_by_continent": {
    "North America": [
      {
        "region_code": "us-central1",
        "region_name": "Iowa, US",
        "location": {"latitude": 39.0458, "longitude": -95.9980}
      }
    ]
  }
}
```

### billing.py
Usage tracking and cost monitoring across all cloud providers.

#### Usage Report Endpoints

#### GET /billing/usage/tensordock
Get TensorDock usage and billing information.

**Query Parameters**:
- `days`: Number of days to look back (1-365, default 30)

**Response**:
```json
{
  "provider": "tensordock",
  "period": {"start": "2024-01-01T00:00:00Z", "end": "2024-01-31T23:59:59Z"},
  "summary": {
    "total_vms": 15,
    "total_runtime_hours": 125.5,
    "total_estimated_cost_usd": 187.25
  },
  "vm_details": [
    {
      "vm_id": "vm-123",
      "console_type": "switch",
      "preset": "premium",
      "runtime_hours": 8.5,
      "estimated_cost_usd": 12.75
    }
  ]
}
```

#### GET /billing/usage/gcp
Get GCP usage for CloudyPad VMs.

**Query Parameters**: Same as TensorDock endpoint
**Response**: Similar structure with GCP-specific cost calculations

#### GET /billing/usage/combined  
Get combined usage report across all providers.

**Response**: Unified report with provider breakdown and overall totals

#### GET /billing/usage/custom
Get usage report for custom date range.

**Query Parameters**:
- `start_date`: Start date (ISO format)
- `end_date`: End date (ISO format)
- `provider`: Optional provider filter ("tensordock", "gcp", or null for all)

**Validation**:
- Date range cannot exceed 365 days
- Start date must be before end date

#### Cost Analysis Endpoints

#### GET /billing/costs/current-month
Get costs for the current month.

**Response**:
```json
{
  "month": "January 2024",
  "period": {"start": "2024-01-01T00:00:00Z", "end": "2024-01-31T15:30:00Z"},
  "costs": {
    "total_vms": 25,
    "total_runtime_hours": 200.5,
    "total_estimated_cost_usd": 425.75
  },
  "provider_breakdown": {
    "tensordock": { /* detailed breakdown */ },
    "gcp": { /* detailed breakdown */ }
  }
}
```

#### GET /billing/costs/daily
Get daily cost breakdown.

**Query Parameters**:
- `days`: Number of days (1-30, default 7)

**Response**:
```json
{
  "period_days": 7,
  "daily_breakdown": [
    {
      "date": "2024-01-31",
      "cost": 15.25,
      "vms": 3,
      "hours": 12.5
    }
  ],
  "total_cost": 95.75,
  "average_daily_cost": 13.68
}
```

#### Alert and Monitoring Endpoints

#### GET /billing/alerts
Check billing alerts and spending limits.

**Query Parameters**:
- `daily_limit`: Daily spending limit in USD (default 50.0)
- `monthly_limit`: Monthly spending limit in USD (default 500.0)

**Process**:
1. Calculate today's spending
2. Calculate current month's spending  
3. Compare against limits
4. Generate alerts for thresholds (80% warning, 100% critical)

**Response**:
```json
{
  "alerts": [
    {
      "type": "monthly_limit_warning",
      "message": "Monthly spending approaching limit: $425.75 (80% of $500.00)",
      "severity": "warning"
    }
  ],
  "current_usage": {
    "daily_cost": 15.25,
    "monthly_cost": 425.75,
    "daily_limit": 50.0,
    "monthly_limit": 500.0
  }
}
```

## Common Patterns

### Background Task Processing
```python
@router.post("/endpoint")
async def endpoint_handler(background_tasks: BackgroundTasks):
    # Immediate response
    background_tasks.add_task(long_running_operation, param1, param2)
    return {"message": "Operation initiated"}
```

### Error Handling
```python
try:
    result = await service_operation()
    return result
except NotFoundError:
    raise HTTPException(status_code=404, detail="Resource not found")
except ValidationError as e:
    raise HTTPException(status_code=400, detail=str(e))
except Exception as e:
    logger.error(f"Unexpected error: {str(e)}")
    raise HTTPException(status_code=500, detail="Internal server error")
```

### Location Validation
```python
if not region_service.validate_location(user_latitude, user_longitude):
    raise HTTPException(status_code=400, detail="Invalid location coordinates")
```

### Service Integration
All routers integrate with business logic services:
- **VMOrchestrator**: VM lifecycle orchestration
- **TensorDockService**: TensorDock API operations
- **GCPRegionService**: Google Cloud region optimization
- **BillingService**: Cost tracking and analysis
- **GeocodingService**: Location and distance calculations

### Response Patterns
- **Success**: HTTP 200 with data payload
- **Created**: HTTP 201 with resource location
- **Accepted**: HTTP 202 for async operations
- **Client Error**: HTTP 4xx with error detail
- **Server Error**: HTTP 5xx with generic message (detailed logging)