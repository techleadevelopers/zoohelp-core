# <img src="https://res.cloudinary.com/limpeja/image/upload/v1779071066/Gemini_Generated_Image_v5ufmcv5ufmcv5uf-removebg-preview_lcxvg8.png" alt="ZooHelp Logo" width="58" align="center"> Helpin Hybrid Core

`zoohelp-backend` is the Rust-first backend infrastructure for ZooHelp, a geolocation-driven animal rescue, adoption, NGO coordination, trust, notification, and community protection platform.

The system is designed around one operational problem:

`animal in need -> trusted report -> geospatial prioritization -> nearby helpers/NGOs -> coordinated rescue outcome`

ZooHelp is not only an adoption app. The backend is being shaped as a real-time animal protection coordination layer: posts, rescue alerts, chat, nearby search, NGO profiles, trust signals, media moderation, donation intents, support workflows, and AI-assisted operational tooling.

![Rust](https://img.shields.io/badge/Rust-Core_Backend-orange?style=for-the-badge&logo=rust)
![Python](https://img.shields.io/badge/Python-AI_Workers-blue?style=for-the-badge&logo=python)
![PostgreSQL](https://img.shields.io/badge/PostgreSQL-Core_DB-blue?style=for-the-badge&logo=postgresql)
![Redis](https://img.shields.io/badge/Redis-Geospatial_Cache-red?style=for-the-badge&logo=redis)
![Axum](https://img.shields.io/badge/Axum-HTTP%2FWebSocket-darkorange?style=for-the-badge&logo=rust)
![Docker](https://img.shields.io/badge/Docker-Containerized-blue?style=for-the-badge&logo=docker)
![License](https://img.shields.io/badge/License-Private-darkgreen?style=for-the-badge)

## Objective

ZooHelp Hybrid Core provides the backend runtime for a modern animal rescue network.

The operational MVP is documented in [`docs/operational-mvp.md`](docs/operational-mvp.md). It keeps the first scope focused on verified NGOs, manual trust review, real rescue coordination, and measurable rescue outcomes before expanding into heavier AI or automation layers.

The platform is intended to support cases such as:

- a person finds an injured animal and posts an urgent request
- GPS coordinates are attached to the rescue post
- nearby users, volunteers, vets, and NGOs are identified
- rescue alerts are generated with deep links and action payloads
- the feed prioritizes urgent and nearby cases
- chat coordinates the rescue operation
- trust signals reduce abuse, fraud, and low-quality reports
- media moderation and AI workers assist content safety
- donations, support tickets, and NGO profiles support the broader ecosystem

The operating model is:

`always available -> mostly local relevance -> urgent cases prioritized -> nearby response activated`

## Product Positioning

The core product claim is:

`geospatial rescue coordination + trust-aware community feed + NGO operational network + AI-assisted safety`

The backend is intentionally split by responsibility:

- Rust owns latency-sensitive, high-concurrency, user-facing systems.
- Python owns ML, automation, moderation, analytics, and experimental intelligence layers.

This separation keeps the operational path fast while allowing the intelligence layer to evolve without slowing the critical rescue flow.

## Design Principles

The codebase is organized around practical production constraints:

- keep urgent rescue creation fast and deterministic
- require real geolocation for emergency fan-out
- prioritize nearby cases before generic feed content
- separate core backend from AI workers
- make fraud, trust, moderation, and reporting first-class systems
- preserve mobile/backend contract compatibility with tests
- avoid putting heavy ML inference in the request hot path
- treat observability and readiness as production features, not afterthoughts

## Architecture Overview

```text
[Mobile App]
   |  HTTPS / WebSocket
   v
[Rust API Gateway - Axum/Tokio]
   |-- Auth / Users / Sessions
   |-- Feed / Posts / Media / Search
   |-- Chat HTTP + WebSocket
   |-- Geo Nearby / Rescue Alerts
   |-- ONG Profiles / Follow / Trust
   |-- Donations / Support / Reports
   |
   | events / jobs
   v
[Notification Engine]
   |-- nearby recipient selection
   |-- rescue alert generation
   |-- deep link action payloads
   |-- push-token subscription registry
   |
   v
[PostgreSQL latitude/longitude]  [Redis Geospatial Cache - production scale]  [NATS/Kafka - planned production queue]
   |
   v
[Python Intelligence Layer]
   |-- image moderation
   |-- NLP classification
   |-- fraud model experiments
   |-- recommendation jobs
   |-- analytics and admin automation
```

## Visual Architecture

Versioned architecture diagrams live in [`docs/architecture/README.md`](docs/architecture/README.md).

Included diagrams:

- C4 context
- C4 container
- rescue lifecycle
- rescue creation sequence
- notification flow
- event flow
- chat realtime flow
- benchmark evidence flow

## Runtime Pipeline

The active rescue publication flow is intentionally linear.

1. Mobile user writes a rescue/help request.
2. Mobile captures photo, urgency, and GPS coordinates.
3. Rust validates post payload and media contract.
4. Emergency posts must include `latitude` and `longitude`.
5. Rust creates the post contract.
6. Fraud text scoring runs in the request path as a cheap deterministic signal.
7. Urgent/emergency cases create a durable `rescue_fanout_state`.
8. The rescue fanout worker expands by operational phase and ranks nearby candidates.
9. Push jobs are created through the existing durable notification infrastructure.
10. `Estou indo` creates a real rescue response and pauses aggressive expansion.
11. Feed/search/notifications expose the rescue case back to the app with operational status.

Simplified:

`mobile GPS -> post create -> validation -> fanout state -> phased nearby push -> response -> feed/chat coordination`

## Operational Rescue Alert Model

The current backend supports a production-shaped rescue coordination contract.

Emergency posts require:

- `postType = emergency` or `urgent = true`
- `latitude`
- `longitude`
- description
- location label

When accepted, the backend creates a durable fanout state and returns operational rescue metadata:

```json
{
  "rescueFanoutStateId": "uuid",
  "rescueOperational": {
    "fanoutPhase": 1,
    "helpGoingCount": 0,
    "helpArrivedCount": 0,
    "operationalLabel": "Precisa de ajuda"
  }
}
```

The fanout worker is controlled by `RESCUE_FANOUT_WORKER_ENABLED`. It claims due fanout states with row locking, selects candidates using geo filters plus operational score, creates push jobs, records attempts, and pauses expansion when a real helper confirms `Estou indo`.

MVP fanout phases:

| Phase | Radius | Purpose |
|-------|--------|---------|
| 1 | `0.3 km` | sniper local, critical-alert users and recently active helpers |
| 2 | `0.7 km` | controlled local expansion |
| 3 | `1.0 km` | neighborhood expansion |
| 4 | `3.0 km` | broader city-area expansion |
| 5 | verified/ONG/provider escalation | include trusted institutional actors with wider radius |
| 6 | `10 km` specialists | local specialist search |
| 7 | `30 km` specialists | regional specialist search |
| 8 | `100 km` specialists | state-level specialist search |
| 9 | `300 km` agencies/specialists | environmental agency or rare-case escalation |

Specialist escalation uses `rescue_specialist_providers` and `rescue_escalation_attempts`. It searches for competent responders by animal scope and provider type before falling back to verified/ONG/vet/admin users. It does not broadcast regional alerts to generic unverified users.

The old rescue alert preview endpoint remains useful for contract preview, but production delivery should use the persisted fanout state, specialist escalation state and durable push jobs.

## Core Backend Surface

### Auth and Identity

- `POST /v1/auth/login`
- `POST /v1/auth/register`
- `POST /v1/auth/password-reset`
- `DELETE /v1/me`

Supports personal users, NGOs, and vet-style accounts at the frontend contract level.

### Feed and Posts

- `GET /v1/feed`
- `POST /v1/posts`
- `GET /v1/posts/:id`
- `POST /v1/posts/:id/like`
- `POST /v1/posts/:id/comments`
- `POST /v1/posts/:id/report`
- `POST /v1/posts/:id/rescue-response`

Posts support adoption, lost, found, emergency, campaign, and general community post types.

### Media

- `POST /v1/media/upload-intents`

Cloudinary upload-intent flow is used for image/video media before post creation.

### Chat

- `GET /v1/chat/rooms`
- `GET /v1/chat/rooms/:id`
- `GET /v1/chat/rooms/:id/messages`
- `POST /v1/chat/rooms/:id/messages`
- `GET /v1/chat/rooms/:id/ws`

HTTP chat and WebSocket room path are present for real-time coordination.

### Geolocation

- `GET /v1/geo/nearby`

Nearby logic is based on geographic distance and is aligned with rescue, feed, and map usage.

### Rescue Coordination

- `POST /v1/posts/:id/rescue-response`
- `POST /v1/rescue/active/:id/responses`

The response endpoint records helper intent such as `confirmed` or `arrived`. A confirmed response means someone is going, not that the case is resolved. The backend must not mark a post or rescue session as resolved from this action alone.

### Notifications

- `GET /v1/notifications`
- `PATCH /v1/notifications/:id/mark-as-read`
- `POST /v1/notifications/:id/ack`
- `POST /v1/notifications/push-token`
- `POST /v1/notifications/rescue-alerts/:post_id/preview`

The notification layer supports rescue alert modeling, push token registration, dedupe keys, categories, deep links, and critical flags.

### NGOs, Trust, Donations, Support, Search

- `GET /v1/ongs`
- `GET /v1/ongs/:id`
- `POST /v1/ongs/:id/follow`
- `GET /v1/trust/score/:subject_id`
- `POST /v1/donations/intents`
- `GET /v1/support/meta`
- `GET /v1/support/tickets`
- `POST /v1/support/tickets`
- `GET /v1/search`

## Hybrid Intelligence Layer

Python is reserved for auxiliary intelligence and automation, not the latency-sensitive request core.

Intended Python responsibilities:

- image moderation
- content classification
- NLP risk tagging
- advanced recommendations
- analytics pipelines
- internal dashboards
- fraud model experiments
- admin automation scripts

Production rule:

`Rust handles real-time user operations. Python handles intelligence and background automation.`

## Geospatial Decision Framework

The practical decision model for rescue visibility is based on signals that can be measured and replayed:

- emergency status
- user location
- post coordinates
- radius in kilometers
- recipient subscription radius
- trust state
- content category
- notification dedupe state
- feed freshness and proximity

The rescue alert radius is phase-based:

- phase 1 sniper local: `0.3 km`
- phase 2: `0.7 km`
- phase 3: `1.0 km`
- phase 4: `3.0 km`
- phase 5: verified/ONG/provider escalation with wider operational reach

`30 m` may remain a technical lower bound for validation or preview paths, but it is not the operational phase-1 radius.

Compact distance rule:

```math
recipient\_eligible = distance(post, subscriber) <= min(phase\_radius, subscriber\_radius)
```

Candidate ordering is not distance-only. The worker ranks by expected operational response using proximity, recent activity, trust, role, verification, critical-alert preference, and fatigue/cooldown.

## Operational Evidence

Current local validation is based on automated tests and contract checks.

### Backend Test Surface

`cargo test` currently validates:

- frontend feed filters
- auth register frontend shape
- post validation
- media upload contract
- emergency coordinate requirement
- emergency rescue fanout state creation
- geospatial distance calculations
- notification recipient filtering
- fraud scoring
- trust scoring
- JWT/password auth services

Latest local result for the fanout integration:

```text
cargo check passed
cargo test compiled; route tests hit local DB pool timeout
```

### Mobile Contract Validation

The mobile app type contract has been validated with:

```bash
pnpm --filter zoohelp-mobile run typecheck
```

This validates the TypeScript contract across:

- post creation
- latitude/longitude forwarding
- rescue alert response typing
- mobile/backend post mapping
- feed micro-composer integration

## Performance and Scaling Notes

The architecture is designed for low-latency rescue coordination, but performance claims should be backed by measured output.

Executable benchmark assets live in [`benchmarks/`](benchmarks/).

Quick commands:

```powershell
k6 run .\benchmarks\k6\http-rescue-feed.js
k6 run .\benchmarks\k6\websocket-chat.js
locust -f .\benchmarks\locust\locustfile.py --host http://127.0.0.1:8080
vegeta attack -duration=60s -rate=100 -targets=.\benchmarks\vegeta\feed.targets | vegeta report
```

Candidate WebSocket scale run:

```powershell
$env:ROOM_ID = "<chat-room-id>"
$env:ACCESS_TOKEN = "<jwt>"
$env:K6_WS_VUS = "10000"
$env:K6_DURATION = "10m"
k6 run .\benchmarks\k6\websocket-chat.js
```

Benchmark reports should be attached under [`benchmarks/reports/`](benchmarks/reports/) before using any public throughput claim.

Useful evidence for production hardening:

- post creation latency p50/p95/p99
- feed latency p50/p95/p99
- WebSocket connection count and fan-out latency
- rescue alert fan-out time by recipient count
- Redis geospatial query latency
- PostgreSQL coordinate query latency
- push delivery success and delay by provider
- image upload success rate and moderation delay
- report/fraud false-positive review rate

No unsupported global-scale throughput claim should be treated as production proof until benchmarked with PostgreSQL, Redis, queue, upload, WebSocket, and push delivery enabled.

## Production Architecture Target

The production target is:

```text
Mobile App
  -> Cloudflare / Edge Protection
  -> Rust API Gateway
  -> PostgreSQL latitude/longitude
  -> Redis Geospatial / Rate Limit / Session Cache
  -> NATS or Kafka Event Bus
  -> Notification Delivery Workers
  -> FCM / APNs
  -> Python AI Workers
  -> Observability Stack
```

Recommended durability split:

| Layer | Production Role |
|-------|-----------------|
| PostgreSQL | authoritative relational state and latitude/longitude storage |
| Redis | low-latency geospatial lookup, cache, rate limits |
| NATS/Kafka | durable rescue alert and moderation events target |
| Rust workers | notification fan-out, realtime coordination, trust/fraud core |
| Python workers | AI moderation, NLP, analytics, model experiments |
| Cloudinary/S3/R2 | media storage and delivery |
| FCM/APNs | push notification delivery |

Current queue reality:

- critical rescue notification state is persisted in PostgreSQL through `notification_events`, `push_delivery_jobs`, `rescue_fanout_states`, `rescue_fanout_attempts`, `rescue_responses`, `rescue_specialist_providers`, and `rescue_escalation_attempts`
- workers claim due jobs with row locking and persist retry/dead-letter state
- NATS is present for cross-process realtime fanout, but the current implementation uses plain pub/sub, not JetStream/Kafka-style durable replay
- WebSocket broadcast channels are in-memory delivery surfaces only; the authoritative chat and rescue history remains in PostgreSQL

## Security and Trust Model

ZooHelp is a trust-sensitive system. The backend assumes abuse will happen.

Security and integrity controls:

- JWT-based auth surface
- password hashing service
- refresh tokens persisted in PostgreSQL with revocation timestamps
- account deletion endpoint
- report endpoint for content moderation
- trust scoring service
- fraud text scoring
- media moderation queue status
- push-token registration contract
- support tickets and operational escalation
- validation on critical request payloads
- emergency geolocation requirement

Production hardening still required:

- access-token revocation before expiry through a session table, `jti` denylist, or user token-version check
- role-based authorization beyond contract shape
- full audit log
- rate limits enforced at edge and API levels
- durable report/moderation workflow
- FCM/APNs delivery receipts
- durable NATS JetStream/Kafka consumers for replayable realtime/domain events
- API restart and worker restart evidence proving no critical rescue/chat state is lost

## Reliability Controls

Current reliability-oriented surfaces:

- `/healthz`
- `/readyz`
- `/metrics`
- `/v1/observability`
- structured rescue alert logging
- validation tests for frontend/backend contracts
- Docker Compose local infrastructure

Production reliability targets:

- readiness tied to PostgreSQL/Redis/NATS availability
- OpenTelemetry traces across post -> alert -> push delivery
- Sentry or equivalent error aggregation
- Prometheus dashboards for API, queue, push, and websocket metrics
- alerting for notification delay, failed uploads, auth failures, and WebSocket disconnect spikes

Production readiness gate:

- [`docs/production-readiness.md`](docs/production-readiness.md)
- persistence completeness
- durable notifications
- queue guarantees
- retries and DLQ
- staging evidence before public scale claims

## Environment Variables

Core backend:

```text
BIND_ADDR
DATABASE_URL
REDIS_URL
NATS_URL
AI_WORKER_URL
JWT_SECRET
ACCESS_TOKEN_TTL_MINUTES
REFRESH_TOKEN_TTL_DAYS
PUSH_WORKER_ENABLED
RESCUE_FANOUT_WORKER_ENABLED
POSTGIS_ENABLED
```

Production guardrails:

- outside development, `JWT_SECRET` must be a real non-placeholder secret with at least 32 characters
- outside development, `ACCESS_TOKEN_TTL_MINUTES` must be between `1` and `60`
- outside development, `PUSH_WORKER_ENABLED=true` and `RESCUE_FANOUT_WORKER_ENABLED=true` are required
- `NATS_URL` is required outside development, but NATS currently supports realtime fanout only; durable queue semantics still come from PostgreSQL job tables until JetStream/Kafka is implemented

Cloudinary media:

```text
CLOUDINARY_CLOUD_NAME
CLOUDINARY_API_KEY
CLOUDINARY_API_SECRET
CLOUDINARY_URL
```

Mobile/API integration:

```text
EXPO_PUBLIC_API_BASE_URL=https://helpin-platform-core-production.up.railway.app
```

For Android builds, EAS reads this value from the selected profile in
`artifacts/mobile/eas.json` and embeds it in the APK. Keep it aligned with
`API_PUBLIC_URL` in the backend deployment.

Operational note:

Do not commit `.env`, secrets, Cloudinary API secrets, tokens, database dumps, local target artifacts, or private operational datasets.

## Running Locally

Requirements:

- Rust toolchain
- Docker
- Docker Compose
- PostgreSQL
- Redis
- Python 3.11+
- pnpm for the mobile workspace

Start infrastructure:

```bash
cd backend
cp .env.example .env
docker compose up -d
```

Run backend:

```bash
cargo run
```

Run tests:

```bash
cargo fmt --check
cargo test
```

Run mobile type contract:

```bash
cd ../client
pnpm --filter zoohelp-mobile run typecheck
```

## Repository Layout

```text
backend/
  Cargo.toml
  docker-compose.yml
  migrations/
    0001_init.sql
    ...
    0018_rescue_fanout_progressive.sql
  src/
    main.rs
    config.rs
    domain.rs
    error.rs
    state.rs
    routes/
      auth.rs
      chat.rs
      donations.rs
      feed.rs
      geo.rs
      media.rs
      notifications.rs
      ongs.rs
      posts.rs
      rescue.rs
      search.rs
      support.rs
      trust.rs
    services/
      auth.rs
      fraud.rs
      geo.rs
      notifications.rs
      rescue_fanout.rs
      rate_limit.rs
      trust.rs
  python-workers/
    app/
      main.py
    requirements.txt
```

## Current Boundaries

This backend is production-shaped, but not yet fully production-complete.

Strong current surfaces:

- Rust Axum API structure
- mobile/backend contract alignment
- auth/register contract
- feed/post/search/ONG/support/donation routes
- media upload-intent contract
- chat HTTP and WebSocket route surface
- geospatial rescue alert modeling
- emergency coordinate validation
- notification subscription and alert preview contracts
- tests for the key frontend/backend paths

Known hardening gaps before real public scale:

- remove any remaining public seed/mock fallback paths and prove critical endpoints are PostgreSQL-backed
- move realtime/domain event delivery from plain NATS pub/sub to durable JetStream/Kafka consumers where replay is required
- add immediate access-token invalidation for banned, deleted, or compromised accounts
- deliver push notifications through FCM/APNs workers
- enforce production rate limits and abuse controls
- complete durable moderation and report review flows
- publish measured benchmark reports for PostgreSQL, Redis, queue, upload, WebSocket, and push delivery paths
- run and document API restart plus worker restart tests for chat, rescue sessions, push jobs, and `Estou indo`
- add production observability dashboards and alerting

## Production Intent

ZooHelp Hybrid Core is intended to become a global animal rescue coordination backend.

The strategic direction is narrow and operational:

- fast rescue post creation
- real geolocation
- nearby helper discovery
- trusted community coordination
- NGO operational profiles
- chat-based response
- donation and support infrastructure
- AI-assisted moderation and fraud prevention

The operating thesis is:

`simple mobile action -> reliable backend coordination -> nearby human response -> measurable animal impact`
