import pytest
from fastapi.testclient import TestClient
from app.main import app

@pytest.fixture
def client():
    """Simple test client for API testing"""
    return TestClient(app)
