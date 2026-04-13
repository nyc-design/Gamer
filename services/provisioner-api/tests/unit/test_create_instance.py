import pytest
from unittest.mock import patch, MagicMock, Mock
from fastapi import HTTPException, BackgroundTasks
from app.routers.gaming import create_instance
from app.models.vm import (
    ConsoleType, VMCreateRequest, CloudProvider, OperatingSystems,
    GPUTypes, VMStatus, VMResponse, VMDocument
)
from decimal import Decimal

def test_create_instance_success_tensordock():
    """Test create_instance successfully creates TensorDock VM"""
    # Mock console config
    mock_console_config = MagicMock()
    mock_console_config.min_cpus = 2
    mock_console_config.min_ram = 4
    mock_console_config.min_disk = 100

    # Mock create request
    create_request = VMCreateRequest(
        console_type=ConsoleType.NES,
        provider=CloudProvider.TENSORDOCK,
        provider_id="test-location",
        instance_name="test-vm",
        hourly_price=Decimal("0.50"),
        instance_lat=37.7749,
        instance_long=-122.4194,
        operating_system=OperatingSystems.Ubuntu,
        gpu=GPUTypes.NoGPU,
        num_cpus=None,  # Should use console config default
        num_ram=None,   # Should use console config default
        num_disk=None,  # Should use console config default
        auto_stop_timeout=9000,
        user_id="test-user"
    )

    # Mock background tasks
    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.add_new_instance') as mock_add_new_instance, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.uuid.uuid4') as mock_uuid, \
         patch('app.routers.gaming.secrets.choice') as mock_choice, \
         patch('app.routers.gaming.rsa.generate_private_key') as mock_rsa:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_uuid.return_value = Mock()
        mock_uuid.return_value.__str__ = Mock(return_value="test-vm-id")

        # Mock password generation
        mock_choice.side_effect = lambda x: 'a'  # Simple mock for password generation

        # Mock SSH key generation
        mock_private_key = MagicMock()
        mock_private_key.private_bytes.return_value.decode.return_value = "test-ssh-key"
        mock_rsa.return_value = mock_private_key

        result = create_instance(
            console_type=ConsoleType.NES,
            create_request=create_request,
            user_id="test-user",
            background_tasks=background_tasks
        )

        # Verify console config lookup
        mock_get_console_config.assert_called_once_with(ConsoleType.NES)

        # Verify VM document creation with defaults applied
        mock_add_new_instance.assert_called_once()
        vm_doc_call = mock_add_new_instance.call_args[0][0]
        assert vm_doc_call.vm_id == "test-vm-id"
        assert vm_doc_call.status == VMStatus.CREATING
        assert vm_doc_call.num_cpus == 2  # From console config
        assert vm_doc_call.num_ram == 4   # From console config
        assert vm_doc_call.num_disk == 100 # From console config

        # Verify background task was added for TensorDock
        background_tasks.add_task.assert_called_once()
        task_call = background_tasks.add_task.call_args
        assert task_call[0][0] == mock_tensordock_service.create_vm

        # Verify return response
        assert isinstance(result, VMResponse)
        assert result.vm_id == "test-vm-id"
        assert result.status == VMStatus.CREATING
        assert result.console_type == ConsoleType.NES

def test_create_instance_success_gcp():
    """Test create_instance successfully creates GCP VM"""
    # Mock console config
    mock_console_config = MagicMock()
    mock_console_config.min_cpus = 4
    mock_console_config.min_ram = 8
    mock_console_config.min_disk = 200

    # Mock create request for GCP
    create_request = VMCreateRequest(
        console_type=ConsoleType.SWITCH,
        provider=CloudProvider.GCP,
        provider_id="us-central1-a",
        instance_name="test-gcp-vm",
        hourly_price=Decimal("1.20"),
        instance_lat=41.2619,
        instance_long=-95.8608,
        operating_system=OperatingSystems.Ubuntu,
        gpu=GPUTypes.RTX4090,
        num_cpus=8,
        num_ram=16,
        num_disk=500,
        auto_stop_timeout=7200,
        user_id="test-user"
    )

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.add_new_instance') as mock_add_new_instance, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service, \
         patch('app.routers.gaming.uuid.uuid4') as mock_uuid, \
         patch('app.routers.gaming.secrets.choice') as mock_choice, \
         patch('app.routers.gaming.rsa.generate_private_key') as mock_rsa:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_uuid.return_value = Mock()
        mock_uuid.return_value.__str__ = Mock(return_value="gcp-vm-id")
        mock_choice.side_effect = lambda x: 'b'

        mock_private_key = MagicMock()
        mock_private_key.private_bytes.return_value.decode.return_value = "gcp-ssh-key"
        mock_rsa.return_value = mock_private_key

        result = create_instance(
            console_type=ConsoleType.SWITCH,
            create_request=create_request,
            background_tasks=background_tasks
        )

        # Verify GCP service was called
        background_tasks.add_task.assert_called_once()
        task_call = background_tasks.add_task.call_args
        assert task_call[0][0] == mock_gcp_service.create_vm

        # Verify user-provided values were used (not defaults)
        vm_doc_call = mock_add_new_instance.call_args[0][0]
        assert vm_doc_call.num_cpus == 8    # User provided
        assert vm_doc_call.num_ram == 16    # User provided
        assert vm_doc_call.num_disk == 500  # User provided

def test_create_instance_console_config_not_found():
    """Test create_instance raises 404 when console config not found"""
    create_request = VMCreateRequest(
        console_type=ConsoleType.NES,
        provider=CloudProvider.TENSORDOCK,
        provider_id="test-location",
        instance_name="test-vm",
        hourly_price=Decimal("0.50"),
        instance_lat=37.7749,
        instance_long=-122.4194,
        operating_system=OperatingSystems.Ubuntu,
        gpu=GPUTypes.NoGPU
    )

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config:
        mock_get_console_config.return_value = None

        with pytest.raises(HTTPException) as exc_info:
            create_instance(
                console_type=ConsoleType.NES,
                create_request=create_request,
                background_tasks=background_tasks
            )

        assert exc_info.value.status_code == 404
        assert exc_info.value.detail == "Console config not found for nes"

def test_create_instance_unsupported_provider():
    """Test create_instance raises 400 for unsupported provider"""
    mock_console_config = MagicMock()
    mock_console_config.min_cpus = 2
    mock_console_config.min_ram = 4
    mock_console_config.min_disk = 100

    create_request = VMCreateRequest(
        console_type=ConsoleType.NES,
        provider=CloudProvider.AWS,  # Unsupported provider
        provider_id="us-east-1",
        instance_name="test-vm",
        hourly_price=Decimal("0.50"),
        instance_lat=37.7749,
        instance_long=-122.4194,
        operating_system=OperatingSystems.Ubuntu,
        gpu=GPUTypes.NoGPU
    )

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.add_new_instance') as mock_add_new_instance, \
         patch('app.routers.gaming.uuid.uuid4') as mock_uuid, \
         patch('app.routers.gaming.secrets.choice') as mock_choice, \
         patch('app.routers.gaming.rsa.generate_private_key') as mock_rsa:

        mock_get_console_config.return_value = mock_console_config
        mock_uuid.return_value = Mock()
        mock_uuid.return_value.__str__ = Mock(return_value="test-vm-id")
        mock_choice.side_effect = lambda x: 'a'

        mock_private_key = MagicMock()
        mock_private_key.private_bytes.return_value.decode.return_value = "test-ssh-key"
        mock_rsa.return_value = mock_private_key

        with pytest.raises(HTTPException) as exc_info:
            create_instance(
                console_type=ConsoleType.NES,
                create_request=create_request,
                background_tasks=background_tasks
            )

        assert exc_info.value.status_code == 400
        assert exc_info.value.detail == "Provider aws not yet supported"

def test_create_instance_determines_compatible_consoles():
    """Test create_instance determines compatible console types based on resources"""
    # Mock console configs
    nes_config = MagicMock()
    nes_config.min_cpus = 2
    nes_config.min_ram = 4
    nes_config.min_disk = 50

    switch_config = MagicMock()
    switch_config.min_cpus = 8
    switch_config.min_ram = 16
    switch_config.min_disk = 200

    def mock_get_console_config(console_type):
        if console_type == ConsoleType.NES:
            return nes_config
        elif console_type == ConsoleType.SWITCH:
            return switch_config
        return None

    # High-spec request that should support both NES and Switch
    create_request = VMCreateRequest(
        console_type=ConsoleType.NES,  # Primary console
        provider=CloudProvider.TENSORDOCK,
        provider_id="test-location",
        instance_name="test-vm",
        hourly_price=Decimal("2.00"),
        instance_lat=37.7749,
        instance_long=-122.4194,
        operating_system=OperatingSystems.Ubuntu,
        gpu=GPUTypes.RTX4090,
        num_cpus=12,  # More than switch minimum
        num_ram=32,   # More than switch minimum
        num_disk=500  # More than switch minimum
    )

    background_tasks = MagicMock(spec=BackgroundTasks)

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config_patch, \
         patch('app.routers.gaming.add_new_instance') as mock_add_new_instance, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.uuid.uuid4') as mock_uuid, \
         patch('app.routers.gaming.secrets.choice') as mock_choice, \
         patch('app.routers.gaming.rsa.generate_private_key') as mock_rsa, \
         patch('app.routers.gaming.ConsoleType') as mock_console_type_enum:

        mock_get_console_config_patch.side_effect = mock_get_console_config
        mock_console_type_enum.__iter__ = Mock(return_value=iter([ConsoleType.NES, ConsoleType.SWITCH]))

        mock_uuid.return_value = Mock()
        mock_uuid.return_value.__str__ = Mock(return_value="multi-console-vm")
        mock_choice.side_effect = lambda x: 'a'

        mock_private_key = MagicMock()
        mock_private_key.private_bytes.return_value.decode.return_value = "test-ssh-key"
        mock_rsa.return_value = mock_private_key

        result = create_instance(
            console_type=ConsoleType.NES,
            create_request=create_request,
            background_tasks=background_tasks
        )

        # Verify VM document includes compatible console types
        vm_doc_call = mock_add_new_instance.call_args[0][0]
        assert ConsoleType.NES in vm_doc_call.console_types  # Primary console
        assert ConsoleType.SWITCH in vm_doc_call.console_types  # Compatible console