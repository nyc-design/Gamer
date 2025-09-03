from fastapi import APIRouter
from typing import List
from app.models.console_config import ConsoleConfig
from app.models.vm import ConsoleType
from app.core.sync_database import get_sync_database

router = APIRouter()

@router.get("/", response_model=List[ConsoleConfig])
def list_console_configs():
    """Get all console configurations"""
    db = get_sync_database()
    config_data_list = list(db.console_configs.find({}))
    return [ConsoleConfig(**config_data) for config_data in config_data_list]

@router.post("/seed-defaults")
def seed_console_configs():
    """Create default console configurations"""
    
    configs = [
        {
            "console_type": ConsoleType.NES,
            "tensordock_gpus": [],  # No GPU needed
            "gcp_machine_types": ["e2-standard-2", "n1-standard-2"],
            "min_cpu": 2,
            "min_ram_gb": 4
        },
        {
            "console_type": ConsoleType.SNES, 
            "tensordock_gpus": [],
            "gcp_machine_types": ["e2-standard-2", "n1-standard-2"],
            "min_cpu": 2,
            "min_ram_gb": 4
        },
        {
            "console_type": ConsoleType.GBA,
            "tensordock_gpus": [],
            "gcp_machine_types": ["e2-standard-2", "n1-standard-2"], 
            "min_cpu": 2,
            "min_ram_gb": 4
        },
        {
            "console_type": ConsoleType.NDS,
            "tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"],
            "gcp_machine_types": ["n1-standard-4", "n2-standard-4"],
            "min_cpu": 4,
            "min_ram_gb": 8
        },
        {
            "console_type": ConsoleType.N3DS,
            "tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"],
            "gcp_machine_types": ["n1-standard-4", "n2-standard-4"],
            "min_cpu": 4,
            "min_ram_gb": 8
        },
        {
            "console_type": ConsoleType.SWITCH,
            "tensordock_gpus": ["RTX3070", "RTX3080", "RTX4070", "RTX4080", "RTX4090"],
            "gcp_machine_types": ["n1-standard-8", "n2-standard-8"],
            "min_cpu": 8,
            "min_ram_gb": 16
        },
        {
            "console_type": ConsoleType.GAMECUBE,
            "tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"],
            "gcp_machine_types": ["n1-standard-4", "n2-standard-4"],
            "min_cpu": 4,
            "min_ram_gb": 8
        },
        {
            "console_type": ConsoleType.WII,
            "tensordock_gpus": ["GTX1060", "RTX3060", "RTX3070", "RTX3080", "RTX4090"],
            "gcp_machine_types": ["n1-standard-4", "n2-standard-4"],
            "min_cpu": 4, 
            "min_ram_gb": 8
        }
    ]
    
    db = get_sync_database()
    created = 0
    
    for config_data in configs:
        existing = db.console_configs.find_one({"console_type": config_data["console_type"]})
        if not existing:
            db.console_configs.insert_one(config_data)
            created += 1
    
    return {"message": f"Created {created} console configurations"}