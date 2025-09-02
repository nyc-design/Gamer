# Provisioner API Application

## Overview
This directory contains the core FastAPI application code for the Provisioner API service. It implements VM lifecycle management, location-based optimization, and billing monitoring using a clean, modular architecture.

## Structure
```
app/
├── main.py          # FastAPI application entry point and configuration
├── core/            # Core functionality (config, database)
├── models/          # Data models and schemas
├── routers/         # API endpoint definitions
└── services/        # Business logic services
```

## main.py

The main application file that configures and initializes the FastAPI application.

### Functions and Components

#### lifespan(app: FastAPI) -> AsyncGenerator
- **Purpose**: Application lifecycle management using FastAPI's lifespan feature
- **Startup Phase**: 
  - Calls `connect_to_mongo()` to establish MongoDB Atlas connection
  - Initializes database connection pool
- **Shutdown Phase**:
  - Calls `close_mongo_connection()` to gracefully close database connections
  - Ensures clean resource cleanup
- **Parameters**: `app` - The FastAPI application instance
- **Returns**: AsyncGenerator for FastAPI lifespan context manager
- **Error Handling**: Database connection errors are logged and propagated

#### FastAPI Application Configuration
```python
app = FastAPI(
    title="Gamer Provisioner API",
    description="VM provisioning and lifecycle management for cloud gaming",
    version="1.0.0",
    lifespan=lifespan
)
```

#### CORS Middleware Setup
```python
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],          # Allow all origins (restrict in production)
    allow_credentials=True,       # Allow cookies and credentials
    allow_methods=["*"],          # Allow all HTTP methods
    allow_headers=["*"]           # Allow all headers
)
```

#### Router Registration
- **VMs Router**: `/vms` prefix - Direct VM management operations
- **Config Router**: `/config` prefix - Emulator and provider configuration
- **Launch Router**: `/launch` prefix - Intelligent game launching with location optimization
- **Billing Router**: `/billing` prefix - Usage tracking and cost monitoring

### Health Check Endpoints

#### GET /
- **Purpose**: Basic service identification and health check
- **Response**: 
  ```json
  {
    "message": "Gamer Provisioner API",
    "version": "1.0.0"
  }
  ```
- **Use Case**: Load balancer health checks and service discovery

#### GET /health
- **Purpose**: Detailed health status with service identification
- **Response**:
  ```json
  {
    "status": "healthy",
    "service": "provisioner-api"
  }
  ```
- **Use Case**: Kubernetes liveness/readiness probes and monitoring systems

## Application Architecture

### Async/Await Pattern
- All operations use async/await for non-blocking I/O
- Database operations are asynchronous using Beanie ODM
- HTTP client operations use httpx for async requests
- Background tasks for long-running operations

### Dependency Injection
- Database connections injected through FastAPI dependencies
- Service instances created at module level for reuse
- Configuration accessed through settings singleton

### Error Handling Strategy
- Global exception handlers for common errors
- Pydantic validation for request/response data
- Structured error responses with appropriate HTTP status codes
- Comprehensive logging for debugging and monitoring

### Middleware Stack
1. **CORS Middleware**: Cross-origin request handling
2. **Built-in FastAPI Middleware**: Request/response processing
3. **Custom Error Handling**: Application-specific error processing

## Configuration Integration

The application integrates with the core configuration system:
- Settings loaded from environment variables
- Database connection strings configured through `MONGODB_ATLAS_URI`
- Cloud provider credentials managed securely
- Feature flags and operational parameters

## Development vs Production

### Development Configuration
- Verbose logging enabled
- CORS allows all origins
- Debug mode for detailed error responses

### Production Configuration  
- Restricted CORS origins
- Production logging levels
- Error details limited for security
- Health check endpoints for monitoring

## Extension Points

### Adding New Routers
1. Create router file in `/routers/` directory
2. Import router in `main.py`
3. Register with `app.include_router()`

### Custom Middleware
1. Define middleware function or class
2. Add with `app.add_middleware()`
3. Consider order of middleware execution

### Background Tasks
- Use FastAPI's BackgroundTasks for short-running tasks
- Consider Celery or similar for long-running operations
- Implement proper error handling and logging

## Monitoring Integration

### Logging
- Structured logging with consistent format
- Request/response logging for API calls
- Error logging with context and stack traces
- Performance metrics logging

### Health Checks
- Basic health endpoint for load balancers
- Detailed health endpoint for monitoring systems
- Database connectivity validation
- External service dependency checks

### Metrics (Future Enhancement)
- Request count and duration metrics
- Database query performance
- External API call metrics
- Business metrics (VMs provisioned, games launched)