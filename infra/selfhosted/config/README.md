# config/

Optional non-secret files that `.env` refers to, mounted read-only at `/etc/dynodoc` in the web
server (and in the `preflight` check). Examples:

- `notice.txt` for `DYNODOC_NOTICE_FILE=/etc/dynodoc/notice.txt` (the organisation notice)
- `mail-ca.pem` for `SMTP_CA_FILE=/etc/dynodoc/mail-ca.pem` (the authority that signed your mail
  server's certificate)

Make them readable by everyone (mode 0644): they hold no secret. Secrets belong in `secrets/`.
