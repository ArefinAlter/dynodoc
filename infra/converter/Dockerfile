FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends libreoffice-writer python3 fonts-liberation && rm -rf /var/lib/apt/lists/* && useradd --uid 10001 --create-home converter
COPY infra/converter/server.py /app/server.py
USER converter
ENV HOME=/tmp
EXPOSE 8090
CMD ["python3", "/app/server.py"]
