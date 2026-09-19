# Google and email sign-in for dynodoc.online

The public homepage runs on Vercel at `https://dynodoc.online`. The researcher
workspace, authentication, API and database run on the VPS. The workspace address
is `https://app.dynodoc.online/workspace`.

Keep authentication secrets on the VPS. The homepage needs only the public
workspace URL; it does not need database, Google or Resend credentials.

## Vercel

In the homepage project's environment variables, set this for Production and any
Preview deployments that should use the pilot workspace, then redeploy:

```dotenv
PUBLIC_WORKSPACE_URL=https://app.dynodoc.online
```

Keep the existing `dynodoc.online` / `www` Vercel domain records. The registrar's
separate `app` A record points to `103.191.50.32`. Do not point the apex domain at
the VPS or add `app.dynodoc.online` as a Vercel domain for this architecture.

## Google

1. Open [Google Cloud Console](https://console.cloud.google.com/) and select or
   create the Dynodoc project.
2. In **Google Auth Platform → Branding**, use the app name Dynodoc and a support
   email you can receive. Set:
   - Homepage: `https://dynodoc.online`
   - Privacy policy: `https://dynodoc.online/privacy`
   - Terms: `https://dynodoc.online/terms`
   - Authorized domain: `dynodoc.online`
3. Complete any domain-ownership verification Google requests. The privacy and
   terms pages currently disclose that they are drafts; supply the operator and
   contact details and complete their review before adopting them for a public
   launch or representing them as finalized in a verification submission.
4. Under **Audience**, choose **External** unless all researchers belong to your
   Google Workspace organization. For the pilot, use **Testing** and add the
   researchers' Google email addresses as test users.
5. Under **Data Access**, use only `openid`, `email` and `profile`. Google Drive,
   Docs and Sheets permissions are not needed for sign-in.
6. Under **Clients**, create or edit a **Web application** OAuth client. Add this
   exact **Authorized redirect URI**:

   ```text
   https://app.dynodoc.online/auth/google/callback
   ```

   A JavaScript origin is not needed by this server-side authorization-code flow.
   If you previously registered the temporary hostname, add the new callback to
   the same client; there is no need to create a new project just for the domain.
7. Put the client ID and client secret in the VPS private configuration below.
8. After applying configuration, test Google sign-in at
   `https://app.dynodoc.online/login` with a registered pilot user. Confirm that
   the browser returns to the workspace, then invite colleagues through a
   document's **Share** panel.

Google requires an exact redirect URI match, including scheme, hostname and
path. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server).

## Resend

1. Add `mail.dynodoc.online` as a sending domain in Resend. A dedicated sending
   subdomain keeps the sign-in mail configuration separate from the homepage and
   any future mailbox service.
2. At the registrar, copy the exact SPF, DKIM and any required return-path MX
   records shown in the Resend dashboard. Values and record names depend on the
   selected domain and region; do not guess them. Keep the Vercel records and
   the `app` A record. Use Resend's verification check until the domain is verified.
3. Create a Resend API key with permission to send from the verified domain.
4. Set the sender and key on the VPS:

   ```dotenv
   EMAIL_FROM=Dynodoc <signin@mail.dynodoc.online>
   RESEND_API_KEY=your-resend-key
   ```

   You may instead verify the root `dynodoc.online` domain and send from an
   address there. The sender must match the domain you actually verify. Resend
   sends the sign-in messages; setting a From address does not create a mailbox
   for receiving support requests or replies.
5. Apply configuration, request a sign-in link at the workspace login page and
   confirm delivery. The link should begin with
   `https://app.dynodoc.online/auth/verify`; confirm successful sign-in and that
   the same link cannot be used twice. Links expire after 15 minutes.

A test sender is not enough for delivery to the whole team. See
[Resend domain verification](https://resend.com/docs/dashboard/domains/introduction).

## Private VPS configuration

Using your SSH access, edit the existing configuration in place:

```sh
cd /opt/dynodoc-demo
nano infra/demo/.env
chmod 600 infra/demo/.env
```

The public origin and provider values are:

```dotenv
ORIGIN=https://app.dynodoc.online
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
RESEND_API_KEY=your-resend-key
EMAIL_FROM=Dynodoc <signin@mail.dynodoc.online>
```

Preserve the existing `POSTGRES_PASSWORD`, `PASETO_LOCAL_KEY`, `WEB_SERVICE_KEY`,
`SESSION_SECRET` and `CONVERTER_KEY`. Do not replace the whole file with this
snippet, and do not paste secrets into chat or commit them.

After adding provider credentials, recreate only the web service:

```sh
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml up -d --no-deps web
```

If changing `ORIGIN` during a future domain migration, recreate `api` as well,
so its public API metadata uses the same origin. The database volume does not
need to be recreated or migrated when a hostname changes.

Check the non-secret provider flags:

```sh
curl https://app.dynodoc.online/auth/options
```

Expected after configuration: `google: true`, `email: true`, `development: false`.
These flags confirm that values are present; real Google login and email delivery
must still be tested. The public deployment forces `DEV_AUTH=false`.
