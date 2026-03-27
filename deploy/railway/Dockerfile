FROM ghcr.io/zingerlittlebee/serverbee-server:latest

# Trust Railway's internal reverse proxy so X-Forwarded-For is respected
# (required for correct rate limiting and audit log IP detection)
ENV SERVERBEE_SERVER__TRUSTED_PROXIES='["10.0.0.0/8","172.16.0.0/12","192.168.0.0/16"]'

# Railway assigns a dynamic PORT — bridge it to SERVERBEE_SERVER__LISTEN
CMD ["sh", "-c", "SERVERBEE_SERVER__LISTEN=0.0.0.0:${PORT:-9527} exec serverbee-server"]
