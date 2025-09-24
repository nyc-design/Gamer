import os
import logging
from typing import Optional
from pathlib import Path

logger = logging.getLogger(__name__)

class StartupScriptService:
    """Service for managing the shared gaming VM startup script"""

    @staticmethod
    def get_gaming_vm_startup_script() -> str:
        """
        Load the gaming VM startup script

        Implementation checklist:
        [x] Get script file path relative to this service file
        [x] Read script content from file
        [x] Handle FileNotFoundError with descriptive message
        [x] Return script content

        Returns:
            The gaming VM startup script content
        """
        # Get the script file path relative to this service file
        script_path = Path(__file__).parent.parent / "scripts" / "gaming_vm_startup.sh"

        try:
            with open(script_path, 'r') as f:
                script_content = f.read()
        except FileNotFoundError:
            raise FileNotFoundError(f"Gaming VM startup script not found at {script_path}")

        return script_content

    @staticmethod
    def get_script_hash() -> str:
        """
        Get a hash of the startup script for versioning/caching purposes

        Implementation checklist:
        [x] Get script file path relative to this service file
        [x] Read script content as bytes
        [x] Generate SHA256 hash and return first 16 characters
        [x] Handle FileNotFoundError by returning "unknown"

        Returns:
            SHA256 hash of the script content
        """
        import hashlib

        script_path = Path(__file__).parent.parent / "scripts" / "gaming_vm_startup.sh"

        try:
            with open(script_path, 'rb') as f:
                script_bytes = f.read()
            return hashlib.sha256(script_bytes).hexdigest()[:16]  # First 16 chars
        except FileNotFoundError:
            return "unknown"

    @staticmethod
    def validate_script_requirements() -> bool:
        """
        Validate that the startup script contains required gaming VM components

        Implementation checklist:
        [ ] Load the startup script content
        [ ] Check for Docker installation commands
        [ ] Check for CloudyPad container setup
        [ ] Check for auto-stop service configuration
        [ ] Log validation results
        [ ] Return True if all requirements found, False otherwise
        """
        # Load the startup script content
        try:
            script_content = StartupScriptService.get_gaming_vm_startup_script()
        except FileNotFoundError:
            logger.error("Cannot validate script - startup script file not found")
            return False

        # Check for Docker installation commands
        has_docker = "docker" in script_content.lower()

        # Check for CloudyPad container setup
        has_cloudypad = "cloudypad" in script_content.lower()

        # Check for auto-stop service configuration
        has_autostop = "gaming-autostop" in script_content

        # Log validation results
        logger.info(f"Script validation - Docker: {has_docker}, CloudyPad: {has_cloudypad}, AutoStop: {has_autostop}")

        # Return True if all requirements found, False otherwise
        all_requirements_met = has_docker and has_cloudypad and has_autostop
        if all_requirements_met:
            logger.info("Startup script validation passed - all requirements found")
        else:
            logger.warning("Startup script validation failed - missing requirements")

        return all_requirements_met