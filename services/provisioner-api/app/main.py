from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from contextlib import asynccontextmanager
from app.routers import gaming
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

app.include_router(gaming.router, prefix="/gaming", tags=["gaming"])

@app.get("/")
async def root():
    return {"message": "Gamer Provisioner API", "version": "1.0.0"}

@app.get("/health")
async def health():
    return {"status": "healthy", "service": "provisioner-api"}