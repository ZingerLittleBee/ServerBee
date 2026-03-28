FROM ghcr.io/zingerlittlebee/serverbee-server:latest

# ── Server ──────────────────────────────────────────────────────
# Data directory — mount a Railway volume at /data to persist across deploys
ENV SERVERBEE_SERVER__DATA_DIR=/data
# Trust Railway's internal reverse proxy for correct IP detection
ENV SERVERBEE_SERVER__TRUSTED_PROXIES='["10.0.0.0/8","172.16.0.0/12","192.168.0.0/16"]'

# ── Admin (Required) ───────────────────────────────────────────
# Default admin username (created on first startup)
ENV SERVERBEE_ADMIN__USERNAME=admin
# Admin password — REQUIRED: set via Railway Variables before first deploy
# ENV SERVERBEE_ADMIN__PASSWORD=

# ── Database ────────────────────────────────────────────────────
# SQLite file path (relative to data_dir)
ENV SERVERBEE_DATABASE__PATH=serverbee.db
# Maximum database connection pool size
ENV SERVERBEE_DATABASE__MAX_CONNECTIONS=10

# ── Authentication ──────────────────────────────────────────────
# Session token TTL in seconds (default 24h)
ENV SERVERBEE_AUTH__SESSION_TTL=86400
# Key for agent auto-registration (empty = auto-generate on startup)
ENV SERVERBEE_AUTH__AUTO_DISCOVERY_KEY=
# Set Secure flag on session cookies (set false for HTTP-only)
ENV SERVERBEE_AUTH__SECURE_COOKIE=true

# ── Data Retention (days) ───────────────────────────────────────
# Raw metric records
ENV SERVERBEE_RETENTION__RECORDS_DAYS=7
# Hourly aggregated records
ENV SERVERBEE_RETENTION__RECORDS_HOURLY_DAYS=90
# GPU metric records
ENV SERVERBEE_RETENTION__GPU_RECORDS_DAYS=7
# Ping probe records
ENV SERVERBEE_RETENTION__PING_RECORDS_DAYS=7
# Raw network probe records
ENV SERVERBEE_RETENTION__NETWORK_PROBE_DAYS=7
# Hourly network probe aggregates
ENV SERVERBEE_RETENTION__NETWORK_PROBE_HOURLY_DAYS=90
# Audit log retention
ENV SERVERBEE_RETENTION__AUDIT_LOGS_DAYS=180
# Traffic hourly records
ENV SERVERBEE_RETENTION__TRAFFIC_HOURLY_DAYS=7
# Traffic daily records
ENV SERVERBEE_RETENTION__TRAFFIC_DAILY_DAYS=400
# Task results
ENV SERVERBEE_RETENTION__TASK_RESULTS_DAYS=7
# Docker event records
ENV SERVERBEE_RETENTION__DOCKER_EVENTS_DAYS=7
# Service monitor records
ENV SERVERBEE_RETENTION__SERVICE_MONITOR_DAYS=30

# ── Scheduler ───────────────────────────────────────────────────
# Timezone for daily traffic aggregation and cron scheduling (e.g. Asia/Shanghai)
ENV SERVERBEE_SCHEDULER__TIMEZONE=UTC

# ── Rate Limiting ───────────────────────────────────────────────
# Max login attempts per IP within 15-minute window
ENV SERVERBEE_RATE_LIMIT__LOGIN_MAX=5
# Max agent registrations per IP within 15-minute window
ENV SERVERBEE_RATE_LIMIT__REGISTER_MAX=3

# ── OAuth (Optional) ───────────────────────────────────────────
# Public base URL for OAuth callbacks (e.g. https://monitor.example.com)
# ENV SERVERBEE_OAUTH__BASE_URL=
# Auto-create user accounts on first OAuth login
# ENV SERVERBEE_OAUTH__ALLOW_REGISTRATION=false
# GitHub OAuth App credentials
# ENV SERVERBEE_OAUTH__GITHUB__CLIENT_ID=
# ENV SERVERBEE_OAUTH__GITHUB__CLIENT_SECRET=
# Google OAuth credentials
# ENV SERVERBEE_OAUTH__GOOGLE__CLIENT_ID=
# ENV SERVERBEE_OAUTH__GOOGLE__CLIENT_SECRET=
# OIDC provider credentials
# ENV SERVERBEE_OAUTH__OIDC__ISSUER_URL=
# ENV SERVERBEE_OAUTH__OIDC__CLIENT_ID=
# ENV SERVERBEE_OAUTH__OIDC__CLIENT_SECRET=

# ── GeoIP (Optional) ───────────────────────────────────────────
# Path to MaxMind GeoLite2-City.mmdb file (non-empty path enables GeoIP)
# ENV SERVERBEE_GEOIP__MMDB_PATH=

# ── Logging ─────────────────────────────────────────────────────
# Log level: trace, debug, info, warn, error
ENV SERVERBEE_LOG__LEVEL=info
# Log file path (empty = stdout only)
# ENV SERVERBEE_LOG__FILE=

# ── Upgrade ─────────────────────────────────────────────────────
# Base URL for agent upgrade release assets
ENV SERVERBEE_UPGRADE__RELEASE_BASE_URL=https://github.com/ZingerLittleBee/ServerBee/releases

# Railway assigns a dynamic PORT — bridge it to SERVERBEE_SERVER__LISTEN
CMD ["sh", "-c", "SERVERBEE_SERVER__LISTEN=0.0.0.0:${PORT:-9527} exec serverbee-server"]
