import pytest
from httpx import AsyncClient
from fastapi.testclient import TestClient
from app.main import app

@pytest.fixture
def client():
    """Sync test client for basic API testing"""
    return TestClient(app)

@pytest.fixture
async def async_client():
    """Async test client for async endpoint testing"""
    async with AsyncClient(app=app, base_url="http://test") as ac:
        yield ac