# rust-telemetry

Axum 0.8 REST API with PostgreSQL and OpenTelemetry observability.

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

## Observability UIs

| Service    | URL                        | What you'll find                                         |
|------------|----------------------------|----------------------------------------------------------|
| Jaeger     | http://localhost:16686      | Distributed traces — select service "rust-telemetry"     |
| Prometheus | http://localhost:9090       | Metrics — check Targets page to confirm collector is UP  |
| Grafana    | http://localhost:4000       | Dashboards — Prometheus and Jaeger datasources are pre-configured |

Grafana has anonymous admin access enabled, so no login is required.

## How OpenTelemetry works in Rust

If you've ever stared at logs trying to figure out why a request was slow, you already
understand the problem OpenTelemetry solves. Instead of sprinkling `println!` statements
everywhere and hoping for the best, OpenTelemetry gives your application structured
observability: traces that follow a request from start to finish, with timing and metadata
attached automatically.

This section walks through exactly how this project wires up OpenTelemetry in an Axum
application. No handler code was touched — every trace you see in Jaeger comes from
infrastructure code and middleware alone.

### The big picture

Before diving into Rust code, it helps to understand the data flow:

```
  Axum app                   OTel Collector              Backends
 ┌────────┐   OTLP/gRPC    ┌──────────────┐
 │ request │ ──────────────>│  receives    │───> Jaeger   (traces)
 │ spans   │   port 4317    │  batches     │───> Prometheus (metrics)
 └────────┘                 │  routes      │───> debug log
                            └──────────────┘
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
opentelemetry-otlp         = { version = "0.31", features = ["grpc-tonic"] }

# The bridge between the two worlds
tracing-opentelemetry      = "0.32"

# Axum-specific middleware that auto-creates spans for HTTP requests
axum-tracing-opentelemetry = "0.33"
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

### Initializing the pipeline (`src/telemetry.rs`)

All the setup happens in one function:

```rust
pub fn init_telemetry() -> SdkTracerProvider {
    // 1. Build an exporter that sends spans over gRPC
    let exporter = SpanExporter::builder()
        .with_tonic()
        .build()
        .expect("Failed to create OTLP exporter");

    // 2. Create a TracerProvider that batches spans and exports them
    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(
            Resource::builder()
                .with_service_name("rust-telemetry")
                .build(),
        )
        .build();

    // 3. Get a tracer from the provider
    let tracer = provider.tracer("rust-telemetry");

    // 4. Build the bridge layer
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // 5. Assemble the tracing subscriber
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())  // filter by RUST_LOG
        .with(tracing_subscriber::fmt::layer()) // console output
        .with(otel_layer)                       // send to OTel
        .init();

    provider
}
```

Let's unpack this step by step.

**Step 1 — The exporter.** `SpanExporter::builder().with_tonic()` creates a gRPC client
that speaks the OTLP protocol. It reads the `OTEL_EXPORTER_OTLP_ENDPOINT` environment
variable automatically (a standard OTel convention), so there's zero hardcoded config
in the code. In our `docker-compose.yml` this points to `http://otel-collector:4317`.

**Step 2 — The provider.** `SdkTracerProvider` is the heart of the SDK. It owns a
background task that collects finished spans into batches and flushes them to the exporter
periodically. The `Resource` attached here sets the `service.name` attribute — this is
how Jaeger knows to group all spans under "rust-telemetry".

**Step 3 — The tracer.** A tracer is a lightweight handle you use to create spans. We get
one from the provider and hand it to the bridge layer.

**Step 4 — The bridge.** `tracing_opentelemetry::layer()` is where the two worlds connect.
This layer subscribes to the `tracing` ecosystem and, for every `tracing::Span` it sees,
creates a corresponding OpenTelemetry span and forwards it to the SDK for export.

**Step 5 — The subscriber.** `tracing_subscriber::registry()` is a composable subscriber.
We stack three layers on it:
- `EnvFilter` reads `RUST_LOG` and decides which spans/events are enabled.
- `fmt::layer()` prints human-readable logs to stdout (what you see with `docker compose logs`).
- The OTel layer from step 4.

Every `tracing` span or event now flows to *both* the console and the OpenTelemetry
collector simultaneously.

The function returns the `SdkTracerProvider` so that `main()` can shut it down cleanly
later — this is important for flushing any remaining spans on exit.

### Wiring it into `main.rs`

```rust
#[tokio::main]
async fn main() {
    let tracer_provider = telemetry::init_telemetry(); // <-- first thing

    // ... database setup, migrations ...

    tracing::info!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");

    let _ = tracer_provider.shutdown(); // <-- last thing
}
```

Two things matter here:

1. **Init first.** `init_telemetry()` must run before anything else so that all subsequent
   `tracing` calls (including those from libraries like `sqlx`) are captured.
2. **Shutdown last.** The batch exporter runs in a background Tokio task. If the process
   exits abruptly, pending spans are lost. `tracer_provider.shutdown()` flushes the
   remaining batch. The graceful shutdown handler (`with_graceful_shutdown`) gives
   in-flight requests time to complete before we reach this point.

### Automatic HTTP spans via middleware (`src/routes.rs`)

This is where the real payoff is. Two middleware layers, two lines of code, and every
HTTP request gets a full trace — with zero changes to handlers:

```rust
pub fn create_router(pool: PgPool) -> Router {
    Router::new()
        .route("/user/{id}", get(get_user))
        .route("/users", get(get_users))
        .route("/user", post(add_user))
        .layer(OtelInResponseLayer::default())  // adds traceparent to responses
        .layer(OtelAxumLayer::default())         // creates a span per request
        .with_state(pool)
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

The result in Jaeger looks like this:

```
Service: rust-telemetry
  └─ POST /user   [201]  12ms
  └─ GET  /users  [200]   3ms
```

Each span carries structured attributes like `http.route = /user` and
`http.response.status_code = 201` that you can filter and search on.

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

### The collector as a decoupling layer

You might wonder: why not send spans directly from the app to Jaeger? The OTel Collector
in between gives you a few things:

- **Decoupling.** The app speaks OTLP. If you swap Jaeger for Zipkin tomorrow, you change
  the collector config — not your Rust code.
- **Batching and buffering.** The collector can absorb bursts and retry failed exports.
- **Fan-out.** One input stream goes to multiple backends. In our setup, traces go to
  Jaeger while metrics go to Prometheus, all from the same OTLP input.

The collector config (`otel-collector-config.yml`) defines this routing:

```yaml
service:
  pipelines:
    traces:
      receivers: [otlp]
      processors: [batch]
      exporters: [otlp/jaeger, debug]
    metrics:
      receivers: [otlp]
      processors: [batch]
      exporters: [prometheus, debug]
```

The `debug` exporter prints to the collector's own stdout, which is useful for verifying
that data is flowing (`docker compose logs otel-collector`).

### What you get without writing a single `#[instrument]`

With just the middleware and the telemetry init code, every HTTP request automatically
produces a span in Jaeger with:

- HTTP method and route
- Response status code
- Request duration
- URL path, scheme, and server address
- W3C trace context propagation

The `handlers.rs`, `models.rs`, and `db.rs` files have zero tracing code. Everything
described above comes from infrastructure wiring alone. Adding manual spans to specific
functions (with `#[tracing::instrument]`) is the natural next step when you need
finer-grained visibility into what's happening *inside* a request.

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
