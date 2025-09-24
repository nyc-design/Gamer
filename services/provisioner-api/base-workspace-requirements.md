# Base Workspace Image Requirements

## Essential Python Development Tools

### Core Python Environment
```bash
# Python 3.11+ with pip
python3.11
python3-pip
python3-venv

# Poetry for dependency management
poetry>=1.5.0

# Essential build tools
build-essential
git
curl
wget
```

### Testing & Quality Tools
```bash
# These should be available globally in the workspace
pytest>=7.4.0
pytest-asyncio>=0.21.0
pytest-mock>=3.11.0
pytest-cov>=4.1.0
pytest-xdist>=3.3.0        # Parallel testing
pytest-html>=3.2.0         # HTML reports

# HTTP Testing
httpx>=0.25.0
respx>=0.20.0
responses>=0.23.0

# Pydantic Factories
pydantic-factories>=1.17.0
polyfactory>=2.7.0
factory-boy>=3.3.0

# Database Testing
mongomock>=4.1.0
pytest-mongodb>=2.4.0

# Container Testing (for integration tests)
docker
docker-compose
testcontainers>=3.7.0
pytest-docker>=2.0.0

# Code Quality & Formatting
black>=23.0.0
isort>=5.12.0
flake8>=6.0.0
mypy>=1.5.0
bandit>=1.7.0              # Security linting
safety>=2.3.0              # Dependency vulnerability checking

# Additional Testing Utilities
faker>=19.0.0
deepdiff>=6.3.0
freezegun>=1.2.0
parameterized>=0.9.0
hypothesis>=6.80.0

# Performance Testing
pytest-benchmark>=4.0.0
locust>=2.15.0
```

### Database & Infrastructure Tools
```bash
# Database clients for testing
postgresql-client
mongodb-tools
redis-tools

# Container runtime
docker.io
docker-compose

# Cloud tools (for integration testing)
google-cloud-cli
aws-cli
azure-cli

# Monitoring & Debugging
htop
curl
jq                          # JSON processing
```

### FastAPI Specific
```bash
# These are often needed across FastAPI projects
fastapi
uvicorn[standard]
httpx                       # Async HTTP client
pydantic
pydantic-settings
```

### Development Utilities
```bash
# Version control
git
gh                          # GitHub CLI

# Process management
supervisor

# Text processing
jq
yq                          # YAML processing
```

## Recommended Dockerfile for Base Workspace

```dockerfile
FROM python:3.11-slim

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    git \
    jq \
    postgresql-client \
    docker.io \
    docker-compose \
    && rm -rf /var/lib/apt/lists/*

# Install Poetry
RUN pip install poetry>=1.5.0

# Install global development tools
RUN pip install \
    pytest>=7.4.0 \
    pytest-asyncio>=0.21.0 \
    pytest-mock>=3.11.0 \
    pytest-cov>=4.1.0 \
    pytest-xdist>=3.3.0 \
    pytest-html>=3.2.0 \
    httpx>=0.25.0 \
    respx>=0.20.0 \
    black>=23.0.0 \
    isort>=5.12.0 \
    flake8>=6.0.0 \
    mypy>=1.5.0 \
    bandit>=1.7.0 \
    safety>=2.3.0

# Install GitHub CLI
RUN curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | gpg --dearmor -o /usr/share/keyrings/githubcli-archive-keyring.gpg \
    && echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null \
    && apt-get update \
    && apt-get install -y gh

# Set working directory
WORKDIR /workspace

# Configure Poetry to not create virtual environments (since we're in a container)
RUN poetry config virtualenvs.create false

# Create common directories
RUN mkdir -p /workspace/projects
```

## Poetry Commands to Remember

```bash
# Install project dependencies based on environment
poetry install --only main                    # Production
poetry install --with dev                     # Development
poetry install --with test                    # Testing
poetry install --with dev,test               # Full development
poetry install --with dev,test,integration   # Everything

# Add dependencies to specific groups
poetry add pytest --group test
poetry add black --group dev
poetry add testcontainers --group integration

# Export requirements (if needed for Docker)
poetry export -f requirements.txt --output requirements.txt --only main
poetry export -f requirements.txt --output requirements-dev.txt --with dev,test

# Run commands in Poetry environment
poetry run pytest
poetry run black .
poetry run mypy app/

# Check dependency vulnerabilities
poetry run safety check
poetry run bandit -r app/
```

This setup gives you maximum flexibility while keeping production deployments lean and development environments rich with tools.