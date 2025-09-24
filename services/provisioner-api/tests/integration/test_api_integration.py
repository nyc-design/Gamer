"""Integration tests using testcontainers and real HTTP calls."""

import pytest
from testcontainers.mongodb import MongoDbContainer
from httpx import AsyncClient
import respx
from unittest.mock import patch

from app.main import app
from tests.factories import VMCreateRequestFactory, ConsoleConfigFactory


class TestAPIIntegration:
    """Integration tests for the gaming API with real infrastructure."""

    @pytest.mark.integration
    @pytest.mark.slow
    async def test_create_instance_with_external_api_mocking(self):
        """Test instance creation with external API calls mocked."""

        # Mock external HTTP calls using respx
        with respx.mock() as httpx_mock:
            # Mock TensorDock API calls
            httpx_mock.get(
                "https://dashboard.tensordock.com/api/v2/locations"
            ).mock(return_value=httpx.Response(200, json={
                "data": {
                    "locations": [{
                        "id": "us-east",
                        "city": "New York",
                        "country": "USA",
                        "gpus": [{
                            "v0Name": "RTX3060",
                            "displayName": "RTX 3060",
                            "network_features": {"dedicated_ip_available": True},
                            "resources": {"max_vcpus": 8, "max_ram_gb": 16, "max_storage_gb": 500},
                            "pricing": {"per_vcpu_hr": 0.01, "per_gb_ram_hr": 0.002}
                        }]
                    }]
                }
            }))

            # Mock Google Cloud geocoding API
            httpx_mock.get(
                url__regex=r".*maps\.googleapis\.com.*"
            ).mock(return_value=httpx.Response(200, json={
                "results": [{
                    "geometry": {
                        "location": {"lat": 40.7128, "lng": -74.0060}
                    }
                }]
            }))

            async with AsyncClient(app=app, base_url="http://test") as client:
                # Create test data
                create_request = VMCreateRequestFactory.build()
                console_config = ConsoleConfigFactory.build()

                with patch("app.routers.gaming.get_console_config", return_value=console_config), \
                     patch("app.routers.gaming.add_new_instance", return_value="test-vm-123"):

                    # Act
                    response = await client.post("/api/instances", json=create_request.dict())

                    # Assert
                    assert response.status_code == 201
                    data = response.json()
                    assert data["vm_id"] == "test-vm-123"
                    assert data["status"] == "creating"

    @pytest.mark.integration
    @pytest.mark.slow
    @pytest.mark.database
    def test_with_real_mongodb(self):
        """Test with real MongoDB using testcontainers."""

        # Start a real MongoDB container
        with MongoDbContainer("mongo:7.0") as mongo:
            connection_string = mongo.get_connection_url()

            # Configure app to use the test MongoDB
            with patch("app.core.database.get_database") as mock_get_db:
                from pymongo import MongoClient

                test_client = MongoClient(connection_string)
                test_db = test_client["test_provisioner"]
                mock_get_db.return_value = test_db

                # Test database operations
                from app.core.database import add_new_instance, get_instance
                from tests.factories import VMDocumentFactory

                # Create test instance
                test_vm = VMDocumentFactory.build()
                vm_id = add_new_instance(test_vm)

                # Retrieve and verify
                retrieved_vm = get_instance(vm_id)
                assert retrieved_vm is not None
                assert retrieved_vm.vm_id == vm_id

    @pytest.mark.integration
    @pytest.mark.external
    async def test_real_geocoding_service(self):
        """Test with real geocoding service (requires network)."""
        from app.services.geocoding_service import GeocodingService

        geocoding = GeocodingService()

        # Test real geocoding call
        coordinates = await geocoding.get_coordinates("San Francisco", "USA")

        assert coordinates is not None
        assert len(coordinates) == 2
        assert isinstance(coordinates[0], float)  # latitude
        assert isinstance(coordinates[1], float)  # longitude

        # Verify reasonable coordinates for San Francisco
        lat, lng = coordinates
        assert 37.5 < lat < 38.0  # SF is around 37.7749
        assert -123.0 < lng < -122.0  # SF is around -122.4194