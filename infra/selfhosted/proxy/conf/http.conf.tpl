# Rendered by proxy-entrypoint. Markers of the form @@NAME@@ are replaced; everything else is
# literal nginx configuration.

# Container health check only: listens on the container's own loopback address, is never
# published and cannot be reached from another container.
server {
    listen 127.0.0.1:8081;
    access_log off;
    location = /proxy-healthz {
        default_type text/plain;
        return 200 "ok\n";
    }
}

# Plain HTTP: everything goes to the configured public origin, except the ACME HTTP-01 challenge
# (only in `acme` mode). The target is the configured origin, never the request's Host header.
server {
    listen 8080 default_server;
    server_name _;
    access_log /dev/stdout dynodoc_app_safe;
@@ACME_LOCATION@@
    location / {
        return 301 @@ORIGIN@@$request_uri;
    }
}
