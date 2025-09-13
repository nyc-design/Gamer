from pymongo import MongoClient
from app.core.config import settings
from modles.vm import *
import logging

logger = logging.getLogger(__name__)

class Database:
    """ MongoDB client for immediate data access"""
    client: MongoClient(settings.mongodb_atlas_uri)
    
database = Database()
database.consoles = database.client.configurations.consoles
database.instances = database.client.server.instances


def get_console_config(console_type: ConsoleType):
    # Use find one from pymongo to grab console config from collection

    # return config


def set_instance_status(vm_id: str, status: VMStatus):
    # Find instance doc using vm_id

    # Set status of instance doc to specified status

    # Return updated instance doc

def update_instance_doc(vm_id: str, update_doc: VMDocument):
    # Find instance doc in instance collection

    # Update instance doc with latest VMDocument

    # Pass back update doc


def add_new_instance(instance_doc: VMDocument, status: VMStatus):
    # Add instance doc to instances collection

    # Use set_instance_status to set status to "provisioning"


def get_instance(vm_ids: str = None):
    # If vm_id is none, then get all instances that don't have status terminated, destroying, or error

    # Otherwise, if vm_id is set, get only that intsance doc

    # Return instance doc or list of instance docs
