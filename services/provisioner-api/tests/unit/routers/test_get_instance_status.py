"""Tests for get_instance_status gaming router endpoint."""

import pytest
from unittest.mock import patch, MagicMock
from fastapi import HTTPException
from fastapi.testclient import TestClient

from app.models.vm import VMStatus, CloudProvider, ConsoleType, OperatingSystems, GPUTypes
from tests.factories import VMDocumentFactory, VMStatusResponseFactory


class TestGetInstanceStatus:
    """Test cases for get_instance_status endpoint."""

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_success(self, test_client: TestClient, mock_database):
        """Test successful instance status retrieval."""
        # Arrange
        vm_id = "test-vm-123"
        mock_instance = VMDocumentFactory.build(
            vm_id=vm_id,
            status=VMStatus.RUNNING,
            ip_address="203.45.67.89",
            console_types=[ConsoleType.SNES],
            provider=CloudProvider.GCP,
            last_activity=None
        )

        mock_database["get_instance"].return_value = mock_instance

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 200
            data = response.json()

            assert data["vm_id"] == vm_id
            assert data["status"] == VMStatus.RUNNING
            assert data["ip_address"] == "203.45.67.89"
            assert data["last_activity"] is None

            # Verify database was called
            mock_database["get_instance"].assert_called_once_with(vm_id)

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_with_last_activity(self, test_client: TestClient, mock_database):
        """Test instance status retrieval with last activity timestamp."""
        # Arrange
        vm_id = "test-vm-456"
        mock_instance = VMDocumentFactory.build(
            vm_id=vm_id,
            status=VMStatus.STOPPED,
            ip_address="203.45.67.90"
        )

        mock_database["get_instance"].return_value = mock_instance

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 200
            data = response.json()

            assert data["vm_id"] == vm_id
            assert data["status"] == VMStatus.STOPPED
            assert data["ip_address"] == "203.45.67.90"
            assert "last_activity" in data

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_not_found(self, test_client: TestClient, mock_database):
        """Test instance status retrieval for non-existent instance."""
        # Arrange
        vm_id = "non-existent-vm"
        mock_database["get_instance"].return_value = None

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 404
            data = response.json()
            assert "Instance not found" in data["detail"]

            # Verify database was called
            mock_database["get_instance"].assert_called_once_with(vm_id)

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_various_statuses(self, test_client: TestClient, mock_database):
        """Test instance status retrieval for different VM statuses."""
        test_cases = [
            (VMStatus.CREATING, "creating"),
            (VMStatus.CONFIGURING, "configuring"),
            (VMStatus.STARTING, "starting"),
            (VMStatus.RUNNING, "running"),
            (VMStatus.STOPPING, "stopping"),
            (VMStatus.STOPPED, "stopped"),
            (VMStatus.DESTROYING, "destroying"),
            (VMStatus.DESTROYED, "destroyed"),
            (VMStatus.ERROR, "error"),
        ]

        for status, expected_status in test_cases:
            with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
                # Arrange
                vm_id = f"test-vm-{status.value}"
                mock_instance = VMDocumentFactory.build(
                    vm_id=vm_id,
                    status=status,
                    ip_address="203.45.67.100"
                )

                mock_database["get_instance"].return_value = mock_instance

                # Act
                response = test_client.get(f"/api/instances/{vm_id}/status")

                # Assert
                assert response.status_code == 200
                data = response.json()
                assert data["status"] == expected_status

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_invalid_vm_id_format(self, test_client: TestClient):
        """Test instance status retrieval with invalid VM ID format."""
        # Act
        response = test_client.get("/api/instances//status")  # Empty VM ID

        # Assert
        assert response.status_code == 422  # Validation error

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_database_error(self, test_client: TestClient, mock_database):
        """Test instance status retrieval when database throws an error."""
        # Arrange
        vm_id = "test-vm-error"
        mock_database["get_instance"].side_effect = Exception("Database connection error")

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 500

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_response_structure(self, test_client: TestClient, mock_database):
        """Test that response matches VMStatusResponse model structure."""
        # Arrange
        vm_id = "test-vm-structure"
        mock_instance = VMDocumentFactory.build(
            vm_id=vm_id,
            status=VMStatus.RUNNING,
            ip_address="203.45.67.200"
        )

        mock_database["get_instance"].return_value = mock_instance

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 200
            data = response.json()

            # Verify all required fields are present
            required_fields = ["vm_id", "status"]
            for field in required_fields:
                assert field in data

            # Verify optional fields
            optional_fields = ["ip_address", "last_activity"]
            for field in optional_fields:
                assert field in data  # Should be present even if None

    @pytest.mark.unit
    @pytest.mark.api
    def test_get_instance_status_no_ip_address(self, test_client: TestClient, mock_database):
        """Test instance status when VM has no IP address assigned."""
        # Arrange
        vm_id = "test-vm-no-ip"
        mock_instance = VMDocumentFactory.build(
            vm_id=vm_id,
            status=VMStatus.CREATING,
            ip_address=""  # No IP assigned yet
        )

        mock_database["get_instance"].return_value = mock_instance

        with patch("app.routers.gaming.get_instance", mock_database["get_instance"]):
            # Act
            response = test_client.get(f"/api/instances/{vm_id}/status")

            # Assert
            assert response.status_code == 200
            data = response.json()

            assert data["vm_id"] == vm_id
            assert data["status"] == VMStatus.CREATING
            # IP address should be empty string or None, both acceptable
            assert data["ip_address"] == "" or data["ip_address"] is None