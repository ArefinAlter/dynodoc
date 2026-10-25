# The pre-alpha service and provisioner (no npm dependencies: Node's own http, crypto and sqlite).
FROM node:24-bookworm-slim
WORKDIR /app
COPY service/ ./
RUN mkdir -p /data /run/prealpha && chown node:node /data /run/prealpha
USER node
ENV NODE_ENV=production NODE_NO_WARNINGS=1
CMD ["node", "svc.mjs"]
