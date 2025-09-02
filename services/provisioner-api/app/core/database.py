from pymongo import AsyncMongoClient
from app.core.config import settings
import logging

logger = logging.getLogger(__name__)

class Database:
    client: AsyncMongoClient = None
    db = None
    
database = Database()

async def connect_to_mongo():
    """Create database connection to MongoDB Atlas using PyMongo async"""
    try:
        database.client = AsyncMongoClient(settings.mongodb_atlas_uri)
        database.db = database.client[settings.database_name]
        
        # Test the connection
        await database.client.admin.command('ping')
        
        logger.info(f"Connected to MongoDB using PyMongo async: {settings.database_name}")
        
    except Exception as e:
        logger.error(f"Failed to connect to MongoDB: {e}")
        raise

async def close_mongo_connection():
    """Close database connection"""
    if database.client:
        database.client.close()
        logger.info("Disconnected from MongoDB")

def get_database():
    """Get the database instance for collections"""
    if database.db is None:
        raise RuntimeError("Database not initialized. Call connect_to_mongo() first.")
    return database.db