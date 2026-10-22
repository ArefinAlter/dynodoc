#!/bin/sh
# Shared functions of the Dynodoc bundled proxy: settings, certificate checks, the evaluation
# certificate, and rendering the nginx configuration. Sourced by proxy-entrypoint.
# POSIX sh (busybox ash) and OpenSSL 3 only.

STATE_DIR=${DYNODOC_TLS_STATE_DIR:-/var/lib/dynodoc-tls}
PROVIDED_DIR=${DYNODOC_TLS_PROVIDED_DIR:-/etc/dynodoc/tls}
RENDER_DIR=${DYNODOC_RENDER_DIR:-/tmp/dynodoc-nginx}
TEMPLATE_DIR=${DYNODOC_TEMPLATE_DIR:-/etc/nginx/dynodoc}
EVAL_RENEW_DAYS=${DYNODOC_EVAL_RENEW_DAYS:-30}
WARN_DAYS=${DYNODOC_TLS_WARN_DAYS:-30}

log() { printf '[dynodoc-proxy] %s\n' "$*"; }
err() { printf '[dynodoc-proxy] ERROR: %s\n' "$*" >&2; }
problems=0
problem() { err "  - $*"; problems=$((problems + 1)); }

# --- settings ---------------------------------------------------------------------------------

# ORIGIN, DYNODOC_ALT_ORIGINS -> ORIGIN_URL, PRIMARY_HOST, ALL_HOSTS (space separated, lowercase)
parse_origin() {
  if [ -z "${ORIGIN:-}" ]; then
    err "ORIGIN is not set. Set it in .env to the address people type, for example https://docs.example.org"
    return 1
  fi
  ORIGIN_URL=$(printf '%s' "$ORIGIN" | tr 'A-Z' 'a-z')
  if ! printf '%s' "$ORIGIN_URL" | grep -Eq '^https://[a-z0-9]([a-z0-9.-]*[a-z0-9])?(:[0-9]{1,5})?$'; then
    err "ORIGIN '$ORIGIN' is not usable. The bundled proxy terminates TLS, so ORIGIN must be https://host or https://host:port (letters, digits, dots and hyphens in the host; no path, no trailing slash, no IPv6 literal)."
    return 1
  fi
  PRIMARY_HOST=${ORIGIN_URL#https://}
  PRIMARY_HOST=${PRIMARY_HOST%%:*}
  ALL_HOSTS=$PRIMARY_HOST
  alt=$(printf '%s' "${DYNODOC_ALT_ORIGINS:-}" | tr 'A-Z,' 'a-z ')
  for o in $alt; do
    if ! printf '%s' "$o" | grep -Eq '^https://[a-z0-9]([a-z0-9.-]*[a-z0-9])?(:[0-9]{1,5})?$'; then
      err "DYNODOC_ALT_ORIGINS entry '$o' is not an https://host[:port] origin."
      return 1
    fi
    h=${o#https://}
    h=${h%%:*}
    case " $ALL_HOSTS " in *" $h "*) ;; *) ALL_HOSTS="$ALL_HOSTS $h" ;; esac
  done
  return 0
}

is_ipv4() { printf '%s' "$1" | grep -Eq '^[0-9]{1,3}(\.[0-9]{1,3}){3}$'; }

# --- certificate checks -----------------------------------------------------------------------

# check_pair LABEL CERT KEY: report every problem with a certificate chain and its key.
# Returns 0 when the pair can be served. Needs ALL_HOSTS. Counts problems in $problems.
check_pair() {
  label=$1 cert=$2 key=$3
  problems=0
  if [ ! -e "$cert" ]; then problem "the certificate file $label is missing ($cert)"; fi
  if [ ! -e "$key" ]; then problem "the private key file for $label is missing ($key)"; fi
  [ "$problems" -eq 0 ] || return 1
  if [ ! -r "$cert" ] || [ ! -s "$cert" ]; then problem "the certificate file $label cannot be read or is empty"; fi
  if [ ! -r "$key" ] || [ ! -s "$key" ]; then problem "the private key file for $label cannot be read or is empty"; fi
  [ "$problems" -eq 0 ] || return 1

  work=$(mktemp -d)
  # Split the chain into c1.pem (should be the server certificate), c2.pem, ...
  awk -v d="$work" '/-----BEGIN CERTIFICATE-----/ { n++; f = sprintf("%s/c%d.pem", d, n) } f { print > f } /-----END CERTIFICATE-----/ { f = "" }' "$cert"
  count=$(ls "$work" | grep -c '^c[0-9]*\.pem$' || true)
  if [ "$count" -eq 0 ]; then
    problem "$label contains no PEM certificate (expected '-----BEGIN CERTIFICATE-----'). Use the full chain in PEM format, server certificate first."
    rm -rf "$work"
    return 1
  fi
  i=1
  while [ "$i" -le "$count" ]; do
    if ! openssl x509 -in "$work/c$i.pem" -noout >/dev/null 2>&1; then
      problem "certificate number $i in $label is damaged and cannot be read"
    fi
    i=$((i + 1))
  done
  [ "$problems" -eq 0 ] || { rm -rf "$work"; return 1; }

  # The key.
  if ! openssl pkey -in "$key" -noout -passin pass: >/dev/null 2>&1; then
    if grep -q 'ENCRYPTED' "$key" 2>/dev/null; then
      problem "the private key is protected by a passphrase, and the proxy cannot ask for it. Remove the passphrase first: openssl pkey -in privkey.pem -out privkey-open.pem"
    else
      problem "the private key file is not a PEM private key the proxy can read (RSA, ECDSA or Ed25519)"
    fi
    rm -rf "$work"
    return 1
  fi
  keysum=$(openssl pkey -in "$key" -pubout -passin pass: 2>/dev/null | openssl sha256 | awk '{print $NF}')
  leafsum=$(openssl x509 -in "$work/c1.pem" -noout -pubkey 2>/dev/null | openssl sha256 | awk '{print $NF}')
  if [ "$keysum" != "$leafsum" ]; then
    found=0
    i=2
    while [ "$i" -le "$count" ]; do
      s=$(openssl x509 -in "$work/c$i.pem" -noout -pubkey 2>/dev/null | openssl sha256 | awk '{print $NF}')
      [ "$s" = "$keysum" ] && found=$i
      i=$((i + 1))
    done
    if [ "$found" -gt 0 ]; then
      problem "the private key belongs to certificate number $found in $label, but the server certificate must come first. Put the server certificate first, then the intermediate certificates (the order your CA's 'fullchain' file uses)."
    else
      problem "the private key does not match the certificate (the first certificate in $label). They were issued separately, or the wrong key file was supplied."
    fi
  fi

  # Dates.
  if ! openssl x509 -in "$work/c1.pem" -noout -checkend 0 >/dev/null 2>&1; then
    problem "the server certificate has expired ($(openssl x509 -in "$work/c1.pem" -noout -enddate | sed 's/^notAfter=//')). Install a renewed certificate."
  else
    if ! openssl x509 -in "$work/c1.pem" -noout -checkend $((WARN_DAYS * 86400)) >/dev/null 2>&1; then
      log "WARNING: the server certificate expires within $WARN_DAYS days ($(openssl x509 -in "$work/c1.pem" -noout -enddate | sed 's/^notAfter=//')). Renew it."
    fi
  fi
  nb=$(openssl x509 -in "$work/c1.pem" -noout -startdate | sed 's/^notBefore=//; s/ GMT$//')
  nbs=$(date -u -D '%b %e %H:%M:%S %Y' -d "$nb" +%s 2>/dev/null || echo 0)
  if [ "$nbs" -gt "$(date -u +%s)" ]; then
    problem "the server certificate is not valid yet (valid from $nb UTC). Check this machine's clock."
  fi

  # Chain order: each certificate must have been issued by the next one.
  i=1
  while [ "$i" -lt "$count" ]; do
    j=$((i + 1))
    iss=$(openssl x509 -in "$work/c$i.pem" -noout -issuer -nameopt RFC2253 | sed 's/^issuer=//')
    sub=$(openssl x509 -in "$work/c$j.pem" -noout -subject -nameopt RFC2253 | sed 's/^subject=//')
    if [ "$iss" != "$sub" ]; then
      problem "certificate number $i in $label was not issued by certificate number $j (issuer '$iss', next subject '$sub'). The chain is out of order or incomplete: server certificate first, then each intermediate, each issued by the one after it."
    elif ! openssl verify -no-CApath -no-CAstore -partial_chain -CAfile "$work/c$j.pem" "$work/c$i.pem" >/dev/null 2>&1; then
      problem "certificate number $i in $label is not signed by certificate number $j, or one of them has expired"
    fi
    i=$((i + 1))
  done
  if [ "$count" -eq 1 ]; then
    self=$(openssl x509 -in "$work/c1.pem" -noout -subject -issuer -nameopt RFC2253 | sed 's/^[a-z]*=//' | sort -u | wc -l)
    if [ "$self" -gt 1 ]; then
      log "WARNING: $label holds only the server certificate. Browsers that cannot fetch the intermediate certificate, Office hosts and engine-cli will refuse the connection. Use the full chain."
    fi
  fi

  # Names.
  for h in $ALL_HOSTS; do
    if is_ipv4 "$h"; then
      out=$(openssl x509 -in "$work/c1.pem" -noout -checkip "$h" 2>&1 || true)
    else
      out=$(openssl x509 -in "$work/c1.pem" -noout -checkhost "$h" 2>&1 || true)
    fi
    case "$out" in
      *"does match certificate"*) ;;
      *)
        names=$(openssl x509 -in "$work/c1.pem" -noout -ext subjectAltName 2>/dev/null | tail -n +2 | tr -d ' \n')
        problem "the server certificate does not cover the name '$h' (ORIGIN). It lists: ${names:-no subject alternative names}. Get a certificate for this name or change ORIGIN."
        ;;
    esac
  done
  rm -rf "$work"
  [ "$problems" -eq 0 ]
}

# --- evaluation certificate -------------------------------------------------------------------

banner_evaluation() {
  log "=============================================================================="
  log " TLS MODE: EVALUATION. This installation uses a SELF-SIGNED certificate made on"
  log " this machine. Browsers will warn, Office add-ins and engine-cli will refuse it."
  log " NOT FOR PRODUCTION. Use DYNODOC_TLS_MODE=provided or acme for real use."
  log "=============================================================================="
}

# Atomically point DIR/current at a new version directory holding the given files.
publish_version() {
  base=$1 newdir=$2
  ln -s "$(basename "$newdir")" "$base/current.tmp"
  mv -T -f "$base/current.tmp" "$base/current"
  # Keep the newest two version directories.
  ls -1d "$base"/v-* 2>/dev/null | sort -r | tail -n +3 | while read -r old; do rm -rf "$old"; done
}

# Create or renew the evaluation certificate when it is missing, close to expiry, for other names,
# or does not match its key. Prints nothing when it is current. Sets EVAL_CHANGED=1 when it wrote one.
eval_ensure() {
  EVAL_CHANGED=0
  dir=$STATE_DIR/evaluation
  mkdir -p "$dir"
  cert=$dir/current/fullchain.pem
  key=$dir/current/privkey.pem
  reason=""
  if [ ! -s "$cert" ] || [ ! -s "$key" ]; then
    reason="no evaluation certificate yet"
  elif ! openssl x509 -in "$cert" -noout -checkend $((EVAL_RENEW_DAYS * 86400)) >/dev/null 2>&1; then
    reason="the evaluation certificate has less than $EVAL_RENEW_DAYS days left"
  else
    ks=$(openssl pkey -in "$key" -pubout 2>/dev/null | openssl sha256 | awk '{print $NF}')
    cs=$(openssl x509 -in "$cert" -noout -pubkey 2>/dev/null | openssl sha256 | awk '{print $NF}')
    [ "$ks" = "$cs" ] || reason="the evaluation key does not match its certificate"
    for h in $ALL_HOSTS; do
      if is_ipv4 "$h"; then out=$(openssl x509 -in "$cert" -noout -checkip "$h" 2>&1 || true); else out=$(openssl x509 -in "$cert" -noout -checkhost "$h" 2>&1 || true); fi
      case "$out" in *"does match certificate"*) ;; *) reason="the evaluation certificate does not cover $h" ;; esac
    done
  fi
  [ -n "$reason" ] || return 0

  log "Creating a new self-signed evaluation certificate ($reason)."
  san="DNS:localhost,IP:127.0.0.1,IP:::1"
  for h in $ALL_HOSTS; do
    if is_ipv4 "$h"; then
      [ "$h" = "127.0.0.1" ] || san="$san,IP:$h"
    else
      [ "$h" = "localhost" ] || san="$san,DNS:$h"
    fi
  done
  cn=$PRIMARY_HOST
  [ "${#cn}" -le 64 ] || cn="dynodoc-evaluation"
  newdir=$dir/v-$(date -u +%Y%m%d%H%M%S)
  mkdir -p "$newdir"
  if ! openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 90 \
      -keyout "$newdir/privkey.pem" -out "$newdir/fullchain.pem" \
      -subj "/O=Dynodoc EVALUATION - NOT FOR PRODUCTION/CN=$cn" \
      -addext "subjectAltName=$san" \
      -addext "basicConstraints=critical,CA:FALSE" \
      -addext "keyUsage=critical,digitalSignature" \
      -addext "extendedKeyUsage=serverAuth" >/dev/null 2>&1; then
    rm -rf "$newdir"
    err "could not create the evaluation certificate (openssl failed)"
    return 1
  fi
  chmod 600 "$newdir/privkey.pem"
  publish_version "$dir" "$newdir"
  EVAL_CHANGED=1
  log "Evaluation certificate for: $san (valid 90 days, renewed when under $EVAL_RENEW_DAYS days remain)."
  return 0
}

# --- per-mode preparation ---------------------------------------------------------------------

# Sets HTTPS (yes/no), CERT, KEY, TLS_MODE_HEADER_LINE. Returns 1 when the proxy must not start
# (or, on a reload, when the new files must not be used).
mode_prepare() {
  HTTPS=no CERT="" KEY="" TLS_MODE_HEADER_LINE="" ACME_LOCATION_TEXT=""
  case "${DYNODOC_TLS_MODE:-}" in
    provided)
      CERT=$PROVIDED_DIR/fullchain.pem
      KEY=$PROVIDED_DIR/privkey.pem
      if ! check_pair "tls/fullchain.pem" "$CERT" "$KEY"; then
        err "TLS mode 'provided': the certificate cannot be used (see the lines above)."
        err "Fix tls/fullchain.pem and tls/privkey.pem on the host, copy them in with:"
        err "  docker compose run --rm --no-deps secrets-init"
        err "then check them with: docker compose run --rm --no-deps proxy check"
        return 1
      fi
      HTTPS=yes
      ;;
    evaluation)
      eval_ensure || return 1
      CERT=$STATE_DIR/evaluation/current/fullchain.pem
      KEY=$STATE_DIR/evaluation/current/privkey.pem
      HTTPS=yes
      TLS_MODE_HEADER_LINE='    add_header X-Dynodoc-Tls-Mode "evaluation; self-signed certificate; not for production" always;'
      ;;
    acme)
      ACME_LOCATION_TEXT='    location ^~ /.well-known/acme-challenge/ {
        root /var/lib/dynodoc-tls/acme-webroot;
        default_type text/plain;
        try_files $uri =404;
    }'
      mkdir -p "$STATE_DIR/acme-webroot/.well-known/acme-challenge"
      CERT=$STATE_DIR/acme/current/fullchain.pem
      KEY=$STATE_DIR/acme/current/privkey.pem
      if [ -e "$CERT" ] || [ -e "$KEY" ]; then
        if check_pair "the certificate issued by the ACME client" "$CERT" "$KEY"; then
          HTTPS=yes
        else
          err "TLS mode 'acme': the certificate the ACME client left is not usable (see above). Serving plain HTTP only until a good one arrives."
        fi
      else
        log "TLS mode 'acme': no certificate has been issued yet. Serving plain HTTP (the challenge and a redirect) until the ACME client has one. Is the 'acme' profile enabled (COMPOSE_PROFILES=acme) and is port 80 reachable from the internet?"
      fi
      ;;
    *)
      err "DYNODOC_TLS_MODE must be one of: provided, evaluation, acme (it is '${DYNODOC_TLS_MODE:-not set}'). There is no default: choosing how this installation gets its certificate is your decision."
      return 1
      ;;
  esac
  return 0
}

# --- rendering --------------------------------------------------------------------------------

render_config() {
  mkdir -p "$RENDER_DIR"
  hsts=${DYNODOC_HSTS_MAX_AGE:-}
  if [ -z "$hsts" ]; then
    if [ "${DYNODOC_TLS_MODE:-}" = evaluation ]; then hsts=300; else hsts=31536000; fi
  fi
  case "$hsts" in ''|*[!0-9]*) err "DYNODOC_HSTS_MAX_AGE must be a number of seconds"; return 1 ;; esac

  host_map="" names=""
  for h in $ALL_HOSTS; do
    host_map="$host_map    \"$h\" 1;
"
    names="$names $h"
  done

  # Replacement text with newlines and slashes goes through awk's file reading, not sed.
  esc_origin=$(printf '%s' "$ORIGIN_URL" | sed 's/[&|\\]/\\&/g')
  acme_file=$(mktemp)
  printf '%s\n' "$ACME_LOCATION_TEXT" > "$acme_file"
  sed -e "s|@@ORIGIN@@|$esc_origin|g" "$TEMPLATE_DIR/http.conf.tpl" \
    | awk -v f="$acme_file" '/^@@ACME_LOCATION@@$/ { while ((getline line < f) > 0) print line; close(f); next } { print }' \
    > "$RENDER_DIR/10-http.conf.new"
  rm -f "$acme_file"
  mv -f "$RENDER_DIR/10-http.conf.new" "$RENDER_DIR/10-http.conf"

  if [ "$HTTPS" = yes ]; then
    map_file=$(mktemp)
    printf '%s' "$host_map" > "$map_file"
    hdr_file=$(mktemp)
    printf '%s\n' "$TLS_MODE_HEADER_LINE" > "$hdr_file"
    sed -e "s|@@SERVER_NAMES@@|${names# }|" \
        -e "s|@@CERT@@|$CERT|" -e "s|@@KEY@@|$KEY|" \
        -e "s|@@HSTS_MAX_AGE@@|$hsts|" "$TEMPLATE_DIR/https.conf.tpl" \
      | awk -v m="$map_file" -v h="$hdr_file" '
          /^@@HOST_MAP@@$/ { while ((getline line < m) > 0) print line; close(m); next }
          /^@@TLS_MODE_HEADER@@$/ { while ((getline line < h) > 0) if (line != "") print line; close(h); next }
          { print }' \
      > "$RENDER_DIR/20-https.conf.new"
    rm -f "$map_file" "$hdr_file"
    mv -f "$RENDER_DIR/20-https.conf.new" "$RENDER_DIR/20-https.conf"
  else
    rm -f "$RENDER_DIR/20-https.conf"
  fi
}

# A short fingerprint of what the current mode would serve, to notice a changed certificate.
fingerprint() {
  case "${DYNODOC_TLS_MODE:-}" in
    provided) f1=$PROVIDED_DIR/fullchain.pem f2=$PROVIDED_DIR/privkey.pem ;;
    evaluation) f1=$STATE_DIR/evaluation/current/fullchain.pem f2=$STATE_DIR/evaluation/current/privkey.pem ;;
    acme) f1=$STATE_DIR/acme/current/fullchain.pem f2=$STATE_DIR/acme/current/privkey.pem ;;
    *) f1=/dev/null f2=/dev/null ;;
  esac
  { cat "$f1" "$f2" 2>/dev/null || true; } | openssl sha256 | awk '{print $NF}'
}
