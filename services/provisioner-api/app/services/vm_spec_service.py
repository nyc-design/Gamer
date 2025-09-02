from typing import Dict, Any, Optional, List
from app.models.vm_preset import VMSpecDocument
from app.models.vm import VMPreset, ConsoleType
from app.core.sync_database import get_sync_database
import logging

logger = logging.getLogger(__name__)

class VMSpecService:
    """Service for managing VM specifications from MongoDB"""
    
    @staticmethod
    def get_vm_specs(preset: VMPreset, console_type: Optional[ConsoleType] = None) -> Dict[str, Any]:
        """Get VM specifications from MongoDB based on preset and optional console type"""
        try:
            db = get_sync_database()
            
            # Find the preset document
            spec_data = db.vm_specs.find_one({"preset": preset})
            if not spec_data:
                logger.error(f"VM spec not found for preset: {preset}")
                raise ValueError(f"No VM specifications found for preset: {preset}")
            
            spec = VMSpecDocument(**spec_data)
            
            # Base specifications
            base_specs = {
                "cpu": spec.cpu_cores,
                "memory": spec.ram_gb,
                "storage": spec.storage_gb,
                "gpu_count": 1 if spec.requires_gpu else 0,
                "gpu_memory_gb": spec.gpu_memory_gb,
                "estimated_cost_per_hour": spec.estimated_cost_per_hour_usd,
                "performance_tier": spec.performance_tier
            }
            
            # Add provider-specific configurations
            if spec.tensordock_config:
                base_specs.update(spec.tensordock_config)
                
            # Apply console-specific adjustments if provided
            if console_type and str(console_type) in spec.suitable_consoles:
                # For now, we'll use the base specs, but this allows for future 
                # console-specific overrides stored in the database
                pass
            
            return base_specs
            
        except Exception as e:
            logger.error(f"Error getting VM specs for preset {preset}: {str(e)}")
            # Fallback to hardcoded specs for reliability
            return VMSpecService._get_fallback_specs(preset)
    
    @staticmethod
    def _get_fallback_specs(preset: VMPreset) -> Dict[str, Any]:
        """Fallback hardcoded specs if database lookup fails"""
        logger.warning(f"Using fallback specs for preset: {preset}")
        
        fallback_specs = {
            VMPreset.RETRO: {
                "cpu": 2,
                "memory": 4,
                "storage": 50,
                "gpu_count": 0,
                "estimated_cost_per_hour": 0.15,
                "performance_tier": "low"
            },
            VMPreset.ADVANCED: {
                "cpu": 4,
                "memory": 8,
                "storage": 100,
                "gpu_count": 1,
                "gpu_memory_gb": 6,
                "estimated_cost_per_hour": 0.35,
                "performance_tier": "medium"
            },
            VMPreset.PREMIUM: {
                "cpu": 8,
                "memory": 16,
                "storage": 200,
                "gpu_count": 1,
                "gpu_memory_gb": 24,
                "estimated_cost_per_hour": 1.20,
                "performance_tier": "high"
            }
        }
        
        return fallback_specs.get(preset, fallback_specs[VMPreset.ADVANCED])
    
    @staticmethod
    def list_all_specs() -> List[VMSpecDocument]:
        """Get all VM specifications from MongoDB"""
        try:
            db = get_sync_database()
            specs_data = list(db.vm_specs.find({}))
            return [VMSpecDocument(**spec_data) for spec_data in specs_data]
        except Exception as e:
            logger.error(f"Error listing VM specs: {str(e)}")
            return []
    
    @staticmethod
    def create_or_update_spec(spec_data: Dict[str, Any]) -> VMSpecDocument:
        """Create or update a VM specification in MongoDB"""
        try:
            db = get_sync_database()
            
            # Check if spec exists
            existing = db.vm_specs.find_one({"preset": spec_data["preset"]})
            
            spec = VMSpecDocument(**spec_data)
            
            if existing:
                # Update existing
                result = db.vm_specs.replace_one(
                    {"preset": spec.preset},
                    spec.dict(by_alias=True, exclude_none=True)
                )
                logger.info(f"Updated VM spec for preset: {spec.preset}")
            else:
                # Create new
                result = db.vm_specs.insert_one(
                    spec.dict(by_alias=True, exclude_none=True)
                )
                logger.info(f"Created new VM spec for preset: {spec.preset}")
                
            return spec
            
        except Exception as e:
            logger.error(f"Error creating/updating VM spec: {str(e)}")
            raise