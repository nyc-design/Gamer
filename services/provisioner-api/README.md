# Provisioner API Service

## Overview
The Provisioner API is the central VM lifecycle management service for the Gamer cloud gaming platform. It handles provisioning, configuration, and management of gaming VMs across multiple cloud providers (TensorDock and CloudyPad/GCP), with intelligent location-based optimization and comprehensive billing monitoring.

## Architecture
- **Framework**: FastAPI with async/await patterns
- **Database**: MongoDB Atlas with Beanie ODM
- **Authentication**: JWT tokens and API keys
- **Deployment**: Google Cloud Run (containerized)
- **Key Features**:
  - Dual-provider VM orchestration
  - Location-based optimal hostnode/region selection
  - Automatic gaming environment setup
  - Real-time billing and usage tracking
  - Console-specific VM configuration

## Service Structure
```
provisioner-api/
├── app/
│   ├── main.py              # FastAPI application entry point
│   ├── core/                # Core configuration and database
│   ├── models/              # Data models and schemas
│   ├── routers/             # API endpoint definitions
│   └── services/            # Business logic services
├── Dockerfile               # Container configuration
└── requirements.txt         # Python dependencies
```

## Files and Functions

### main.py
Main FastAPI application setup and configuration.

#### lifespan(app: FastAPI) -> AsyncGenerator
- **Purpose**: Manages application startup and shutdown lifecycle
- **Startup**: Connects to MongoDB Atlas database
- **Shutdown**: Closes database connections gracefully
- **Returns**: AsyncGenerator for FastAPI lifespan management

#### FastAPI App Configuration
- **Title**: "Gamer Provisioner API"
- **Version**: "1.0.0"
- **CORS**: Configured for cross-origin requests from web app
- **Routers**: Includes VMs, config, launch, and billing endpoints

### Root Endpoints
#### GET /
- **Purpose**: Service health check and identification
- **Returns**: `{"message": "Gamer Provisioner API", "version": "1.0.0"}`

#### GET /health
- **Purpose**: Detailed health status for load balancers
- **Returns**: `{"status": "healthy", "service": "provisioner-api"}`

## Core Components

The provisioner API is organized into several key modules:

1. **Core** (`/core/`) - Configuration and database connection management
2. **Models** (`/models/`) - Data schemas and MongoDB document definitions
3. **Routers** (`/routers/`) - REST API endpoints organized by functionality
4. **Services** (`/services/`) - Business logic for VM management, billing, and location optimization

## Key Features

### Dual Provider Architecture
- **TensorDock**: Direct API integration with distance-based hostnode selection
- **CloudyPad**: CLI integration with GCP region optimization via Google Cloud Location Finder

### Location Intelligence
- Browser geolocation integration (GPS coordinates)
- TensorDock: Real hostnode distance calculation using city/country geocoding
- CloudyPad: Google Cloud Location Finder API with fallback to distance calculation

### Gaming Environment Setup
- Console-specific VM specifications (CPU, RAM, GPU requirements)
- Automatic CloudyPad deployment via SSH for TensorDock VMs
- Wolf streaming server configuration
- Game library mounting and save file management

### Billing and Cost Management
- Real-time usage tracking across all providers
- Cost estimation based on VM specifications and runtime
- Configurable spending alerts and limits
- Daily and monthly usage reports

## Configuration

### Environment Variables
```bash
# Database
MONGODB_ATLAS_URI=mongodb+srv://username:password@cluster.mongodb.net/gamer

# Cloud Providers  
TENSORDOCK_API_KEY=your-tensordock-api-key
TENSORDOCK_API_TOKEN=your-tensordock-api-token
GCP_PROJECT_ID=your-gcp-project-id
GCP_BILLING_ACCOUNT=your-gcp-billing-account-id

# Storage
GCS_BUCKET_NAME=your-gcs-bucket-name

# CloudyPad
CLOUDYPAD_CONFIG_PATH=/path/to/cloudypad/config
```

### VM Presets
- **RETRO**: 2 vCPU, 4GB RAM, no GPU (NES/SNES/GB/GBA)
- **ADVANCED**: 4 vCPU, 8GB RAM, basic GPU (DS/3DS)
- **PREMIUM**: 8 vCPU, 16GB RAM, high-end GPU (GC/Wii/Switch)

## Usage Examples

### Launch a Game with Location Optimization
```python
# Browser sends GPS coordinates from navigator.geolocation
response = await client.post("/launch/game", json={
    "console_type": "switch",
    "game_id": "mario-odyssey",
    "user_id": "user123",
    "user_latitude": 32.7767,  # Dallas coordinates
    "user_longitude": -96.7970
})
```

### Get Optimal Hostnode Recommendations
```python
# Find closest TensorDock hostnodes
response = await client.get(
    "/launch/hostnodes/closest",
    params={
        "user_latitude": 32.7767,
        "user_longitude": -96.7970,
        "console_type": "switch",
        "limit": 5
    }
)
```

### Monitor Billing and Usage
```python
# Get current month costs
response = await client.get("/billing/costs/current-month")

# Check spending alerts
response = await client.get(
    "/billing/alerts",
    params={"daily_limit": 50.0, "monthly_limit": 500.0}
)
```

## API Endpoints Overview

- **VM Management** (`/vms/*`) - Direct VM operations
- **Configuration** (`/config/*`) - Emulator and provider configuration
- **Game Launch** (`/launch/*`) - Intelligent game launching with optimization
- **Billing** (`/billing/*`) - Usage tracking and cost monitoring

## Error Handling

All endpoints include comprehensive error handling with:
- Input validation using Pydantic models
- Database connection error recovery  
- Provider API failure fallbacks
- Structured error responses with HTTP status codes
- Detailed logging for debugging and monitoring

## Performance Considerations

- Async/await throughout for non-blocking I/O
- Connection pooling for database and HTTP clients
- Caching of geocoding results and region mappings
- Background task processing for long-running operations
- Efficient MongoDB queries with proper indexing

## Security Features

- API key validation for service-to-service communication
- Input sanitization and validation
- No credential logging or exposure
- Scoped database access
- CORS protection with configured origins