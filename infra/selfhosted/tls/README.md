# tls/

Used only with `DYNODOC_TLS_MODE=provided`. Put your certificate here:

- `fullchain.pem` the server certificate first, then every intermediate certificate, in PEM
- `privkey.pem` the matching private key, in PEM, without a passphrase

`docker compose up` copies them into the proxy's own volume, with the right owner, so they can stay
private to you on the host (mode 0600 is fine). Replace the files and run
`docker compose run --rm --no-deps secrets-init`, then `docker compose exec proxy
proxy-entrypoint reload` to switch to a renewed certificate without downtime.

The proxy checks that the key belongs to the certificate, that the certificate is current and
covers the name in `ORIGIN`, and that the chain is in order, and refuses to start with a
plain-words message if not. Never commit these files.
