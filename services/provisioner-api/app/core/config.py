from pydantic_settings import BaseSettings
from typing import Optional

class Settings(BaseSettings):
    # API Configuration
    api_host: str = "0.0.0.0"
    api_port: int = 8000
    
    # Database Configuration (MongoDB Atlas)
    mongodb_atlas_uri: Optional[str] = None
    database_name: str = "gamer"
    
    # Cloud Provider Settings
    tensordock_api_key: Optional[str] = None
    tensordock_api_token: Optional[str] = None
    gcs_bucket_name: Optional[str] = None
    gcp_project_id: Optional[str] = None
    gcp_billing_account: Optional[str] = None
    
    # CloudyPad Configuration
    cloudypad_config_path: Optional[str] = None
    
    # VM Configuration
    default_vm_timeout: int = 900  # 15 minutes in seconds
    
    class Config:
        env_file = ".env"

settings = Settings()