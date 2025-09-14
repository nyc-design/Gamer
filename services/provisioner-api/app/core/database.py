from pymongo import MongoClient
from app.core.config import settings
from app.models.vm import *
import logging

logger = logging.getLogger(__name__)

class Database:
    """ MongoDB client for immediate data access"""
    def __init__(self):
        self.client = MongoClient(settings.mongodb_atlas_uri)
        self.db = self.client.gamer
        self.consoles = self.db.consoles
        self.instances = self.db.instances

database = Database()


def get_console_config(console_type: ConsoleType):
    # Use find one from pymongo to grab console config from collection
    config = database.consoles.find_one({"console_type": console_type})

    # return config
    return ConsoleConfigDocument(**config) if config else None


def set_instance_status(vm_id: str, status: VMStatus):
    # Find instance doc using vm_id
    # Set status of instance doc to specified status
    result = database.instances.update_one(
        {"vm_id": vm_id}, 
        {"$set": {"status": status}}
    )

    # Return updated instance doc
    return database.instances.find_one({"vm_id": vm_id})


def update_instance_doc(vm_id: str, update_doc: VMDocument):
    # Find instance doc in instance collection
    # Update instance doc with latest VMDocument
    doc_dict = update_doc.dict(by_alias=True, exclude_none=True)
    database.instances.update_one(
        {"vm_id": vm_id},
        {"$set": doc_dict}
    )

    # Pass back update doc
    return database.instances.find_one({"vm_id": vm_id})


def add_new_instance(instance_doc: VMDocument, status: VMStatus):
    # Add instance doc to instances collection
    doc_dict = instance_doc.dict(by_alias=True, exclude_none=True)
    database.instances.insert_one(doc_dict)

    # Use set_instance_status to set status to "provisioning"
    set_instance_status(instance_doc.vm_id, status)

    # Pass back inserted doc


def get_instance(vm_id: str = None):
    # If vm_id is none, then get all instances that don't have status terminated, destroying, or error
    if vm_id is None:
        return list(database.instances.find({
            "status": {"$nin": [VMStatus.TERMINATED, VMStatus.DESTROYING, VMStatus.ERROR]}
        }))

    # Otherwise, if vm_id is set, get only that instance doc
    else:
        instance = database.instances.find_one({"vm_id": vm_id})

    # Return instance doc or list of instance docs
        return instance
