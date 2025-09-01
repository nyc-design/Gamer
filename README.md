# Gamer - Cloud Gaming Platform

A cloud-based emulation gaming platform for streaming Nintendo console games through Moonlight.

## Quick Start

1. **Clone the repository**
   ```bash
   git clone <repo-url>
   cd gamer
   ```

2. **Copy environment variables**
   ```bash
   cp .env.example .env
   # Edit .env with your actual values
   ```

3. **Start local development**
   ```bash
   docker-compose up -d
   ```

4. **Access services**
   - Web App: http://localhost:3000
   - Provisioner API: http://localhost:8001
   - Agent API: http://localhost:8002
   - RomM (optional): http://localhost:8080

## Architecture

See [CLAUDE.md](./CLAUDE.md) for detailed project architecture, setup instructions, and development guidelines.

## Services

- **Web App** (Next.js): User interface and authentication
- **Provisioner API** (FastAPI): VM lifecycle management
- **Agent API** (FastAPI): Game coordination and communication
- **Client Agent** (Python): On-VM game launcher
- **VM Image**: Custom Wolf image with emulators

## Development

Each service can be developed independently. See individual README files in each service directory for specific setup instructions.

## Deployment

Services deploy automatically to Google Cloud Run via GitHub Actions when changes are pushed to main branch.

## Contributing

1. Create feature branch
2. Make changes
3. Test locally with docker-compose
4. Submit pull request

## License

MIT