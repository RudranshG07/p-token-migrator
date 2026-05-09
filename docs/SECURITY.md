# Security

This service is designed to be safe for public source uploads.

## Public Deployment Defaults

- Browser uploads use `/api/scan-sources`; users do not need server filesystem access.
- Arbitrary server-path scanning is disabled when `NODE_ENV=production`.
- Uploaded file paths are normalized, path traversal is rejected, and only `.rs`, `.json`, and `.toml` files are scanned.
- Request bodies are limited by `MAX_BODY_BYTES`.
- Scan endpoints are rate-limited by IP.
- Scan endpoints can require `API_KEY` for a controlled beta.
- The server sets basic hardening headers for content type, referrer, permissions, and same-origin resource policy.

## Do Not Enable On Public Hosts

Do not set `ALLOW_SERVER_PATH_SCAN=1` on a public deployment. That mode is only for trusted self-hosted or local use.

## Data Handling

Uploaded source contents are scanned in memory. Public production defaults store only job summaries and public reports. If `STORE_FULL_MANIFESTS=1`, saved jobs include snippets and replacement guidance, so treat `JOB_STORE_PATH` as sensitive.

## Production Gaps

- Add authentication before accepting private customer projects.
- Keep app-level rate limiting enabled and add rate limiting at the edge.
- Move job storage to a database with retention controls.
- Add audit logs for scan requests.
- Add malware/file-type validation if archive upload support is added later.
