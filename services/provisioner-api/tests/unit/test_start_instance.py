import pytest
from unittest.mock import patch, MagicMock
from fastapi import HTTPException, BackgroundTasks
from app.routers.gaming import start_instance
from app.models.vm import CloudProvider, VMStatus

def test_start_instance_success_tensordock():
    """Test start_instance successfully starts TensorDock VM"""
    # Mock instance from database
    mock_instance = {
        'vm_id': 'test-vm-123',
        'provider': CloudProvider.TENSORDOCK,
        'provider_instance_id': 'td-instance-456',
        'status': VMStatus.STOPPED
    }

    # Mock background tasks
    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_instance') as mock_get_instance, \
         patch('app.routers.gaming.set_instance_status') as mock_set_status, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service:

        # Setup mocks
        mock_get_instance.return_value = mock_instance

        # Call function
        result = start_instance(vm_id='test-vm-123', background_tasks=background_tasks)

        # Verify database calls
        mock_get_instance.assert_called_once_with('test-vm-123')
        mock_set_status.assert_called_once_with('test-vm-123', VMStatus.STARTING)

        # Verify background task was added for TensorDock
        background_tasks.add_task.assert_called_once_with(
            mock_tensordock_service.start_vm,
            'td-instance-456',
            'test-vm-123'
        )

        # Verify return response
        assert result == {'status': VMStatus.STARTING, 'vm_id': 'test-vm-123'}

def test_start_instance_success_gcp():
    """Test start_instance successfully starts GCP VM"""
    # Mock instance from database
    mock_instance = {
        'vm_id': 'gcp-vm-789',
        'provider': CloudProvider.GCP,
        'provider_instance_id': 'us-central1-a/gcp-instance-name',
        'status': VMStatus.STOPPED
    }

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_instance') as mock_get_instance, \
         patch('app.routers.gaming.set_instance_status') as mock_set_status, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service:

        # Setup mocks
        mock_get_instance.return_value = mock_instance

        # Call function
        result = start_instance(vm_id='gcp-vm-789', background_tasks=background_tasks)

        # Verify database calls
        mock_get_instance.assert_called_once_with('gcp-vm-789')
        mock_set_status.assert_called_once_with('gcp-vm-789', VMStatus.STARTING)

        # Verify background task was added for GCP
        background_tasks.add_task.assert_called_once_with(
            mock_gcp_service.start_vm,
            'us-central1-a/gcp-instance-name',
            'gcp-vm-789'
        )

        # Verify return response
        assert result == {'status': VMStatus.STARTING, 'vm_id': 'gcp-vm-789'}

def test_start_instance_vm_not_found():
    """Test start_instance raises 404 when VM not found"""
    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_instance') as mock_get_instance:
        mock_get_instance.return_value = None

        with pytest.raises(HTTPException) as exc_info:
            start_instance(vm_id='non-existent-vm', background_tasks=background_tasks)

        assert exc_info.value.status_code == 404
        assert exc_info.value.detail == "VM instance not found"

        # Verify no background tasks were added
        background_tasks.add_task.assert_not_called()

def test_start_instance_unsupported_provider():
    """Test start_instance raises 400 for unsupported provider"""
    # Mock instance with unsupported provider
    mock_instance = {
        'vm_id': 'aws-vm-123',
        'provider': CloudProvider.AWS,  # Unsupported provider
        'provider_instance_id': 'i-1234567890abcdef0',
        'status': VMStatus.STOPPED
    }

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_instance') as mock_get_instance, \
         patch('app.routers.gaming.set_instance_status') as mock_set_status:

        # Setup mocks
        mock_get_instance.return_value = mock_instance

        with pytest.raises(HTTPException) as exc_info:
            start_instance(vm_id='aws-vm-123', background_tasks=background_tasks)

        # Verify status was still updated to STARTING before error
        mock_set_status.assert_called_once_with('aws-vm-123', VMStatus.STARTING)

        assert exc_info.value.status_code == 400
        assert exc_info.value.detail == "Provider aws not supported for start operation"

        # Verify no background tasks were added
        background_tasks.add_task.assert_not_called()

def test_start_instance_workflow_order():
    """Test start_instance performs operations in correct order"""
    # Mock instance
    mock_instance = {
        'vm_id': 'test-vm-123',
        'provider': CloudProvider.TENSORDOCK,
        'provider_instance_id': 'td-instance-456',
        'status': VMStatus.STOPPED
    }

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_instance') as mock_get_instance, \
         patch('app.routers.gaming.set_instance_status') as mock_set_status, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service:

        mock_get_instance.return_value = mock_instance

        # Create a call order tracker
        call_order = []

        def track_get_instance(*args):
            call_order.append('get_instance')
            return mock_instance

        def track_set_status(*args):
            call_order.append('set_status')

        def track_add_task(*args):
            call_order.append('add_task')

        mock_get_instance.side_effect = track_get_instance
        mock_set_status.side_effect = track_set_status
        background_tasks.add_task.side_effect = track_add_task

        # Call function
        start_instance(vm_id='test-vm-123', background_tasks=background_tasks)

        # Verify correct order: get instance → set status → add background task
        assert call_order == ['get_instance', 'set_status', 'add_task']

def test_start_instance_all_supported_providers():
    """Test start_instance works with all currently supported providers"""
    background_tasks = MagicMock(spec=BackgroundTasks)

    # Test TensorDock
    tensordock_instance = {
        'vm_id': 'td-vm',
        'provider': CloudProvider.TENSORDOCK,
        'provider_instance_id': 'td-123',
        'status': VMStatus.STOPPED
    }

    # Test GCP
    gcp_instance = {
        'vm_id': 'gcp-vm',
        'provider': CloudProvider.GCP,
        'provider_instance_id': 'zone/instance',
        'status': VMStatus.STOPPED
    }

    test_cases = [
        (tensordock_instance, 'tensordock_service'),
        (gcp_instance, 'gcp_service')
    ]

    for instance, service_name in test_cases:
        background_tasks.reset_mock()

        with patch('app.routers.gaming.get_instance') as mock_get_instance, \
             patch('app.routers.gaming.set_instance_status') as mock_set_status, \
             patch(f'app.routers.gaming.{service_name}') as mock_service:

            mock_get_instance.return_value = instance

            result = start_instance(vm_id=instance['vm_id'], background_tasks=background_tasks)

            # Verify each provider calls the correct service
            background_tasks.add_task.assert_called_once()
            args = background_tasks.add_task.call_args[0]
            assert args[0] == mock_service.start_vm
            assert args[1] == instance['provider_instance_id']
            assert args[2] == instance['vm_id']

            # Verify return format is consistent
            assert result == {'status': VMStatus.STARTING, 'vm_id': instance['vm_id']}