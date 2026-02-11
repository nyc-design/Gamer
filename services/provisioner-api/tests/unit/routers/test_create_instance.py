import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from fastapi.testclient import TestClient
from app.routers.gaming import router as gaming_router
from app.models.vm import VMCreateRequest, CloudProvider, ConsoleType, OperatingSystems, GPUTypes


class TestCreateInstanceFunction:
    """Test class for gaming router create_instance function implementation."""

    @pytest.mark.asyncio
    async def test_create_instance_gcp_success(self):
        """Test successful GCP instance creation."""
        create_request = MagicMock()
        create_request.console_type = ConsoleType.SNES
        create_request.provider = CloudProvider.GCP
        create_request.provider_id = "us-central1-a"
        create_request.instance_name = "test-gaming-vm"
        create_request.instance_type = "n1-standard-4"
        create_request.operating_system = OperatingSystems.Ubuntu
        create_request.gpu = GPUTypes.RTX3060
        create_request.num_cpus = 4
        create_request.num_ram = 8
        create_request.num_disk = 100

        with patch('app.routers.gaming.get_console_config') as mock_get_config, \
             patch('app.routers.gaming.GCPComputeService') as mock_gcp_service, \
             patch('app.routers.gaming.add_new_instance') as mock_add_instance:

            # Mock console config
            mock_config = MagicMock()
            mock_config.min_cpus = 2
            mock_config.min_ram = 4
            mock_config.min_disk = 50
            mock_get_config.return_value = mock_config

            # Mock GCP service
            mock_service_instance = MagicMock()
            mock_service_instance.create_vm = AsyncMock()
            mock_gcp_service.return_value = mock_service_instance

            # Mock database
            mock_add_instance.return_value = "vm-123"

            from app.routers.gaming import create_instance

            result = await create_instance(create_request)

            # Verify console config was retrieved
            mock_get_config.assert_called_once_with(ConsoleType.SNES)

            # Verify VM creation was called
            mock_service_instance.create_vm.assert_called_once()

            # Verify instance was added to database
            mock_add_instance.assert_called_once()

            # Verify response
            assert result.vm_id == "vm-123"
            assert result.status == "creating"
            assert result.provider == CloudProvider.GCP

    @pytest.mark.asyncio
    async def test_create_instance_tensordock_success(self):
        """Test successful TensorDock instance creation."""
        create_request = MagicMock()
        create_request.console_type = ConsoleType.GAMECUBE
        create_request.provider = CloudProvider.TENSORDOCK
        create_request.provider_id = "us-east"
        create_request.gpu = GPUTypes.RTX4090

        with patch('app.routers.gaming.get_console_config') as mock_get_config, \
             patch('app.routers.gaming.TensorDockService') as mock_td_service, \
             patch('app.routers.gaming.add_new_instance') as mock_add_instance:

            mock_config = MagicMock()
            mock_get_config.return_value = mock_config

            mock_service_instance = MagicMock()
            mock_service_instance.create_vm = AsyncMock()
            mock_td_service.return_value = mock_service_instance

            mock_add_instance.return_value = "vm-456"

            from app.routers.gaming import create_instance

            result = await create_instance(create_request)

            mock_service_instance.create_vm.assert_called_once()
            assert result.vm_id == "vm-456"
            assert result.provider == CloudProvider.TENSORDOCK

    @pytest.mark.asyncio
    async def test_create_instance_insufficient_resources(self):
        """Test instance creation with insufficient resources."""
        create_request = MagicMock()
        create_request.console_type = ConsoleType.SWITCH
        create_request.num_cpus = 2
        create_request.num_ram = 4

        with patch('app.routers.gaming.get_console_config') as mock_get_config:

            mock_config = MagicMock()
            mock_config.min_cpus = 8  # Requires more than provided
            mock_config.min_ram = 16  # Requires more than provided
            mock_get_config.return_value = mock_config

            from app.routers.gaming import create_instance

            with pytest.raises(Exception, match="Insufficient resources"):
                await create_instance(create_request)

    @pytest.mark.asyncio
    async def test_create_instance_unsupported_provider(self):
        """Test instance creation with unsupported provider."""
        create_request = MagicMock()
        create_request.provider = "unsupported_provider"

        with patch('app.routers.gaming.get_console_config') as mock_get_config:
            mock_get_config.return_value = MagicMock()

            from app.routers.gaming import create_instance

            with pytest.raises(Exception, match="Unsupported provider"):
                await create_instance(create_request)