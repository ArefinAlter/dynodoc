# Google and email sign-in for the researcher pilot

The workspace callback is:

```text
https://dynodoc.103.191.50.32.sslip.io/auth/google/callback
```

The Vercel homepage links to this workspace. Configure login on the workspace host; the homepage does not need access to authentication secrets.

## Google

1. Open [Google Cloud Console](https://console.cloud.google.com/) and select or create a project for Dynodoc.
2. Open **Google Auth Platform**. Set up **Branding** with the app name, support email and developer contact email.
3. Under **Audience**, choose **External** (unless all researchers belong to your Google Workspace organization). Keep the app in **Testing** and add your pilot researchers' Google email addresses as test users.
4. Under **Data Access**, use only `openid`, `email` and `profile`. Dynodoc uses these for sign-in; it does not request access to the user's Google Drive.
5. Under **Clients**, create an OAuth client of type **Web application**. Name it `Dynodoc researcher pilot`. Add the exact callback URL above as an **Authorized redirect URI**. A JavaScript origin is not required by this server-side authorization-code flow.
6. Record the client ID and client secret directly in the server configuration below. Do not commit or paste the client secret into chat.
7. After restarting the web container, open the workspace login page, choose Google, and test with one of the registered test users. Then invite the other researchers from a document's **Share** panel.

The temporary hostname is suitable for a pilot. If Google's console requires ownership verification for your configuration, use a hostname you control and update `ORIGIN` and the registered callback together. Production branding or verification may require a domain you own.

Google requires an exact redirect URI match, including scheme, hostname and path. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server) and [OAuth domain policies](https://developers.google.com/identity/protocols/oauth2/policies).

## Resend

1. In Resend, create an API key with permission to send email.
2. Add a sending domain you control and complete the DNS verification shown by Resend. The site itself can use the temporary workspace address; the sending domain can be a separate existing domain.
3. Set `EMAIL_FROM` to an address on that verified domain, for example `Dynodoc <research@example.org>`.
4. Set `RESEND_API_KEY` on the server. After restarting, request a sign-in link and check delivery, one-time use and expiry.

A Resend test sender is not a substitute for a verified sending domain when inviting the full team. See [Resend domain setup](https://resend.com/docs/dashboard/domains/introduction).

## Configure the server

Connect to the VPS using your existing SSH access, then edit this private file:

```sh
cd /opt/dynodoc-demo
nano infra/demo/.env
chmod 600 infra/demo/.env
```

Fill in these values (keep all generated engine/database/session keys as they are):

```dotenv
GOOGLE_CLIENT_ID=your-client-id.apps.googleusercontent.com
GOOGLE_CLIENT_SECRET=your-client-secret
RESEND_API_KEY=your-resend-key
EMAIL_FROM=Dynodoc <research@your-verified-domain>
```

Apply the configuration:

```sh
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml up -d web
```

Do not enable `DEV_AUTH` on the public deployment. The provided Compose configuration forces it off.

Until credentials are configured, unavailable sign-in methods are shown as unavailable. Google and Resend account configuration and a real provider login still require your participation.
