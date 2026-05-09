# Storage

The app currently ships with a file-backed job store. The server talks to storage through the `JobStore` interface in `src/storage.ts`.

## Current Adapter

`createFileJobStore(path)` stores job summaries in JSON at `JOB_STORE_PATH`.

## Production Database Plan

For a public service with real users, replace the file adapter with a database adapter that implements:

```ts
interface JobStore {
  list(): Promise<Job[]>;
  find(id: string): Promise<Job | undefined>;
  save(manifest: Manifest, options?: SaveJobOptions): Promise<Job>;
}
```

Recommended first database: Postgres through Neon, Supabase, Railway, or Fly Postgres.

Minimal table shape:

```sql
create table migration_jobs (
  id text primary key,
  protocol text not null,
  created_at timestamptz not null,
  totals jsonb not null,
  simulation jsonb not null,
  report jsonb not null,
  manifest jsonb,
  created_by text
);

create index migration_jobs_created_at_idx on migration_jobs (created_at desc);
```

Keep `manifest` nullable. Public deployments should store only summaries unless users explicitly agree to retain source snippets.
