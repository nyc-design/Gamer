from pymongo import MongoClient
from app.core.config import settings
import logging

logger = logging.getLogger(__name__)

class Database:
    """ MongoDB client for immediate data access"""
    client: MongoClient(settings.mongodb_atlas_uri)
    
database = Database()

def get_client():
    """Get the database instance for collections"""
    return database.client