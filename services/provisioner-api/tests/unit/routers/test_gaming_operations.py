import pytest
from unittest.mock import MagicMock, AsyncMock, patch
from app.models.vm import VMStatus, CloudProvider


class TestGamingOperationsFunction:
    """Test class for gaming router start/stop/destroy/billing functions."""

    @pytest.mark.asyncio
    async def test_start_instance_gcp_success(self):
        """Test successful GCP instance start."""
        vm_id = "test-vm-123"

        with patch('app.routers.gaming.get_instance') as mock_get_instance, \
             patch('app.routers.gaming.GCPComputeService') as mock_gcp_service:

            # Mock instance document
            mock_instance = MagicMock()
            mock_instance.provider = CloudProvider.GCP
            mock_instance.provider_instance_id = "us-central1-a/test-instance"
            mock_get_instance.return_value = mock_instance

            # Mock GCP service
            mock_service_instance = MagicMock()
            mock_service_instance.start_vm = AsyncMock()
            mock_gcp_service.return_value = mock_service_instance

            from app.routers.gaming import start_instance

            result = await start_instance(vm_id)

            # Verify service was called
            mock_service_instance.start_vm.assert_called_once_with(
                "us-central1-a/test-instance", vm_id
            )

            assert result.vm_id == vm_id

    @pytest.mark.asyncio
    async def test_stop_instance_tensordock_success(self):
        """Test successful TensorDock instance stop."""
        vm_id = "test-vm-456"

        with patch('app.routers.gaming.get_instance') as mock_get_instance, \
             patch('app.routers.gaming.TensorDockService') as mock_td_service:

            mock_instance = MagicMock()
            mock_instance.provider = CloudProvider.TENSORDOCK
            mock_instance.provider_instance_id = "td-instance-123"
            mock_get_instance.return_value = mock_instance

            mock_service_instance = MagicMock()
            mock_service_instance.stop_vm = AsyncMock()
            mock_td_service.return_value = mock_service_instance

            from app.routers.gaming import stop_instance

            result = await stop_instance(vm_id)

            mock_service_instance.stop_vm.assert_called_once_with("td-instance-123", vm_id)
            assert result.vm_id == vm_id

    @pytest.mark.asyncio
    async def test_destroy_instance_success(self):
        """Test successful instance destruction."""
        vm_id = "test-vm-789"

        with patch('app.routers.gaming.get_instance') as mock_get_instance, \
             patch('app.routers.gaming.GCPComputeService') as mock_gcp_service:

            mock_instance = MagicMock()
            mock_instance.provider = CloudProvider.GCP
            mock_instance.provider_instance_id = "us-west1-b/gaming-vm"
            mock_get_instance.return_value = mock_instance

            mock_service_instance = MagicMock()
            mock_service_instance.destroy_vm = AsyncMock()
            mock_gcp_service.return_value = mock_service_instance

            from app.routers.gaming import destroy_instance

            result = await destroy_instance(vm_id)

            mock_service_instance.destroy_vm.assert_called_once_with("us-west1-b/gaming-vm", vm_id)
            assert result.vm_id == vm_id

    @pytest.mark.asyncio
    async def test_get_billing_success(self):
        """Test successful billing information retrieval."""
        user_id = "user-123"

        with patch('app.routers.gaming.list_instances_by_user') as mock_list_instances:

            mock_instance1 = MagicMock()
            mock_instance1.hourly_price = 0.50
            mock_instance1.status = VMStatus.RUNNING

            mock_instance2 = MagicMock()
            mock_instance2.hourly_price = 0.30
            mock_instance2.status = VMStatus.STOPPED

            mock_list_instances.return_value = [mock_instance1, mock_instance2]

            from app.routers.gaming import get_billing

            result = await get_billing(user_id)

            assert result.total_hourly_cost == 0.80
            assert result.active_instances == 1