# rust-telemetry

Demo project showing how to add observability to a Rust API using OpenTelemetry, Jaeger, Prometheus, and Grafana.

Built with Axum 0.8, PostgreSQL (sqlx), and a full Docker Compose stack.

## Starting and stopping

Start all services (builds the app image and starts Postgres, OTel Collector, Jaeger, Prometheus, and Grafana):

```sh
./scripts/start.sh
```

Stop everything:

```sh
./scripts/stop.sh
```

Follow the app logs:

```sh
./scripts/logs.sh
```

## Using the API

Create a user:

```sh
./scripts/create-user.sh Alice Smith
```

List all users:

```sh
./scripts/get-users.sh
```

Or use curl directly:

```sh
curl http://localhost:3000/users                                              # GET all users
curl http://localhost:3000/user/{id}                                          # GET user by UUID
curl -X POST http://localhost:3000/user -H "Content-Type: application/json" \
  -d '{"first_name":"Alice","last_name":"Smith"}'                             # POST create user
```

## Observability UIs

| Service    | URL                        | What you'll find                                         |
|------------|----------------------------|----------------------------------------------------------|
| Jaeger     | http://localhost:16686      | Distributed traces — select service "rust-telemetry"     |
| Prometheus | http://localhost:9090       | Metrics — check Targets page to confirm collector is UP  |
| Grafana    | http://localhost:4000       | Dashboards — Prometheus and Jaeger datasources are pre-configured |

Grafana has anonymous admin access enabled, so no login is required.

## Project structure

```
src/
  main.rs       — Entry point: init telemetry, DB pool, migrations, start server
  otel.rs       — OTLP/gRPC exporter setup for traces and metrics
  db.rs         — PgPool creation
  routes.rs     — Axum router with OTel middleware layers
  handlers.rs   — HTTP handlers with #[instrument] and DB child spans
  models.rs     — User and CreateUserRequest structs
  state.rs      — AppState (DB pool + metrics counter)
```

## Infrastructure (Docker Compose)

| Service        | Port  | Purpose                          |
|----------------|-------|----------------------------------|
| app            | 3000  | This Rust API                    |
| postgres       | 5432  | PostgreSQL 17                    |
| otel-collector | 4317  | OTLP gRPC receiver               |
| jaeger         | 16686 | Trace visualization UI           |
| prometheus     | 9090  | Metrics scraping & UI            |
| grafana        | 4000  | Dashboards (no login required)   |

## How OpenTelemetry works in Rust

### The big picture

Before diving into Rust code, it helps to understand the data flow:

```
  Axum app                   OTel Collector              Backends
 ┌────────┐   OTLP/gRPC    ┌──────────────┐
 │ request │ ──────────────>│  receives    │───> Jaeger   (traces)
 │ spans & │   port 4317    │  batches     │───> Prometheus (metrics)
 │ metrics │                │  routes      │───> debug log
 └────────┘                 └──────────────┘
```

The app doesn't know about Jaeger or Prometheus. It speaks one protocol (OTLP) to one
destination (the collector). The collector then fans out to whatever backends you configure.
This is a key design choice — your application code never couples to a specific
observability vendor.

### The crate ecosystem

OpenTelemetry in Rust involves a few crates that each handle a distinct layer. This can be
confusing at first because there are more crates than you might expect, so let's clarify
what each one does:

```toml
# The tracing ecosystem (Rust-native structured logging)
tracing                    = "0.1"
tracing-subscriber         = { version = "0.3", features = ["env-filter"] }

# The OpenTelemetry SDK (vendor-neutral telemetry API + implementation)
opentelemetry              = "0.31"
opentelemetry_sdk          = { version = "0.31", features = ["rt-tokio"] }
opentelemetry-otlp         = { version = "0.31", features = ["grpc-tonic", "metrics"] }

# The bridge between the two worlds
tracing-opentelemetry      = "0.32"

# Axum-specific middleware that auto-creates spans for HTTP requests
axum-tracing-opentelemetry = "0.33"

# Error handling
anyhow                     = "1"
```

Think of it as three layers:

1. **`tracing`** is the Rust ecosystem's standard for structured, span-based diagnostics.
   Your code (and library code) creates spans and events through `tracing`.
2. **`opentelemetry` + `opentelemetry_sdk` + `opentelemetry-otlp`** is the OpenTelemetry
   SDK that knows how to batch spans and ship them over gRPC to a collector.
3. **`tracing-opentelemetry`** is the bridge — it takes spans created by `tracing` and
   forwards them to the OpenTelemetry SDK for export.

This layering means libraries that already use `tracing` (like `sqlx`, `hyper`, `tower`)
automatically participate in your traces without knowing OpenTelemetry exists.

### Initializing the pipeline (`src/otel.rs`)

The telemetry setup creates both a trace provider and a metrics provider:

```rust
pub fn init_providers() -> anyhow::Result<Providers> {
    let resource = Resource::builder().with_service_name("rust-telemetry").build();

    // 1. Build a span exporter that sends traces over gRPC
    let span_exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .context("Failed to create OTLP span exporter")?;

    // 2. Create a TracerProvider that batches spans and exports them
    let tracer = SdkTracerProvider::builder()
        .with_batch_exporter(span_exporter)
        .with_resource(resource.clone())
        .build();

    // 3. Build a metric exporter
    let metric_exporter = MetricExporter::builder()
        .with_tonic()
        .build()
        .context("Failed to create OTLP metric exporter")?;

    // 4. Create a MeterProvider for periodic metric export
    let meter = SdkMeterProvider::builder()
        .with_periodic_exporter(metric_exporter)
        .with_resource(resource)
        .build();

    Ok(Providers { tracer, meter })
}
```

### Wiring it into `main.rs`

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let providers = otel::init_providers()
        .context("Failed to initialize telemetry providers")?;

    // Build the bridge layer: tracing spans → OpenTelemetry spans
    let tracer = providers.tracer.tracer("rust-telemetry");
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // Assemble the tracing subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())  // filter by RUST_LOG
        .with(tracing_subscriber::fmt::layer()
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE))  // console output
        .with(otel_layer)                      // send to OTel
        .init();

    let database_url = env::var("DATABASE_URL").context("DATABASE_URL must be set")?;
    let pool = db::create_pool(&database_url).await?;
    sqlx::migrate!("./migrations").run(&pool).await.context("Failed to run migrations")?;

    // ... set up metrics, state, router, start server ...

    let _ = providers.tracer.shutdown();
    let _ = providers.meter.shutdown();
    Ok(())
}
```

Key points:

1. **Init first.** Telemetry must initialize before anything else so that all subsequent
   `tracing` calls (including those from libraries like `sqlx`) are captured.
2. **Shutdown last.** The batch exporter runs in a background Tokio task. If the process
   exits abruptly, pending spans are lost. `shutdown()` flushes remaining data.
   The graceful shutdown handler (`with_graceful_shutdown`) gives in-flight requests
   time to complete before we reach this point.
3. **Error propagation.** `main()` returns `anyhow::Result<()>` so startup failures
   produce clear error messages instead of panics.

### Automatic HTTP spans via middleware (`src/routes.rs`)

This is where the real payoff is. Two middleware layers, two lines of code, and every
HTTP request gets a full trace — with zero changes to handlers:

```rust
pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/user/{id}", get(get_user))
        .route("/users", get(get_users))
        .route("/user", post(add_user))
        .layer(OtelInResponseLayer::default())  // adds traceparent to responses
        .layer(OtelAxumLayer::default())         // creates a span per request
        .with_state(state)
}
```

**`OtelAxumLayer`** intercepts every inbound request and creates a `tracing` span with
fields following the OpenTelemetry semantic conventions: `http.request.method`,
`http.route`, `url.path`, `http.response.status_code`, and more. It also extracts the
W3C `traceparent` header from incoming requests, so if an upstream service started a
trace, this service continues it rather than starting a new one.

**`OtelInResponseLayer`** does the reverse: it injects the `traceparent` header into the
HTTP response so that clients (or browser dev tools) can correlate their request with the
server-side trace.

Layer order matters in Axum — layers are executed bottom-to-top. So `OtelAxumLayer` runs
first (creates the span), then the route handler executes inside that span, then
`OtelInResponseLayer` runs last (injects the trace ID into the response).

### Manual spans in handlers (`src/handlers.rs`)

On top of the automatic HTTP spans, handlers use `#[instrument]` to create parent spans
and `.instrument()` on DB queries to create child spans:

```rust
#[instrument(skip(state))]
pub async fn get_users(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let rows = sqlx::query("SELECT id, first_name, last_name FROM users")
        .fetch_all(&state.db)
        .instrument(tracing::info_span!("db.query", db.statement = "SELECT users"))
        .await
        .context("Failed to fetch users")?;

    // ...
    Ok(Json(users))
}
```

Handlers return `Result<impl IntoResponse, AppError>` where `AppError` wraps
`anyhow::Error` and implements `IntoResponse` (returning 500 with the error message).
This means DB errors return proper HTTP responses instead of panicking.

### Custom metrics

The app defines custom metrics alongside traces:

- **`app.users.created`** — a counter incremented each time a user is created
- **`db.client.connections.pool_size`** — an observable gauge reporting the current
  connection pool size

These are exported through the same OTLP pipeline and appear in Prometheus/Grafana.

### The OTel Collector as a decoupling layer

You might wonder: why not send spans directly from the app to Jaeger? The OTel Collector
in between gives you:

- **Decoupling.** The app speaks OTLP. If you swap Jaeger for Zipkin tomorrow, you change
  the collector config — not your Rust code.
- **Batching and buffering.** The collector can absorb bursts and retry failed exports.
- **Fan-out.** One input stream goes to multiple backends. In our setup, traces go to
  Jaeger while metrics go to Prometheus, all from the same OTLP input.
- **Span metrics.** The collector's `spanmetrics` connector derives RED metrics
  (rate, errors, duration) from traces automatically — no extra instrumentation needed.

The collector also scrapes host-level metrics (CPU, memory, disk, network) via the
`hostmetrics` receiver and routes them to Prometheus alongside application metrics.

The collector config (`otel-collector-config.yml`) defines this routing:

```yaml
service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [otlp/jaeger, spanmetrics, debug]
    metrics:
      receivers: [otlp, spanmetrics, hostmetrics]
      processors: [batch]
      exporters: [prometheus, debug]
```

The `debug` exporter prints to the collector's own stdout, which is useful for verifying
that data is flowing (`docker compose logs otel-collector`).

### The RUST_LOG gotcha

There's one non-obvious detail. The `axum-tracing-opentelemetry` middleware creates spans
at `TRACE` level under the target `otel::tracing`. The default `RUST_LOG=info` will filter
these out entirely, and you'll see a warning:

```
WARN axum_tracing_opentelemetry: can not set parent trace_id to span error=SpanDisabled
```

The fix is to selectively enable that target in `RUST_LOG`:

```
RUST_LOG=info,otel::tracing=trace,otel=debug
```

This keeps your general log output at `info` while allowing the OTel middleware's spans
through. This is configured in `docker-compose.yml` so it works out of the box.

### Tracking requests across services with W3C traceparent

The middleware already handles W3C trace context propagation. This section shows
how to use it in practice — linking spans from multiple HTTP calls into a single
distributed trace.

#### The traceparent header format

Every `traceparent` header follows this structure:

```
traceparent: 00-<trace-id>-<span-id>-<flags>
              │   32 hex     16 hex    2 hex
              version
```

For example: `00-4bf92f3577b6a27ff35a6d911c5b9b4e-d75597dee50b0cac-01`

- **trace-id** — shared by every span in the distributed call chain
- **span-id** — unique to this particular hop
- **flags** — `01` means "sampled" (will be recorded)

The trace-id is the glue. As long as every service reads and forwards it, all
spans appear in a single trace in Jaeger.

#### Try it: linking two requests into one trace

Make a request without a traceparent — the service creates a new trace and
returns the traceparent in the response:

```sh
curl -v http://localhost:3000/users
# Response header: traceparent: 00-ab38f4a2c1904f67b58e4e1d3e2faa01-7c9f2b4a1d3e5f08-01
```

Now pass that same trace-id into a second request (with a different span-id):

```sh
curl -v http://localhost:3000/user \
  -H "Content-Type: application/json" \
  -H "traceparent: 00-ab38f4a2c1904f67b58e4e1d3e2faa01-aaaaaaaaaaaaaaaa-01" \
  -d '{"first_name":"Bob","last_name":"Jones"}'
```

Open Jaeger at http://localhost:16686, search for service `rust-telemetry`, and
you'll find a single trace containing spans from both calls — linked by the
shared trace-id.

#### How this works in a multi-service architecture

In production, each service forwards the traceparent to downstream calls:

```
Browser/Client
    │
    │  POST /order   (no traceparent — new trace created)
    ▼
┌───────────┐
│ Order API  │  receives traceparent: 00-<TRACE_A>-<span1>-01
└─────┬─────┘
      │  GET /users   (forwards same trace-id, new span-id)
      │  traceparent: 00-<TRACE_A>-<span2>-01
      ▼
┌───────────┐
│ User API   │  OtelAxumLayer reads the header, joins TRACE_A
└───────────┘
```

Each service:
1. **Reads** the incoming `traceparent` to join the existing trace
   (`OtelAxumLayer` does this automatically)
2. Creates its own spans as children of that context
3. **Forwards** a new `traceparent` (same trace-id, new span-id) to any
   downstream HTTP call

#### Propagating context in outgoing HTTP calls

The middleware handles inbound propagation. For outbound calls (e.g., calling
another service from a handler), you need to inject the traceparent into your
outgoing request headers. Here's how with `reqwest`:

```rust
use opentelemetry::global;
use opentelemetry::propagation::Injector;
use tracing_opentelemetry::OpenTelemetrySpanExt;

struct HeaderInjector<'a>(&'a mut reqwest::header::HeaderMap);

impl<'a> Injector for HeaderInjector<'a> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = reqwest::header::HeaderName::from_bytes(key.as_bytes()) {
            if let Ok(val) = reqwest::header::HeaderValue::from_str(&value) {
                self.0.insert(name, val);
            }
        }
    }
}

// Inside an instrumented handler:
let mut headers = reqwest::header::HeaderMap::new();
let cx = tracing::Span::current().context();
global::get_text_map_propagator(|propagator| {
    propagator.inject_context(&cx, &mut HeaderInjector(&mut headers));
});

// headers now contains traceparent — use it in your outgoing request
let resp = reqwest::Client::new()
    .get("http://other-service/endpoint")
    .headers(headers)
    .send()
    .await?;
```

This completes the chain: `OtelAxumLayer` handles inbound context, and
`inject_context` handles outbound context. Together, every hop in a distributed
call appears as part of a single trace in Jaeger.
