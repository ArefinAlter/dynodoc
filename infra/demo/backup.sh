#!/bin/sh
set -eu
umask 077
cd /opt/dynodoc-demo
mkdir -p backups
backup_file="backups/dynodoc-$(date -u +%Y%m%dT%H%M%SZ).dump"
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml exec -T postgres pg_dump -U dynodoc -d dynodoc -Fc > "$backup_file.partial"
# Refuse to promote an incomplete archive.
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml exec -T postgres pg_restore --list < "$backup_file.partial" > /dev/null
mv "$backup_file.partial" "$backup_file"
sha256sum "$backup_file" > "$backup_file.sha256"
# Retention only touches this application's dated backup files.
find backups -maxdepth 1 -name 'dynodoc-*.dump*' -mtime +14 -delete
