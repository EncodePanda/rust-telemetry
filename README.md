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
