# Modern Testing Infrastructure for Gaming VM Provisioner API

This document outlines the comprehensive testing setup using modern Python testing practices with Poetry, FastAPI TestClient, Pydantic factories, and advanced testing tools.

## 🏗️ Testing Architecture

### Environment Separation with Poetry Groups

```toml
[tool.poetry.dependencies]           # Production only
[tool.poetry.group.dev.dependencies] # Development tools (linting, formatting)
[tool.poetry.group.test.dependencies] # Core testing tools
[tool.poetry.group.integration.dependencies] # Heavy integration tests (optional)
[tool.poetry.group.performance.dependencies] # Performance testing (optional)
[tool.poetry.group.reporting.dependencies]   # Advanced reporting (optional)
```

### Installation Commands
```bash
# Production (minimal)
poetry install --only main

# Development (code + dev tools)
poetry install --with dev --without test

# Testing (code + test tools)
poetry install --with test --without dev

# Full development (everything)
poetry install

# CI/CD pipeline
poetry install --with test,reporting --without dev
```

## 🧪 Test Types and Tools

### Unit Tests
- **FastAPI TestClient**: Direct endpoint testing without server startup
- **Pydantic Factories**: Realistic model instances with Faker integration
- **pytest-mock**: Clean mocking with automatic cleanup
- **respx**: HTTP request/response mocking for external APIs

### Integration Tests
- **testcontainers**: Real database instances in containers
- **pytest-asyncio**: Async test support
- **httpx**: Async HTTP client testing

### Performance Tests
- **pytest-benchmark**: Performance regression testing
- **locust**: Load testing capabilities

### Code Quality
- **pytest-cov**: Coverage reporting with HTML/XML output
- **black**: Code formatting
- **isort**: Import sorting
- **mypy**: Type checking
- **flake8**: Linting
- **bandit**: Security analysis
- **safety**: Dependency vulnerability scanning

## 📁 Test Structure

```
tests/
├── __init__.py
├── conftest.py              # Shared fixtures and configuration
├── factories.py             # Pydantic model factories
├── unit/                    # Unit tests
│   └── routers/
│       └── test_get_instance_status.py
└── integration/             # Integration tests
    └── test_api_integration.py
```

## 🔧 Key Testing Components

### 1. Advanced pytest Configuration (`pytest.ini`)
```ini
[tool:pytest]
testpaths = tests
asyncio_mode = auto
addopts =
    -v --cov=app --cov-report=html --cov-fail-under=80

markers =
    asyncio: async tests
    unit: unit tests
    integration: integration tests
    api: API endpoint tests
    service: service layer tests
    database: database tests
    external: external API tests
    slow: slow running tests
```

### 2. Modern Fixtures (`conftest.py`)
```python
@pytest.fixture
def test_client() -> TestClient:
    """FastAPI test client for endpoint testing."""
    return TestClient(app)

@pytest.fixture
async def async_client() -> AsyncGenerator[AsyncClient, None]:
    """Async HTTP client for testing."""
    async with AsyncClient(app=app, base_url="http://test") as client:
        yield client

@pytest.fixture
def respx_mock() -> respx.MockRouter:
    """HTTP mock router using respx."""
    with respx.mock() as router:
        yield router
```

### 3. Pydantic Factories (`factories.py`)
```python
class VMDocumentFactory(ModelFactory[VMDocument]):
    """Factory for VM documents with realistic fake data."""

    vm_id = Use(lambda: f"vm-{fake.uuid4()}")
    status = Use(lambda: fake.random_element(elements=list(VMStatus)))
    ip_address = Use(lambda: fake.ipv4())
    # ... other realistic fields
```

### 4. Comprehensive Unit Tests
```python
@pytest.mark.unit
@pytest.mark.api
def test_get_instance_status_success(self, test_client: TestClient, mock_database):
    """Test successful instance status retrieval."""
    # Arrange
    mock_instance = VMDocumentFactory.build(vm_id="test-vm", status=VMStatus.RUNNING)
    mock_database["get_instance"].return_value = mock_instance

    # Act
    response = test_client.get("/api/instances/test-vm/status")

    # Assert
    assert response.status_code == 200
    assert response.json()["status"] == "running"
```

### 5. Integration Tests with Real Infrastructure
```python
@pytest.mark.integration
@pytest.mark.database
def test_with_real_mongodb(self):
    """Test with real MongoDB using testcontainers."""
    with MongoDbContainer("mongo:7.0") as mongo:
        connection_string = mongo.get_connection_url()
        # ... test with real database
```

## 🚀 Running Tests

### Using Makefile Commands
```bash
# Install dependencies for testing
make install-test

# Run different test types
make test              # All tests
make test-unit         # Unit tests only
make test-integration  # Integration tests only
make test-fast         # Exclude slow tests
make test-coverage     # With coverage report
make test-parallel     # Parallel execution

# Code quality
make lint              # Linting
make format            # Code formatting
make type-check        # Type checking
make security-check    # Security analysis
```

### Direct Poetry Commands
```bash
# Run tests with specific markers
poetry run pytest -m "unit"
poetry run pytest -m "api"
poetry run pytest -m "not slow"

# Run with coverage
poetry run pytest --cov=app --cov-report=html

# Run specific test file
poetry run pytest tests/unit/routers/test_get_instance_status.py -v
```

## 🐳 Docker Testing

### Production Testing
```dockerfile
# Dockerfile.production - minimal dependencies
FROM python:3.11-slim
RUN poetry install --only main
```

### Development Testing
```dockerfile
# Dockerfile.dev - includes dev tools
FROM python:3.11-slim
RUN poetry install --with dev --without test
```

### CI Testing
```bash
# CI-specific installation
poetry install --with test,reporting --without dev,integration,performance
```

## 📊 Test Coverage and Reporting

### Coverage Reports
- **Terminal**: Real-time coverage during test runs
- **HTML**: Detailed coverage report in `htmlcov/`
- **XML**: For CI/CD integration

### Test Markers for Organization
- `@pytest.mark.unit` - Fast unit tests
- `@pytest.mark.integration` - Slower integration tests
- `@pytest.mark.api` - API endpoint tests
- `@pytest.mark.service` - Service layer tests
- `@pytest.mark.database` - Database-dependent tests
- `@pytest.mark.external` - External API tests
- `@pytest.mark.slow` - Long-running tests

## 🔍 Example Test Cases

### Unit Test Example
```python
def test_get_instance_status_not_found(self, test_client, mock_database):
    """Test 404 response for non-existent instance."""
    mock_database["get_instance"].return_value = None

    response = test_client.get("/api/instances/missing/status")

    assert response.status_code == 404
    assert "Instance not found" in response.json()["detail"]
```

### Integration Test Example
```python
@pytest.mark.integration
async def test_create_instance_with_external_apis(self):
    """Test with mocked external API calls."""
    with respx.mock() as httpx_mock:
        # Mock TensorDock API
        httpx_mock.get("https://dashboard.tensordock.com/api/v2/locations").mock(
            return_value=httpx.Response(200, json={"data": {"locations": [...]}})
        )

        # Test the full workflow
        async with AsyncClient(app=app) as client:
            response = await client.post("/api/instances", json=request_data)
            assert response.status_code == 201
```

## 🚀 Benefits of This Setup

1. **Environment Separation**: Production stays lean, development is feature-rich
2. **Modern Testing**: FastAPI TestClient, async support, realistic factories
3. **External API Mocking**: respx for HTTP mocking without network calls
4. **Real Infrastructure**: testcontainers for integration tests
5. **Comprehensive Coverage**: Unit, integration, performance, security tests
6. **Developer Experience**: Makefile commands, clear test organization
7. **CI/CD Ready**: Proper reporting formats, parallel execution
8. **Maintainable**: Factories reduce test code duplication

This setup provides a solid foundation for testing FastAPI applications with proper separation of concerns, modern tooling, and scalable test organization.