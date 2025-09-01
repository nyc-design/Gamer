# Development Guide

This guide covers local development setup and workflows for the Gamer project.

## Prerequisites

- Docker and Docker Compose
- Node.js 18+ (for web app development)
- Python 3.11+ (for API development)
- Google Cloud SDK (for deployment)

## Local Development Setup

### 1. Environment Configuration

```bash
# Copy example environment file
cp .env.example .env

# Edit with your values
vim .env
```

### 2. Start Local Services

```bash
# Start all services
docker-compose up -d

# View logs
docker-compose logs -f

# Stop services
docker-compose down
```

### 3. Individual Service Development

#### Web App (Next.js)
```bash
cd services/web-app
npm install
npm run dev  # Runs on http://localhost:3000
```

#### Provisioner API (FastAPI)
```bash
cd services/provisioner-api
python -m venv venv
source venv/bin/activate  # On Windows: venv\Scripts\activate
pip install -r requirements.txt
uvicorn app.main:app --reload --port 8001
```

#### Agent API (FastAPI)
```bash
cd services/agent-api
python -m venv venv
source venv/bin/activate
pip install -r requirements.txt
uvicorn app.main:app --reload --port 8002
```

## Testing

### Running Tests
```bash
# Web App
cd services/web-app
npm test

# Python services
cd services/provisioner-api
pytest

cd services/agent-api
pytest
```

### Integration Testing
```bash
# Full stack testing with docker-compose
docker-compose up -d
# Run integration tests
npm run test:integration
```

## Debugging

### Logs
```bash
# Service logs
docker-compose logs web-app
docker-compose logs provisioner-api
docker-compose logs agent-api

# Follow logs
docker-compose logs -f web-app
```

### Common Issues

1. **Port conflicts**: Change ports in docker-compose.yml if needed
2. **Environment variables**: Ensure .env is properly configured
3. **Docker build issues**: Run `docker-compose build --no-cache`

## Code Quality

### Linting and Formatting
```bash
# Web App
cd services/web-app
npm run lint
npm run lint:fix

# Python services
cd services/provisioner-api
black .
flake8 .
mypy .
```

### Pre-commit Hooks
```bash
# Install pre-commit
pip install pre-commit
pre-commit install

# Run on all files
pre-commit run --all-files
```

## Deployment

### Local Testing
Test deployment locally before pushing:
```bash
# Build production images
docker-compose -f docker-compose.prod.yml build

# Test production setup
docker-compose -f docker-compose.prod.yml up
```

### Cloud Deployment
Deployment happens automatically via GitHub Actions when pushing to main branch.

## Monitoring

### Health Checks
- Web App: http://localhost:3000/api/health
- Provisioner API: http://localhost:8001/health
- Agent API: http://localhost:8002/health

### Performance Monitoring
Use browser dev tools and API response times for performance analysis.

## Contributing

1. Create feature branch: `git checkout -b feature/your-feature`
2. Make changes and test locally
3. Run linting and tests
4. Submit PR with clear description
5. Ensure CI passes before merge