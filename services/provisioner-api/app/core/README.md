# Core Configuration and Database

## Overview
The core module provides essential infrastructure services for the Provisioner API including configuration management and database connectivity. It ensures consistent settings across the application and manages the MongoDB Atlas connection lifecycle.

## Files

### config.py
Application configuration management using Pydantic settings with environment variable support.

#### Settings Class
```python
class Settings(BaseSettings):
```

**Environment Variables and Default Values:**

#### API Configuration
- **api_host**: `str = "0.0.0.0"` - Host interface for the FastAPI server
- **api_port**: `int = 8000` - Port for the FastAPI server

#### Database Configuration (MongoDB Atlas)
- **mongodb_atlas_uri**: `Optional[str] = None` - MongoDB Atlas connection string
- **database_name**: `str = "gamer"` - Default database name for collections

#### Cloud Provider Settings
- **tensordock_api_key**: `Optional[str] = None` - API key for TensorDock service
- **tensordock_api_token**: `Optional[str] = None` - API token for TensorDock service (often same as key)
- **gcs_bucket_name**: `Optional[str] = None` - Google Cloud Storage bucket for game files
- **gcp_project_id**: `Optional[str] = None` - Google Cloud Project ID for Location Finder API
- **gcp_billing_account**: `Optional[str] = None` - GCP billing account for cost tracking

#### CloudyPad Configuration
- **cloudypad_config_path**: `Optional[str] = None` - Path to CloudyPad CLI configuration file

#### VM Configuration
- **default_vm_timeout**: `int = 900` - Default VM timeout in seconds (15 minutes)

#### Config Class
```python
class Config:
    env_file = ".env"
```
- **Purpose**: Instructs Pydantic to load settings from `.env` file
- **Precedence**: Environment variables override `.env` file values

#### Settings Singleton
```python
settings = Settings()
```
- **Purpose**: Global settings instance used throughout the application
- **Usage**: Import with `from app.core.config import settings`
- **Thread Safety**: Pydantic Settings are thread-safe after initialization

### database.py
MongoDB Atlas connection management using Motor async driver and Beanie ODM.

#### connect_to_mongo() -> None
- **Purpose**: Establishes connection to MongoDB Atlas and initializes Beanie ODM
- **Process**:
  1. Creates Motor AsyncIOMotorClient with connection string
  2. Initializes Beanie with document models
  3. Registers all document classes for ODM functionality
- **Parameters**: None (uses `settings.mongodb_atlas_uri`)
- **Returns**: None
- **Raises**: 
  - `Exception` if connection string is missing
  - `PyMongoError` if database connection fails
- **Side Effects**: Sets up global database client and document models

#### close_mongo_connection() -> None
- **Purpose**: Gracefully closes MongoDB connection
- **Process**:
  1. Closes Motor client connection
  2. Cleans up connection pool
  3. Releases database resources
- **Parameters**: None
- **Returns**: None
- **Error Handling**: Catches and logs connection closure errors
- **Usage**: Called during application shutdown

## Configuration Patterns

### Environment Variable Loading
The configuration system supports multiple sources in order of precedence:
1. **Environment Variables** - Highest priority
2. **`.env` File** - Local development configuration  
3. **Default Values** - Fallback values defined in Settings class

### Naming Conventions
- Environment variables use UPPERCASE with underscores: `MONGODB_ATLAS_URI`
- Settings attributes use lowercase with underscores: `mongodb_atlas_uri`
- Automatic conversion between naming conventions

### Optional vs Required Settings
- **Required for Production**: `mongodb_atlas_uri`, `tensordock_api_key`
- **Optional**: Most cloud provider settings have sensible defaults
- **Development**: Can run with minimal configuration using defaults

## Database Architecture

### MongoDB Atlas Integration
- **Driver**: Motor (async MongoDB driver)
- **ODM**: Beanie (async ODM built on Pydantic)
- **Connection**: Single connection pool shared across application
- **Collections**: Defined by Beanie document models

### Document Models Registered
- **VMDocument**: Virtual machine instances and metadata
- **EmulatorConfigDocument**: Console-specific configuration and provider preferences

### Connection Management
- **Startup**: Connection established during FastAPI lifespan startup
- **Runtime**: Connection pool handles concurrent requests
- **Shutdown**: Graceful connection closure during lifespan shutdown

## Security Considerations

### Sensitive Data Handling
- API keys and tokens stored in environment variables
- Database connection strings contain credentials
- No default values for sensitive settings (forces explicit configuration)

### Connection Security
- MongoDB Atlas requires TLS encryption
- Connection strings should use `mongodb+srv://` format
- Authentication handled through connection string credentials

## Usage Examples

### Accessing Configuration
```python
from app.core.config import settings

# Database operations
database_name = settings.database_name
connection_uri = settings.mongodb_atlas_uri

# API configuration
host = settings.api_host
port = settings.api_port

# Cloud provider settings
if settings.tensordock_api_key:
    # Initialize TensorDock client
    pass
```

### Database Operations
```python
from app.core.database import connect_to_mongo, close_mongo_connection

# Application startup
await connect_to_mongo()

# Application shutdown  
await close_mongo_connection()
```

### Environment Configuration
```bash
# .env file
MONGODB_ATLAS_URI=mongodb+srv://user:pass@cluster.mongodb.net/gamer
TENSORDOCK_API_KEY=your-api-key
GCP_PROJECT_ID=your-project-id
```

## Error Handling

### Configuration Errors
- Missing required settings raise ValueError with clear messages
- Invalid URLs or formats caught during validation
- Environment variable parsing errors include variable name

### Database Errors
- Connection failures logged with detailed error information
- Retry logic for transient connection issues
- Graceful degradation when database unavailable

## Development vs Production

### Development Settings
- Local MongoDB instance support
- Debug logging enabled
- Relaxed security settings for development

### Production Settings
- MongoDB Atlas required
- All security settings enforced
- Production logging levels
- Health check integration

## Extension Points

### Adding New Settings
1. Add attribute to Settings class with type annotation
2. Provide default value or mark as Optional
3. Add corresponding environment variable
4. Update documentation

### Custom Validation
```python
@validator('mongodb_atlas_uri')
def validate_mongo_uri(cls, v):
    if v and not v.startswith('mongodb'):
        raise ValueError('Invalid MongoDB URI format')
    return v
```

### Additional Databases
- Extend database.py with additional connection functions
- Register new document models in connect_to_mongo()
- Add connection management for multiple databases