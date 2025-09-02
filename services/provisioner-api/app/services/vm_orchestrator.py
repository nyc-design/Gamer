import asyncio
import logging
from typing import Optional
from app.models.vm import VMDocument, VMStatus, CloudProvider
from app.services.tensordock_service import TensorDockService
from app.services.cloudypad_service import CloudyPadService
from app.services.region_service import RegionService
from app.core.database import get_database
from datetime import datetime

logger = logging.getLogger(__name__)

class VMOrchestrator:
    """Orchestrates VM provisioning across different providers"""
    
    def __init__(self):
        self.tensordock_service = TensorDockService()
        self.cloudypad_service = CloudyPadService()
        self.region_service = RegionService()
    
    async def _update_vm(self, vm: VMDocument) -> VMDocument:
        """Helper method to update VM document in database"""
        db = get_database()
        vm_dict = vm.dict(by_alias=True, exclude_none=True)
        vm_dict["updated_at"] = datetime.utcnow()
        
        await db.vms.replace_one(
            {"vm_id": vm.vm_id}, 
            vm_dict
        )
        return vm
    
    async def provision_and_launch_game(
        self, 
        vm_id: str, 
        game_id: str, 
        save_id: Optional[str] = None
    ):
        """Complete workflow: provision VM, setup gaming environment, launch game"""
        
        try:
            db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            if not vm_data:
                logger.error(f"VM {vm_id} not found")
                return
            vm = VMDocument(**vm_data)
            
            logger.info(f"Starting provisioning workflow for VM {vm_id}")
            
            # Phase 1: Provision VM based on provider
            if vm.provider == CloudProvider.TENSORDOCK:
                success = await self._provision_tensordock_vm(vm)
            else:
                success = await self._provision_cloudypad_vm(vm)
            
            if not success:
                vm.status = VMStatus.ERROR
                await self._update_vm(vm)
                logger.error(f"VM {vm_id} provisioning failed")
                return
            
            # Phase 2: Setup gaming environment
            logger.info(f"Setting up gaming environment for VM {vm_id}")
            vm.status = VMStatus.CONFIGURING
            await self._update_vm(vm)
            
            success = await self._setup_gaming_environment(vm)
            if not success:
                vm.status = VMStatus.ERROR
                await self._update_vm(vm)
                logger.error(f"Gaming environment setup failed for VM {vm_id}")
                return
            
            # Phase 3: Launch the game
            vm.status = VMStatus.RUNNING
            vm.gaming_environment_ready = True
            vm.last_activity = datetime.utcnow()
            await self._update_vm(vm)
            
            logger.info(f"VM {vm_id} ready, launching game {game_id}")
            await self.launch_game_on_vm(vm_id, game_id, save_id)
            
        except Exception as e:
            logger.error(f"Error in provisioning workflow for VM {vm_id}: {str(e)}")
            db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            vm = VMDocument(**vm_data) if vm_data else None
            if vm:
                vm.status = VMStatus.ERROR
                await self._update_vm(vm)
    
    async def _provision_tensordock_vm(self, vm: VMDocument) -> bool:
        """Provision TensorDock VM and setup for CloudyPad deployment"""
        
        logger.info(f"Provisioning TensorDock VM {vm.vm_id}")
        
        # Get user location coordinates if available
        user_location = None
        if hasattr(vm, 'user_location') and vm.user_location:
            try:
                lat = vm.user_location.get('latitude')
                lon = vm.user_location.get('longitude')
                if lat and lon:
                    user_location = (lat, lon)
                    logger.info(f"Using user location ({lat}, {lon}) for hostnode selection")
            except Exception as e:
                logger.warning(f"Failed to parse user location: {e}, using default selection")
        
        # Create VM via TensorDock API with optimal hostnode selection
        result = await self.tensordock_service.create_vm(
            vm.vm_id, 
            vm.console_type, 
            vm.preset,
            user_location
        )
        
        if not result.get("success"):
            logger.error(f"TensorDock VM creation failed: {result.get('error')}")
            return False
        
        # Update VM with TensorDock details
        vm.provider_instance_id = result.get("instance_id")
        vm.ip_address = result.get("ip_address")
        vm.provider_metadata = result.get("metadata", {})
        
        # Wait for VM to be ready
        await self._wait_for_vm_ready(vm)
        
        await self._update_vm(vm)
        logger.info(f"TensorDock VM {vm.vm_id} provisioned successfully")
        return True
    
    async def _provision_cloudypad_vm(self, vm: VMDocument) -> bool:
        """Provision VM using CloudyPad directly"""
        
        logger.info(f"Provisioning CloudyPad VM {vm.vm_id}")
        
        # Get user location coordinates if available
        user_location = None
        if hasattr(vm, 'user_location') and vm.user_location:
            try:
                lat = vm.user_location.get('latitude')
                lon = vm.user_location.get('longitude')
                if lat and lon:
                    user_location = (lat, lon)
                    logger.info(f"Using user location ({lat}, {lon}) for CloudyPad GCP region selection")
            except Exception as e:
                logger.warning(f"Failed to parse user location for CloudyPad: {e}")
        
        # Use existing CloudyPad service with user location
        success = await self.cloudypad_service.provision_vm(
            vm.vm_id,
            # Convert our request format to CloudyPad format
            type('VMCreateRequest', (), {
                'preset': vm.preset,
                'provider': vm.provider,
                'auto_stop_timeout': vm.auto_stop_timeout
            })(),
            user_location
        )
        
        if success:
            # Get VM details from CloudyPad
            status = await self.cloudypad_service.get_vm_status(vm.vm_id)
            if status:
                vm.ip_address = status.get("ip_address")
            
            await self._update_vm(vm)
            logger.info(f"CloudyPad VM {vm.vm_id} provisioned successfully")
            return True
        
        logger.error(f"CloudyPad VM {vm.vm_id} provisioning failed")
        return False
    
    async def _setup_gaming_environment(self, vm: VMDocument) -> bool:
        """Setup gaming environment on VM"""
        
        if vm.provider == CloudProvider.TENSORDOCK:
            # For TensorDock VMs, use CloudyPad SSH provider to setup gaming environment
            return await self._setup_cloudypad_via_ssh(vm)
        else:
            # For CloudyPad native providers, gaming environment is already setup
            vm.cloudypad_configured = True
            vm.games_mounted = True
            await self._update_vm(vm)
            return True
    
    async def _setup_cloudypad_via_ssh(self, vm: VMDocument) -> bool:
        """Use CloudyPad SSH provider to setup gaming environment on TensorDock VM"""
        
        try:
            logger.info(f"Setting up CloudyPad via SSH on VM {vm.vm_id}")
            
            # TODO: This is where we would call CloudyPad's SSH provider
            # to deploy the gaming environment to the existing TensorDock VM
            
            # For now, simulate the setup process
            await asyncio.sleep(30)  # Simulate setup time
            
            vm.cloudypad_configured = True
            vm.games_mounted = True
            await self._update_vm(vm)
            
            logger.info(f"CloudyPad SSH setup completed for VM {vm.vm_id}")
            return True
            
        except Exception as e:
            logger.error(f"CloudyPad SSH setup failed for VM {vm.vm_id}: {str(e)}")
            return False
    
    async def _wait_for_vm_ready(self, vm: VMDocument, max_wait_minutes: int = 10):
        """Wait for VM to be ready for SSH connections"""
        
        logger.info(f"Waiting for VM {vm.vm_id} to be ready...")
        
        for _ in range(max_wait_minutes * 6):  # Check every 10 seconds
            if vm.provider == CloudProvider.TENSORDOCK:
                status = await self.tensordock_service.get_vm_status(vm.provider_instance_id)
                if status and status.get("status") == "running":
                    logger.info(f"TensorDock VM {vm.vm_id} is ready")
                    return True
            
            await asyncio.sleep(10)
        
        logger.warning(f"VM {vm.vm_id} not ready after {max_wait_minutes} minutes")
        return False
    
    async def launch_game_on_vm(
        self, 
        vm_id: str, 
        game_id: str, 
        save_id: Optional[str] = None
    ):
        """Launch a specific game on a running VM"""
        
        try:
            logger.info(f"Launching game {game_id} on VM {vm_id}")
            
            db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            vm = VMDocument(**vm_data) if vm_data else None
            if not vm:
                logger.error(f"VM {vm_id} not found")
                return
            
            if not vm.gaming_environment_ready:
                logger.error(f"Gaming environment not ready on VM {vm_id}")
                return
            
            # TODO: Call Agent API to launch game on VM
            # This would involve:
            # 1. Mounting the specific game file
            # 2. Loading the save file if provided  
            # 3. Starting the appropriate emulator
            # 4. Monitoring for game launch success
            
            logger.info(f"Game {game_id} launch initiated on VM {vm_id}")
            
            # Update activity tracking
            vm.last_activity = datetime.utcnow()
            await self._update_vm(vm)
            
        except Exception as e:
            logger.error(f"Error launching game {game_id} on VM {vm_id}: {str(e)}")
    
    async def stop_vm(self, vm_id: str) -> bool:
        """Stop a running VM using the appropriate provider"""
        
        db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            vm = VMDocument(**vm_data) if vm_data else None
        if not vm:
            return False
        
        if vm.provider == CloudProvider.TENSORDOCK:
            success = await self.tensordock_service.stop_vm(vm.provider_instance_id)
        else:
            success = await self.cloudypad_service.stop_vm(vm_id)
        
        if success:
            vm.status = VMStatus.STOPPED
            await self._update_vm(vm)
        
        return success
    
    async def terminate_vm(self, vm_id: str) -> bool:
        """Terminate a VM using the appropriate provider"""
        
        db = get_database()
            vm_data = await db.vms.find_one({"vm_id": vm_id})
            vm = VMDocument(**vm_data) if vm_data else None
        if not vm:
            return False
        
        if vm.provider == CloudProvider.TENSORDOCK:
            success = await self.tensordock_service.terminate_vm(vm.provider_instance_id)
        else:
            success = await self.cloudypad_service.terminate_vm(vm_id)
        
        if success:
            vm.status = VMStatus.TERMINATED
            await self._update_vm(vm)
        
        return success