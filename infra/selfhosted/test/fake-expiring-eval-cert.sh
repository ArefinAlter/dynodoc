#!/bin/sh
# Test helper, runs inside the proxy container: replaces the evaluation certificate with one that
# has only five days left, the way a certificate looks shortly before it must be renewed. The
# proxy's watcher should notice, make a new 90-day certificate and reload without dropping
# connections.
set -eu
d=/var/lib/dynodoc-tls/evaluation
n=$d/v-99990101000000
rm -rf "$n"
mkdir "$n"
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 5 \
  -keyout "$n/privkey.pem" -out "$n/fullchain.pem" -subj '/CN=localhost' \
  -addext 'subjectAltName=DNS:localhost' 2>/dev/null
ln -s v-99990101000000 "$d/current.tmp"
mv -T -f "$d/current.tmp" "$d/current"
openssl x509 -in "$d/current/fullchain.pem" -noout -enddate
