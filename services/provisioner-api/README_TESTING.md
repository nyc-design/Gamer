# Testing Guide

## Simple Testing Setup

This project uses pytest with essential testing dependencies for simple unit and integration tests.

## Running Tests

### Option 1: Direct pytest (if dependencies installed)
```bash
# Run all tests
pytest

# Run specific test file
pytest tests/unit/test_get_instance_status.py

# Run with verbose output
pytest -v

# Run specific test function
pytest tests/unit/test_get_instance_status.py::test_get_instance_status_success
```

### Option 2: Using Docker (recommended for consistent environment)
```bash
# Build and run tests in container
./test.sh

# Or manually with docker-compose
docker-compose -f docker-compose.test.yml run --rm provisioner-api-test
```

## Test Structure

Each PR branch includes:
- **Function implementation** in the service/router files
- **Unit test** in `tests/unit/test_[function_name].py`

Example test files:
- `tests/unit/test_get_instance_status.py` - Tests the get_instance_status function
- `tests/unit/test_tensordock_create_vm.py` - Tests TensorDock VM creation
- `tests/unit/test_gcp_create_vm.py` - Tests GCP VM creation

## Dependencies

Essential testing tools included:
- `pytest` - Core testing framework
- `pytest-asyncio` - For async function testing
- `pytest-mock` - Clean mocking capabilities
- `httpx` - HTTP client (already used by app)
- `respx` - HTTP request/response mocking
- `polyfactory` - Pydantic model factories
- `faker` - Realistic test data
- `mongomock` - Database mocking

## Writing Tests

Keep tests simple and focused:

1. **One test file per function**
2. **Mock external dependencies** (databases, APIs, services)
3. **Test success and failure scenarios**
4. **Use descriptive test function names**

Example test pattern:
```python
import pytest
from unittest.mock import patch, MagicMock

def test_function_success():
    # Mock external dependencies
    with patch('app.module.external_service') as mock_service:
        mock_service.return_value = expected_data

        # Call function
        result = your_function(input_data)

        # Assert results
        assert result == expected_result
        mock_service.assert_called_once()
```