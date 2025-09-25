# Test Dockerfile - includes test dependencies
FROM python:3.11-slim

WORKDIR /app

# Install Poetry
RUN pip install poetry

# Copy dependency files
COPY pyproject.toml poetry.lock ./

# Install all dependencies including tests
RUN poetry config virtualenvs.create false \
    && poetry install --with dev,test

# Copy application code
COPY . .

# Default command for testing
CMD ["pytest"]