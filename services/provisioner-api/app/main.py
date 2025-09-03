from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager
from app.routers import vms, config, launch, billing, vm_specs, gcp_regions, instances, console_config
from app.core.config import settings
from app.core.database import connect_to_mongo, close_mongo_connection
from app.core.sync_database import connect_sync_mongo, close_sync_mongo_connection

@asynccontextmanager
async def lifespan(app: FastAPI):
    # Startup
    await connect_to_mongo()
    connect_sync_mongo()  # Initialize sync connection for VM specs
    yield
    # Shutdown
    await close_mongo_connection()
    close_sync_mongo_connection()

app = FastAPI(
    title="Gamer Provisioner API",
    description="VM provisioning and lifecycle management for cloud gaming",
    version="1.0.0",
    lifespan=lifespan
)

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

app.include_router(vms.router, prefix="/vms", tags=["vms"])
app.include_router(config.router, prefix="/config", tags=["configuration"])
app.include_router(launch.router, prefix="/launch", tags=["game-launch"])
app.include_router(billing.router, prefix="/billing", tags=["billing"])
app.include_router(vm_specs.router, prefix="/vm-specs", tags=["vm-specs"])
app.include_router(gcp_regions.router, prefix="/gcp-regions", tags=["gcp-regions"])
app.include_router(instances.router, prefix="/instances", tags=["instances"])
app.include_router(console_config.router, prefix="/console-config", tags=["console-config"])

@app.get("/")
async def root():
    return {"message": "Gamer Provisioner API", "version": "1.0.0"}

@app.get("/health")
async def health():
    return {"status": "healthy", "service": "provisioner-api"}