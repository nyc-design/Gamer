from pymongo import MongoClient
from app.core.config import settings
import logging

logger = logging.getLogger(__name__)

class SyncDatabase:
    """Synchronous MongoDB client for immediate data access"""
    client: MongoClient = None
    db = None
    
sync_database = SyncDatabase()

def connect_sync_mongo():
    """Create synchronous database connection to MongoDB Atlas"""
    try:
        sync_database.client = MongoClient(settings.mongodb_atlas_uri)
        sync_database.db = sync_database.client[settings.database_name]
        
        # Test the connection
        sync_database.client.admin.command('ping')
        
        logger.info(f"Connected to MongoDB using sync client: {settings.database_name}")
        
    except Exception as e:
        logger.error(f"Failed to connect to MongoDB with sync client: {e}")
        raise

def close_sync_mongo_connection():
    """Close synchronous database connection"""
    if sync_database.client:
        sync_database.client.close()
        logger.info("Disconnected from MongoDB (sync client)")

def get_sync_database():
    """Get the synchronous database instance for collections"""
    if sync_database.db is None:
        connect_sync_mongo()
    return sync_database.db