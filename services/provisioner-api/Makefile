# Gaming VM Provisioner API - Development Commands

# Default Python and Poetry settings
PYTHON := python
POETRY := poetry
APP_DIR := app
TESTS_DIR := tests

.PHONY: help install test test-unit test-integration test-external test-coverage lint format type-check clean

# Help target
help:
	@echo "Available commands:"
	@echo "  install          Install dependencies for development"
	@echo "  install-prod     Install only production dependencies"
	@echo "  install-test     Install test dependencies"
	@echo "  test             Run all tests"
	@echo "  test-unit        Run unit tests only"
	@echo "  test-integration Run integration tests only"
	@echo "  test-external    Run tests with external API calls"
	@echo "  test-coverage    Run tests with coverage report"
	@echo "  test-fast        Run fast tests (exclude slow/integration)"
	@echo "  lint             Run code linting"
	@echo "  format           Format code with black and isort"
	@echo "  type-check       Run mypy type checking"
	@echo "  clean            Clean up generated files"
	@echo "  dev-server       Start development server"

# Installation targets
install:
	$(POETRY) install --with dev,test

install-prod:
	$(POETRY) install --only main

install-test:
	$(POETRY) install --with test --without dev

install-all:
	$(POETRY) install --with dev,test,integration,performance,reporting

# Test targets
test:
	$(POETRY) run pytest

test-unit:
	$(POETRY) run pytest -m "unit" -v

test-integration:
	$(POETRY) run pytest -m "integration" -v

test-external:
	$(POETRY) run pytest -m "external" -v

test-database:
	$(POETRY) run pytest -m "database" -v

test-api:
	$(POETRY) run pytest -m "api" -v

test-service:
	$(POETRY) run pytest -m "service" -v

test-fast:
	$(POETRY) run pytest -m "not slow and not integration and not external"

test-coverage:
	$(POETRY) run pytest --cov=$(APP_DIR) --cov-report=html --cov-report=term-missing

test-parallel:
	$(POETRY) run pytest -n auto

# Code quality targets
lint:
	$(POETRY) run flake8 $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run bandit -r $(APP_DIR)

format:
	$(POETRY) run black $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run isort $(APP_DIR) $(TESTS_DIR)

type-check:
	$(POETRY) run mypy $(APP_DIR)

format-check:
	$(POETRY) run black --check $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run isort --check-only $(APP_DIR) $(TESTS_DIR)

security-check:
	$(POETRY) run safety check
	$(POETRY) run bandit -r $(APP_DIR)

# Development targets
dev-server:
	$(POETRY) run uvicorn app.main:app --reload --host 0.0.0.0 --port 8000

dev-install:
	$(POETRY) install --with dev --without test,integration,performance,reporting

# Clean up
clean:
	rm -rf htmlcov/
	rm -rf .coverage
	rm -rf coverage.xml
	rm -rf .pytest_cache/
	rm -rf __pycache__/
	find . -type d -name __pycache__ -delete
	find . -type f -name "*.pyc" -delete

# CI/CD targets
ci-install:
	$(POETRY) install --with test,reporting --without dev,integration,performance

ci-test:
	$(POETRY) run pytest --cov=$(APP_DIR) --cov-report=xml --junitxml=pytest.xml

ci-quality:
	$(POETRY) run black --check $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run isort --check-only $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run flake8 $(APP_DIR) $(TESTS_DIR)
	$(POETRY) run mypy $(APP_DIR)
	$(POETRY) run safety check

# Database commands (for integration testing)
db-start:
	docker run -d --name test-mongodb -p 27017:27017 mongo:7.0

db-stop:
	docker stop test-mongodb || true
	docker rm test-mongodb || true

# Docker commands
docker-build:
	docker build -f Dockerfile.production -t provisioner-api:latest .

docker-build-dev:
	docker build -f Dockerfile.dev -t provisioner-api:dev .

docker-test:
	docker build -f Dockerfile.dev -t provisioner-api:test . \
	&& docker run --rm provisioner-api:test poetry run pytest