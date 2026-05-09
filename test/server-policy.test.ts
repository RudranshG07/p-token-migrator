import test from "node:test";
import assert from "node:assert/strict";
import { ApiError, applyRequestPolicy } from "../src/server.ts";

test("public read routes bypass auth and rate limits", () => {
  const buckets = new Map<string, { count: number; resetAt: number }>();

  assert.doesNotThrow(() => applyRequestPolicy({
    method: "GET",
    pathname: "/api/health",
    client: "client-a"
  }, {
    apiKey: "secret",
    rateLimitWindowMs: 60_000,
    rateLimitMax: 0,
    now: 1,
    buckets
  }));
});

test("api key protects scan routes when configured", () => {
  const config = {
    apiKey: "secret",
    rateLimitWindowMs: 60_000,
    rateLimitMax: 10,
    now: 1,
    buckets: new Map<string, { count: number; resetAt: number }>()
  };

  assert.throws(() => applyRequestPolicy({
    method: "POST",
    pathname: "/api/scan-sources",
    client: "client-a"
  }, config), (error) => error instanceof ApiError && error.status === 401);

  assert.doesNotThrow(() => applyRequestPolicy({
    method: "POST",
    pathname: "/api/scan-sources",
    client: "client-a",
    apiKeyHeader: "secret"
  }, config));
});

test("rate limit protects scan routes", () => {
  const config = {
    rateLimitWindowMs: 60_000,
    rateLimitMax: 1,
    now: 1,
    buckets: new Map<string, { count: number; resetAt: number }>()
  };

  assert.doesNotThrow(() => applyRequestPolicy({
    method: "POST",
    pathname: "/api/scan-sources",
    client: "client-a"
  }, config));

  assert.throws(() => applyRequestPolicy({
    method: "POST",
    pathname: "/api/scan-sources",
    client: "client-a"
  }, config), (error) => error instanceof ApiError && error.status === 429);
});
