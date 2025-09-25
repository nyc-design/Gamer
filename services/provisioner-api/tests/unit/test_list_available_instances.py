import pytest
from unittest.mock import patch, AsyncMock, MagicMock
from fastapi import HTTPException
from app.routers.gaming import list_available_instances
from app.models.vm import (
    ConsoleType, VMAvailableResponse, CloudProvider, GPUTypes
)
from decimal import Decimal

@pytest.mark.asyncio
async def test_list_available_instances_success():
    """Test list_available_instances successfully combines TensorDock and GCP results"""
    # Mock console config
    mock_console_config = MagicMock()
    mock_console_config.min_cpus = 4
    mock_console_config.min_ram = 8
    mock_console_config.min_disk = 200

    # Mock TensorDock instances
    tensordock_instances = [
        VMAvailableResponse(
            provider=CloudProvider.TENSORDOCK,
            provider_id="td-location-1",
            hourly_price=Decimal("0.75"),
            gpu=GPUTypes.RTX4090,
            avail_cpus=8,
            avail_ram=16,
            avail_disk=500,
            instance_lat=40.7128,
            instance_long=-74.0060,
            distance_to_user=0.0
        ),
        VMAvailableResponse(
            provider=CloudProvider.TENSORDOCK,
            provider_id="td-location-2",
            hourly_price=Decimal("1.20"),
            gpu=GPUTypes.RTX3090,
            avail_cpus=6,
            avail_ram=12,
            avail_disk=400,
            instance_lat=34.0522,
            instance_long=-118.2437,
            distance_to_user=0.0
        )
    ]

    # Mock GCP instances
    gcp_instances = [
        VMAvailableResponse(
            provider=CloudProvider.GCP,
            provider_id="us-central1",
            hourly_price=Decimal("0.50"),
            gpu=GPUTypes.NoGPU,
            avail_cpus=4,
            avail_ram=8,
            avail_disk=200,
            instance_lat=41.2619,
            instance_long=-95.8608,
            distance_to_user=0.0
        )
    ]

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_tensordock_service.list_available_hostnodes = AsyncMock(return_value=tensordock_instances)
        mock_gcp_service.list_available_regions = AsyncMock(return_value=gcp_instances)

        # Call function
        result = await list_available_instances(console_type=ConsoleType.SWITCH)

        # Verify console config lookup
        mock_get_console_config.assert_called_once_with(ConsoleType.SWITCH)

        # Verify service calls
        mock_tensordock_service.list_available_hostnodes.assert_called_once_with(mock_console_config, None)
        mock_gcp_service.list_available_regions.assert_called_once_with(mock_console_config, None)

        # Verify combined results
        assert len(result) == 3
        assert len([x for x in result if x.provider == CloudProvider.TENSORDOCK]) == 2
        assert len([x for x in result if x.provider == CloudProvider.GCP]) == 1

        # Verify all instances are VMAvailableResponse
        for instance in result:
            assert isinstance(instance, VMAvailableResponse)

@pytest.mark.asyncio
async def test_list_available_instances_with_user_location():
    """Test list_available_instances calculates distances and sorts by distance"""
    # Mock console config
    mock_console_config = MagicMock()

    # Mock instances with different locations
    mock_instances = [
        VMAvailableResponse(
            provider=CloudProvider.TENSORDOCK,
            provider_id="td-far",
            hourly_price=Decimal("0.50"),
            gpu=GPUTypes.NoGPU,
            avail_cpus=4,
            avail_ram=8,
            avail_disk=200,
            instance_lat=0.0,  # Far from user
            instance_long=0.0,
            distance_to_user=0.0
        ),
        VMAvailableResponse(
            provider=CloudProvider.GCP,
            provider_id="gcp-close",
            hourly_price=Decimal("0.75"),
            gpu=GPUTypes.NoGPU,
            avail_cpus=4,
            avail_ram=8,
            avail_disk=200,
            instance_lat=37.7749,  # Close to user
            instance_long=-122.4194,
            distance_to_user=0.0
        )
    ]

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service, \
         patch('app.routers.gaming.geocoding_service') as mock_geocoding_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_tensordock_service.list_available_hostnodes = AsyncMock(return_value=[mock_instances[0]])
        mock_gcp_service.list_available_regions = AsyncMock(return_value=[mock_instances[1]])

        # Mock distance calculations
        def mock_calculate_distance(user_lat, user_lng, instance_lat, instance_lng):
            if instance_lat == 0.0:  # Far instance
                return 5000.0
            else:  # Close instance
                return 50.0

        mock_geocoding_service.calculate_distance = MagicMock(side_effect=mock_calculate_distance)

        # Call function with user location
        user_lat, user_lng = 37.7749, -122.4194  # San Francisco
        result = await list_available_instances(
            console_type=ConsoleType.NES,
            user_lat=user_lat,
            user_lng=user_lng
        )

        # Verify distance calculations were called
        assert mock_geocoding_service.calculate_distance.call_count == 2

        # Verify instances are sorted by distance (closest first)
        assert len(result) == 2
        assert result[0].distance_to_user == 50.0    # Close instance first
        assert result[1].distance_to_user == 5000.0  # Far instance second
        assert result[0].provider == CloudProvider.GCP    # Close GCP instance first
        assert result[1].provider == CloudProvider.TENSORDOCK  # Far TensorDock instance second

@pytest.mark.asyncio
async def test_list_available_instances_without_user_location():
    """Test list_available_instances without user location doesn't calculate distances"""
    # Mock console config
    mock_console_config = MagicMock()

    # Mock instances
    mock_instances = [
        VMAvailableResponse(
            provider=CloudProvider.TENSORDOCK,
            provider_id="td-1",
            hourly_price=Decimal("0.50"),
            gpu=GPUTypes.NoGPU,
            avail_cpus=4,
            avail_ram=8,
            avail_disk=200,
            instance_lat=40.7128,
            instance_long=-74.0060,
            distance_to_user=0.0
        )
    ]

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service, \
         patch('app.routers.gaming.geocoding_service') as mock_geocoding_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_tensordock_service.list_available_hostnodes = AsyncMock(return_value=mock_instances)
        mock_gcp_service.list_available_regions = AsyncMock(return_value=[])

        # Call function without user location
        result = await list_available_instances(console_type=ConsoleType.NES)

        # Verify distance calculation was not called
        mock_geocoding_service.calculate_distance.assert_not_called()

        # Verify instances returned without distance sorting
        assert len(result) == 1
        assert result[0].distance_to_user == 0.0  # Default value unchanged

@pytest.mark.asyncio
async def test_list_available_instances_console_config_not_found():
    """Test list_available_instances raises 404 when console config not found"""
    with patch('app.routers.gaming.get_console_config') as mock_get_console_config:
        mock_get_console_config.return_value = None

        with pytest.raises(HTTPException) as exc_info:
            await list_available_instances(console_type=ConsoleType.NES)

        assert exc_info.value.status_code == 404
        assert exc_info.value.detail == "Console config not found for nes"

@pytest.mark.asyncio
async def test_list_available_instances_empty_results():
    """Test list_available_instances handles empty results from both services"""
    # Mock console config
    mock_console_config = MagicMock()

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_tensordock_service.list_available_hostnodes = AsyncMock(return_value=[])
        mock_gcp_service.list_available_regions = AsyncMock(return_value=[])

        # Call function
        result = await list_available_instances(console_type=ConsoleType.GAMECUBE)

        # Verify empty results
        assert len(result) == 0
        assert isinstance(result, list)

@pytest.mark.asyncio
async def test_list_available_instances_service_error_handling():
    """Test list_available_instances handles service errors gracefully"""
    # Mock console config
    mock_console_config = MagicMock()

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config

        # Mock TensorDock service to raise an exception
        mock_tensordock_service.list_available_hostnodes = AsyncMock(
            side_effect=Exception("TensorDock API Error")
        )
        mock_gcp_service.list_available_regions = AsyncMock(return_value=[])

        # Verify that the exception propagates (as expected for service errors)
        with pytest.raises(Exception) as exc_info:
            await list_available_instances(console_type=ConsoleType.NES)

        assert str(exc_info.value) == "TensorDock API Error"

@pytest.mark.asyncio
async def test_list_available_instances_partial_user_location():
    """Test list_available_instances handles partial user location correctly"""
    # Mock console config
    mock_console_config = MagicMock()

    mock_instances = [
        VMAvailableResponse(
            provider=CloudProvider.TENSORDOCK,
            provider_id="td-1",
            hourly_price=Decimal("0.50"),
            gpu=GPUTypes.NoGPU,
            avail_cpus=4,
            avail_ram=8,
            avail_disk=200,
            instance_lat=40.7128,
            instance_long=-74.0060,
            distance_to_user=0.0
        )
    ]

    with patch('app.routers.gaming.get_console_config') as mock_get_console_config, \
         patch('app.routers.gaming.tensordock_service') as mock_tensordock_service, \
         patch('app.routers.gaming.gcp_service') as mock_gcp_service, \
         patch('app.routers.gaming.geocoding_service') as mock_geocoding_service:

        # Setup mocks
        mock_get_console_config.return_value = mock_console_config
        mock_tensordock_service.list_available_hostnodes = AsyncMock(return_value=mock_instances)
        mock_gcp_service.list_available_regions = AsyncMock(return_value=[])

        # Call function with only latitude (partial location)
        result = await list_available_instances(
            console_type=ConsoleType.NES,
            user_lat=37.7749,
            user_lng=None  # Missing longitude
        )

        # Verify distance calculation was not called (requires both lat and lng)
        mock_geocoding_service.calculate_distance.assert_not_called()

        # Verify function still works
        assert len(result) == 1
        assert result[0].distance_to_user == 0.0