import pytest
from unittest.mock import MagicMock, patch
from app.models.vm import VMStatus


class TestGetInstanceStatusFunction:
    """Test class for gaming router get_instance_status function implementation."""

    @pytest.mark.asyncio
    async def test_get_instance_status_success(self):
        """Test successful instance status retrieval."""
        vm_id = "test-vm-123"

        with patch('app.routers.gaming.get_instance') as mock_get_instance:
            # Mock instance document
            mock_instance = MagicMock()
            mock_instance.vm_id = vm_id
            mock_instance.status = VMStatus.RUNNING
            mock_instance.ip_address = "203.45.67.89"
            mock_instance.last_activity = None
            mock_get_instance.return_value = mock_instance

            from app.routers.gaming import get_instance_status

            result = await get_instance_status(vm_id)

            # Verify database call
            mock_get_instance.assert_called_once_with(vm_id)

            # Verify response
            assert result.vm_id == vm_id
            assert result.status == VMStatus.RUNNING
            assert result.ip_address == "203.45.67.89"

    @pytest.mark.asyncio
    async def test_get_instance_status_not_found(self):
        """Test instance status retrieval for non-existent instance."""
        vm_id = "non-existent-vm"

        with patch('app.routers.gaming.get_instance') as mock_get_instance:
            mock_get_instance.return_value = None

            from app.routers.gaming import get_instance_status

            with pytest.raises(Exception, match="Instance not found"):
                await get_instance_status(vm_id)