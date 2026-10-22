#!/bin/sh
# Exercises the `provided` TLS checks of the proxy against generated certificate chains, good and
# bad. Runs inside the proxy image, as its own user, with nothing but openssl and the proxy's own
# scripts:
#
#   docker run --rm --entrypoint sh -v "<this directory>:/t:ro" <proxy image> /t/tls-provided-cases.sh
#
# Exit code 0 when every case behaves as expected.
set -u
export ORIGIN=https://localhost:8443
export DYNODOC_TLS_MODE=provided
W=/tmp/tlscases
rm -rf "$W"
mkdir -p "$W"
cd "$W" || exit 2
fails=0
passes=0

mk() { openssl "$@" >/dev/null 2>&1; }

# --- fixtures ----------------------------------------------------------------------------------
mk req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout ca.key -out ca.pem -days 3650 -subj "/CN=Test Root CA" -addext "basicConstraints=critical,CA:TRUE" -addext "keyUsage=critical,keyCertSign,cRLSign"
mk req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout int.key -out int.csr -subj "/CN=Test Intermediate CA"
printf 'basicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n' > int.ext
mk x509 -req -in int.csr -CA ca.pem -CAkey ca.key -CAcreateserial -out int.pem -days 3650 -extfile int.ext

# leaf NAME SAN [extra x509 args]: leaf.NAME.key / leaf.NAME.pem signed by the intermediate
leaf() {
  n=$1 san=$2; shift 2
  mk req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout "leaf.$n.key" -out "leaf.$n.csr" -subj "/CN=$n"
  printf 'subjectAltName=%s\nbasicConstraints=CA:FALSE\nextendedKeyUsage=serverAuth\n' "$san" > "leaf.$n.ext"
  mk x509 -req -in "leaf.$n.csr" -CA int.pem -CAkey int.key -CAcreateserial -out "leaf.$n.pem" -extfile "leaf.$n.ext" "$@"
}
leaf good "DNS:localhost" -days 365
leaf other "DNS:other.example" -days 365
leaf soon "DNS:localhost" -days 5
leaf expired "DNS:localhost" -not_before 20200101000000Z -not_after 20200201000000Z
leaf future "DNS:localhost" -not_before 20990101000000Z -not_after 20991201000000Z
mk req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -keyout stranger.key -out stranger.csr -subj "/CN=stranger"
mk pkey -in leaf.good.key -aes256 -passout pass:secret -out leaf.good.enc.key

# dirset NAME CERT KEY: a directory shaped like the proxy's tls volume.
dirset() { mkdir -p "$1"; cat "$2" > "$1/fullchain.pem"; cat "$3" > "$1/privkey.pem"; }
dirset ok leaf.good.pem leaf.good.key
cat int.pem >> ok/fullchain.pem
dirset leafonly leaf.good.pem leaf.good.key
dirset reversed int.pem leaf.good.key
cat leaf.good.pem >> reversed/fullchain.pem
dirset wrongkey leaf.good.pem stranger.key
cat int.pem >> wrongkey/fullchain.pem
dirset expired leaf.expired.pem leaf.expired.key
cat int.pem >> expired/fullchain.pem
dirset future leaf.future.pem leaf.future.key
cat int.pem >> future/fullchain.pem
dirset soon leaf.soon.pem leaf.soon.key
cat int.pem >> soon/fullchain.pem
dirset othername leaf.other.pem leaf.other.key
cat int.pem >> othername/fullchain.pem
dirset encrypted leaf.good.pem leaf.good.enc.key
cat int.pem >> encrypted/fullchain.pem
dirset garbage leaf.good.pem leaf.good.key
echo "this is not a certificate" > garbage/fullchain.pem
dirset notakey leaf.good.pem leaf.good.key
echo "this is not a key" > notakey/privkey.pem
dirset gap leaf.good.pem leaf.good.key
cat ca.pem >> gap/fullchain.pem
mkdir -p empty
mkdir -p nokey
cat leaf.good.pem int.pem > nokey/fullchain.pem

# --- cases -------------------------------------------------------------------------------------
# run_case NAME EXPECT(ok|fail) PATTERN: PATTERN must appear in the output (a plain-words message).
run_case() {
  name=$1 expect=$2 pattern=$3
  out=$(DYNODOC_TLS_PROVIDED_DIR="$W/$name" DYNODOC_TLS_STATE_DIR="$W/state" /usr/local/bin/proxy-entrypoint check 2>&1)
  rc=$?
  want=0
  [ "$expect" = ok ] || want=78
  if [ "$rc" -eq "$want" ] && printf '%s' "$out" | grep -qi -- "$pattern"; then
    passes=$((passes + 1))
    printf 'ok    %-10s (exit %s) %s\n' "$name" "$rc" "$pattern"
    [ -z "${VERBOSE:-}" ] || printf '%s\n' "$out" | sed 's/^/        /'
  else
    fails=$((fails + 1))
    printf 'FAIL  %-10s expected exit %s and "%s", got exit %s\n%s\n' "$name" "$want" "$pattern" "$rc" "$out"
  fi
}

run_case ok ok "OK"
run_case leafonly ok "only the server certificate"
run_case soon ok "expires within"
run_case reversed fail "must come first"
run_case wrongkey fail "does not match the certificate"
run_case expired fail "has expired"
run_case future fail "not valid yet"
run_case othername fail "does not cover the name 'localhost'"
run_case encrypted fail "protected by a passphrase"
run_case garbage fail "contains no PEM certificate"
run_case notakey fail "not a PEM private key"
run_case empty fail "is missing"
run_case nokey fail "private key file for tls/fullchain.pem is missing"
run_case gap fail "was not issued by certificate number"

# The check must refuse a mode that is not set, and an origin that is not https.
out=$(DYNODOC_TLS_MODE= /usr/local/bin/proxy-entrypoint check 2>&1); rc=$?
if [ "$rc" -eq 78 ] && printf '%s' "$out" | grep -q "There is no default"; then passes=$((passes + 1)); echo "ok    no-mode    refuses an unset TLS mode"; else fails=$((fails + 1)); echo "FAIL  no-mode    $out"; fi
out=$(ORIGIN=http://localhost DYNODOC_TLS_PROVIDED_DIR="$W/ok" /usr/local/bin/proxy-entrypoint check 2>&1); rc=$?
if [ "$rc" -eq 78 ] && printf '%s' "$out" | grep -q "must be https://"; then passes=$((passes + 1)); echo "ok    http-origin refuses a plain http ORIGIN"; else fails=$((fails + 1)); echo "FAIL  http-origin $out"; fi

printf '\n%s passed, %s failed\n' "$passes" "$fails"
[ "$fails" -eq 0 ]
