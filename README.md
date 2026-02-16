# Go vs Rust vs Python: Realistic CRUD + JWT + Rate Limit Benchmark (2026 Edition)

Same microservice implemented in **Go (Fiber)**, **Rust (Axum)**, **Python (FastAPI)**.

Features:
- CRUD on `/users` (id, name, email)
- JWT HS256 authentication
- Per-user rate limiting (100 req/min) backed by Redis
- PostgreSQL storage
- Docker Compose setup

## Quick Start

1. Start infra
```bash
docker compose up -d postgres redis
