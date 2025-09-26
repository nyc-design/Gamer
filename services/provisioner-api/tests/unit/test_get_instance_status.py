import pytest
from unittest.mock import patch, MagicMock
from fastapi import HTTPException
from app.routers.gaming import get_instance_status
from app.models.vm import VMStatusResponse, VMStatus

def test_get_instance_status_success():
    """Test get_instance_status returns correct data for existing VM"""
    # Mock database response
    mock_instance = {
        'vm_id': 'test-vm-123',
        'status': VMStatus.RUNNING,
        'ip_address': '192.168.1.100',
        'last_activity': None
    }

    with patch('app.routers.gaming.get_instance') as mock_get_instance:
        mock_get_instance.return_value = mock_instance

        result = get_instance_status('test-vm-123')

        assert isinstance(result, VMStatusResponse)
        assert result.vm_id == 'test-vm-123'
        assert result.status == VMStatus.RUNNING
        assert result.ip_address == '192.168.1.100'
        assert result.last_activity is None
        mock_get_instance.assert_called_once_with('test-vm-123')

def test_get_instance_status_not_found():
    """Test get_instance_status raises 404 for non-existent VM"""
    with patch('app.routers.gaming.get_instance') as mock_get_instance:
        mock_get_instance.return_value = None

        with pytest.raises(HTTPException) as exc_info:
            get_instance_status('non-existent-vm')

        assert exc_info.value.status_code == 404
        assert exc_info.value.detail == "VM instance not found"
        mock_get_instance.assert_called_once_with('non-existent-vm')

def test_get_instance_status_with_activity():
    """Test get_instance_status with last_activity field"""
    from datetime import datetime
    activity_time = datetime.now()

    mock_instance = {
        'vm_id': 'test-vm-456',
        'status': VMStatus.STOPPED,
        'ip_address': None,
        'last_activity': activity_time
    }

    with patch('app.routers.gaming.get_instance') as mock_get_instance:
        mock_get_instance.return_value = mock_instance

        result = get_instance_status('test-vm-456')

        assert result.vm_id == 'test-vm-456'
        assert result.status == VMStatus.STOPPED
        assert result.ip_address is None
        assert result.last_activity == activity_time