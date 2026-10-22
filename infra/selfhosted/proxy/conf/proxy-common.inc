# Request headers for every proxied location. The proxy is the edge: the client address is
# what this connection says, not what the client claims in X-Forwarded-For.
proxy_http_version 1.1;
proxy_set_header Host $http_host;
proxy_set_header X-Forwarded-Proto $scheme;
proxy_set_header X-Forwarded-For $remote_addr;
proxy_set_header Upgrade $http_upgrade;
proxy_set_header Connection $dynodoc_connection_header;
