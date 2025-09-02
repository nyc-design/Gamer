import asyncio
import subprocess
import json
import logging
from typing import Optional, Dict, Any, Tuple
from app.models.vm import VMCreateRequest, VMStatusResponse, VMStatus, VMPreset, CloudProvider
from app.services.gcp_region_service import GCPRegionService
from app.services.vm_spec_service import VMSpecService
from app.core.config import settings

logger = logging.getLogger(__name__)

class CloudyPadService:
    """Service for interacting with CloudyPad CLI"""
    
    def __init__(self):
        self._active_vms: Dict[str, Dict[str, Any]] = {}
        self.gcp_region_service = GCPRegionService()
    
    async def provision_vm(self, vm_id: str, vm_request: VMCreateRequest, user_location: Optional[Tuple[float, float]] = None) -> bool:
        """Provision a new VM using CloudyPad CLI"""
        try:
            logger.info(f"Starting VM provisioning for {vm_id} with preset {vm_request.preset}")
            
            # Get VM specs based on preset
            vm_specs = self._get_vm_specs(vm_request.preset)
            
            # Get optimal GCP region based on user location
            gcp_region = None
            if user_location:
                try:
                    region_info = await self.gcp_region_service.get_closest_region_via_api(user_location)
                    if region_info:
                        gcp_region = region_info.get('region_code')
                        logger.info(f"Selected GCP region {gcp_region} ({region_info.get('region_name')}) for user location {user_location}")
                except Exception as e:
                    logger.warning(f"Error selecting GCP region: {e}, using default")
            
            # Build CloudyPad CLI command
            cmd = self._build_create_command(vm_id, vm_request, vm_specs, gcp_region)
            
            logger.info(f"Executing CloudyPad command: {' '.join(cmd)}")
            
            # Execute CloudyPad create command
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await process.communicate()
            
            if process.returncode == 0:
                logger.info(f"VM {vm_id} provisioned successfully")
                
                # Parse output for VM details (IP, etc.)
                vm_info = self._parse_create_output(stdout.decode())
                self._active_vms[vm_id] = vm_info
                
                return True
            else:
                logger.error(f"VM provisioning failed for {vm_id}: {stderr.decode()}")
                return False
                
        except Exception as e:
            logger.error(f"Error provisioning VM {vm_id}: {str(e)}")
            return False
    
    async def get_vm_status(self, vm_id: str) -> Optional[VMStatusResponse]:
        """Get VM status using CloudyPad CLI"""
        try:
            # For now, use stored VM info
            # TODO: Implement actual CloudyPad status check
            if vm_id in self._active_vms:
                vm_info = self._active_vms[vm_id]
                return VMStatusResponse(
                    vm_id=vm_id,
                    status=VMStatus.RUNNING,
                    ip_address=vm_info.get('ip_address')
                )
            return None
            
        except Exception as e:
            logger.error(f"Error getting VM status for {vm_id}: {str(e)}")
            return None
    
    async def stop_vm(self, vm_id: str) -> bool:
        """Stop VM using CloudyPad CLI"""
        try:
            cmd = ["cloudypad", "stop", vm_id]
            
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await process.communicate()
            
            if process.returncode == 0:
                logger.info(f"VM {vm_id} stopped successfully")
                return True
            else:
                logger.error(f"Failed to stop VM {vm_id}: {stderr.decode()}")
                return False
                
        except Exception as e:
            logger.error(f"Error stopping VM {vm_id}: {str(e)}")
            return False
    
    async def start_vm(self, vm_id: str) -> bool:
        """Start VM using CloudyPad CLI"""
        try:
            cmd = ["cloudypad", "start", vm_id]
            
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await process.communicate()
            
            if process.returncode == 0:
                logger.info(f"VM {vm_id} started successfully")
                return True
            else:
                logger.error(f"Failed to start VM {vm_id}: {stderr.decode()}")
                return False
                
        except Exception as e:
            logger.error(f"Error starting VM {vm_id}: {str(e)}")
            return False
    
    async def terminate_vm(self, vm_id: str) -> bool:
        """Terminate VM using CloudyPad CLI"""
        try:
            cmd = ["cloudypad", "destroy", vm_id]
            
            process = await asyncio.create_subprocess_exec(
                *cmd,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE
            )
            
            stdout, stderr = await process.communicate()
            
            if process.returncode == 0:
                logger.info(f"VM {vm_id} terminated successfully")
                if vm_id in self._active_vms:
                    del self._active_vms[vm_id]
                return True
            else:
                logger.error(f"Failed to terminate VM {vm_id}: {stderr.decode()}")
                return False
                
        except Exception as e:
            logger.error(f"Error terminating VM {vm_id}: {str(e)}")
            return False
    
    def _get_vm_specs(self, preset: VMPreset) -> Dict[str, Any]:
        """Get VM specifications from MongoDB based on preset"""
        specs = VMSpecService.get_vm_specs(preset)
        
        # Convert to CloudyPad format
        cloudypad_specs = {
            "cpu": specs["cpu"],
            "memory": specs["memory"],
            "gpu": "premium" if specs.get("gpu_count", 0) > 0 else None,
            "description": f"{preset.title()} tier VM"
        }
        
        return cloudypad_specs
    
    def _build_create_command(self, vm_id: str, vm_request: VMCreateRequest, vm_specs: Dict[str, Any], gcp_region: Optional[str] = None) -> list:
        """Build CloudyPad create command"""
        cmd = [
            "cloudypad", "create",
            "--name", vm_id,
            "--cpu", str(vm_specs["cpu"]),
            "--memory", f"{vm_specs['memory']}GB",
            "--autostop-enable",
            "--autostop-timeout", str(vm_request.auto_stop_timeout // 60),  # Convert to minutes
        ]
        
        # Add GPU if specified
        if vm_specs["gpu"]:
            cmd.extend(["--gpu", vm_specs["gpu"]])
        
        # Add provider-specific options
        if vm_request.provider == CloudProvider.TENSORDOCK:
            cmd.extend(["--provider", "tensordock"])
        elif vm_request.provider == CloudProvider.CLOUDYPAD_GCP:
            cmd.extend(["--provider", "gcp"])
            # Add GCP region if specified
            if gcp_region:
                cmd.extend(["--region", gcp_region])
        
        return cmd
    
    def _parse_create_output(self, output: str) -> Dict[str, Any]:
        """Parse CloudyPad create command output"""
        # TODO: Parse actual CloudyPad output format
        # For now, return dummy data
        return {
            "ip_address": "127.0.0.1",  # Placeholder
            "status": "running"
        }