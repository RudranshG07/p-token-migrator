# Deployment

## Local Production Run

```bash
cp .env.example .env
NODE_ENV=production HOST=0.0.0.0 PORT=4173 npm run dev
```

## Docker

```bash
docker build -t p-token-migrator .
docker run --rm -p 4173:4173 -v p-token-migrator-data:/app/data p-token-migrator
```

Or:

```bash
docker compose up --build
```

## Public Hosting Requirements

- Run with `NODE_ENV=production`.
- Keep `ALLOW_SERVER_PATH_SCAN=0`.
- Put the service behind HTTPS.
- Mount `/app/data` to durable storage or replace the JSON job store with a database.
- Keep `STORE_FULL_MANIFESTS=0` unless users explicitly agree to persisted source snippets.
- Set `MAX_BODY_BYTES` to the largest project upload you want to support.
- Set `PTOKEN_PROFILE_PATH` when you have a validated benchmark profile JSON.
- Set `RATE_LIMIT_MAX` and also add rate limiting at the reverse proxy or platform edge.
- Set `API_KEY` if you want to restrict write/scan API access during beta. Dashboard users can enter this key before scanning.
- Set up logs and uptime checks against `/api/ready`.

## Reverse Proxy

Forward traffic to the app on `PORT` and preserve the original host header. The app serves the dashboard and API from the same origin.

## Benchmark Profiles

The bundled profile lives at `profiles/simd-0266-estimator.json`. Once p-token interfaces and benchmarks are finalized, create a new profile with the same shape and set:

```bash
PTOKEN_PROFILE_PATH=/app/profiles/mainnet-profile.json
```

The profile controls CU estimates only. It does not change scanner detection rules.

## First Public Launch Checklist

1. Pick a public domain.
2. Deploy the Docker image or equivalent Node service.
3. Configure HTTPS.
4. Verify `GET /api/ready`.
5. Upload a real Anchor project folder through the dashboard.
6. Download the manifest and confirm findings.
7. Share the API docs with early Solana protocol teams.
