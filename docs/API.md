# API

The public dashboard uses the same API that external tools can call.

If `API_KEY` is set, mutating scan endpoints require either:

```http
Authorization: Bearer <key>
```

or:

```http
X-API-Key: <key>
```

## Health

```http
GET /api/health
```

Returns service mode, whether server-path scanning is enabled, and the bundled sample path.
It also returns the active benchmark profile name.

## Readiness

```http
GET /api/ready
```

Returns `200` when the job store can be read.

## Scan Uploaded Sources

```http
POST /api/scan-sources
Content-Type: application/json
```

Body:

```json
{
  "protocol": "My Protocol",
  "files": [
    {
      "relative": "programs/vault/src/lib.rs",
      "content": "use anchor_lang::prelude::*;"
    }
  ]
}
```

This is the endpoint to use for public deployments. It accepts `.rs`, `.json`, and `.toml` files, ignores unsafe relative paths, and limits uploads to the configured `MAX_BODY_BYTES`.

## Scan Server Path

```http
POST /api/scan
Content-Type: application/json
```

Body:

```json
{
  "protocol": "Sample Vault",
  "projectPath": "sample"
}
```

In production, arbitrary server paths are blocked unless `ALLOW_SERVER_PATH_SCAN=1`. Keep that disabled for public deployments.

## List Jobs

```http
GET /api/jobs
```

Returns the latest saved scan jobs. The default store is `data/jobs.json`.

## Public Report

```http
GET /api/reports/:id
```

Returns a public summary for a saved scan job. This endpoint exposes aggregate operation counts, risk counts, affected files, and CU totals. It does not expose snippets or replacement patches when `STORE_FULL_MANIFESTS=0`.
