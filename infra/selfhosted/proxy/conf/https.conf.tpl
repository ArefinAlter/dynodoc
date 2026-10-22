# Rendered by proxy-entrypoint. Markers of the form @@NAME@@ are replaced; everything else is
# literal nginx configuration. Derived from infra/demo/nginx-app.conf (same timeouts, buffers,
# rate limits, header handling); the policy maps are in maps.conf.

# Docker's embedded DNS, so a restarted web container is found again without reloading nginx.
resolver 127.0.0.11 valid=10s ipv6=off;

# Only the configured names are served. Any other Host header (an address typed by mistake, a
# forged Host) gets 421 instead of the application.
map $host $dynodoc_known_host {
    default 0;
@@HOST_MAP@@
}

server {
    listen 8443 ssl default_server;
    http2 on;
    server_name @@SERVER_NAMES@@;
    server_tokens off;

    ssl_certificate @@CERT@@;
    ssl_certificate_key @@KEY@@;
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305;
    ssl_prefer_server_ciphers off;
    ssl_session_cache shared:dynodoc_ssl:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    client_max_body_size 16m;
    client_header_timeout 15s;
    client_body_timeout 30s;
    send_timeout 30s;
    # Signed-in SvelteKit pages include asset preload Link headers larger than
    # Nginx's default 4 KiB header buffer; include room for session rotation too.
    proxy_buffer_size 16k;
    proxy_buffers 8 16k;
    proxy_busy_buffers_size 32k;

    access_log /dev/stdout dynodoc_app_safe;
    proxy_hide_header Content-Security-Policy;
    proxy_hide_header Strict-Transport-Security;
    proxy_hide_header X-Frame-Options;
    proxy_hide_header X-Content-Type-Options;
    proxy_hide_header Referrer-Policy;
    add_header Content-Security-Policy $dynodoc_app_csp always;
    add_header Strict-Transport-Security "max-age=@@HSTS_MAX_AGE@@" always;
    add_header X-Content-Type-Options nosniff always;
    add_header Referrer-Policy no-referrer always;
    add_header X-Frame-Options $dynodoc_app_frame_options always;
@@TLS_MODE_HEADER@@

    default_type text/plain;
    if ($dynodoc_known_host = 0) {
        return 421 "Misdirected request: this server answers only to its configured name.\n";
    }

    set $dynodoc_web http://web:3000;

    location = /telemetry {
        client_max_body_size 2k;
        limit_req zone=dynodoc_metrics burst=30 nodelay;
        limit_req_status 429;
        include /etc/nginx/dynodoc/proxy-common.inc;
        proxy_pass $dynodoc_web;
    }
    location /auth/ {
        limit_req zone=dynodoc_app_auth burst=60 nodelay;
        limit_req_status 429;
        include /etc/nginx/dynodoc/proxy-common.inc;
        proxy_pass $dynodoc_web;
    }
    # Everything else, including the server-sent event streams (no buffering, long read timeout)
    # and WebSocket upgrades (Upgrade and Connection are passed on).
    location / {
        include /etc/nginx/dynodoc/proxy-common.inc;
        proxy_buffering off;
        proxy_read_timeout 3600s;
        proxy_pass $dynodoc_web;
    }
}
