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
   Google Workspace organization. Use **Publish app → In production** for public Google sign-in. The owner has
   confirmed this setting; researchers do not need to be registered as test users.
5. Under **Data Access**, use only `openid`, `email` and `profile`. Google Drive,
   Docs and Sheets permissions are not needed for sign-in.
6. Under **Clients**, create or edit a **Web application** OAuth client. Add this
   exact **Authorized redirect URIs**:

   ```text
   https://app.dynodoc.online/auth/google/callback
   https://admin.dynodoc.online/auth/google/callback
   ```

   A JavaScript origin is not needed by this server-side authorization-code flow.
   If you previously registered the temporary hostname, add the new callback to
   the same client; there is no need to create a new project just for the domain.
7. Put the client ID and client secret in the VPS private configuration below.
8. After applying configuration, test Google sign-in at
   `https://app.dynodoc.online/login` with a Google account. Confirm that
   the browser returns to the workspace, then invite colleagues through a
   document's **Share** panel.

Google requires an exact redirect URI match, including scheme, hostname and
path. See [Google's web-server OAuth guide](https://developers.google.com/identity/protocols/oauth2/web-server).

## Resend

1. The owner selected `dynodoc.online` as the sending domain in Resend, with
   `signin@dynodoc.online` as the sender.
2. At the registrar, copy the exact SPF, DKIM and any required return-path MX
   records shown in the Resend dashboard. Values and record names depend on the
   selected domain and region; do not guess them. Keep the Vercel records and
   the `app` A record. Use Resend's verification check until the domain is verified.
3. Create a Resend API key with permission to send from the verified domain.
4. Set the sender and key on the VPS:

   ```dotenv
   EMAIL_FROM=Dynodoc <signin@dynodoc.online>
   RESEND_API_KEY=your-resend-key
   ```

   The sender must match the domain you actually verify. Resend
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
EMAIL_FROM=Dynodoc <signin@dynodoc.online>
```

Preserve the existing `POSTGRES_PASSWORD`, `PASETO_LOCAL_KEY`, `WEB_SERVICE_KEY`,
`SESSION_SECRET` and `CONVERTER_KEY`. Do not replace the whole file with this
snippet, and do not paste secrets into chat or commit them.

After adding provider credentials, recreate the two web services:

```sh
docker compose --env-file infra/demo/.env -f infra/demo/compose.yml up -d --no-deps web admin
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

## Configuration status — 19 September 2026

Google credentials and the Resend send-only key have been transferred privately
from the ignored local configuration to the VPS. The sender is
`Dynodoc <signin@dynodoc.online>`. The owner confirmed public Google publishing
and both callback URLs. `ADMIN_EMAILS` restricts operations to the supplied owner
address; it does not restrict workspace registration.

A send-only Resend key cannot list domains: the provider's `restricted_api_key`
response to that read endpoint is expected and does not mean the key is invalid.
Confirm domain verification in the Resend dashboard, then test actual delivery and
Google login from your browser on both hosts. No real message was sent by the
deployment checks, and provider-presence flags do not substitute for these tests.

## Optional Google Forms connection

Ordinary sign-in still requests only `openid email profile`. Importing private Forms
uses a separate, explicit read-only consent flow with the existing OAuth client.
The owner confirmed enabling Forms API and registering:

```text
https://app.dynodoc.online/integrations/google/callback
```

In **Google Auth Platform → Data Access**, declare:

```text
https://www.googleapis.com/auth/forms.body.readonly
https://www.googleapis.com/auth/forms.responses.readonly
```

Keep **Audience → In production**. Check **Verification Center** for scope/brand
review requirements. Publishing basic sign-in does not automatically verify new
Forms permissions. Google may apply an unverified-app warning or user cap until
required review is completed; Workspace administrators can also restrict access.
The scopes are documented by Google's [form reader](https://developers.google.com/workspace/forms/api/reference/rest/v1/forms/get)
and [response reader](https://developers.google.com/workspace/forms/api/reference/rest/v1/forms.responses/list).
See [Google's verification instructions](https://developers.google.com/identity/protocols/oauth2/production-readiness/sensitive-scope-verification).

`GOOGLE_FORMS_API` is not used: an API key cannot authorize reading a researcher's
private forms. Keep the existing client secret private; no additional key is
required by this implementation. Finalize the legal operator details and policy
review before submitting the policy as final to Google.

In the workspace, open **Google Forms**, connect, and paste the form's **editor**
URL (`https://docs.google.com/forms/d/FORM_ID/edit`) or API form ID. A public
respondent URL containing `/d/e/` is a different identifier and is rejected. Choose
questionnaire or responses, review conversion notes, then confirm import. The
connection is encrypted in an HttpOnly cookie, bound to the current Dynodoc account,
and expires within one hour. No refresh token or background synchronization is
requested. Disconnect/sign-out clears it; revoke the grant in Google Account
permissions to remove provider authorization. Imported documents remain until
handled through the workspace's retention process.

Acceptance requires an owner-controlled real form: check single/multiple choice,
grids, dates, validation, branching and response counts against the original.
Grids flatten; branching, media, quiz grading and unsupported settings require
review. Import retains the source form definition in the generated original XLSX.
API reads are capped at 10 MiB, 2,000 questions, 2,999 response rows and 100 columns;
large imports fail with a visible explanation instead of silently truncating.

### Verification Center scope justifications

The owner confirmed saving both Forms scopes on 19 September 2026. Use these
implementation-specific explanations when Google requests justification:

**forms.body.readonly:** Dynodoc lets signed-in researchers import a Google Form
they can access into an editable research questionnaire. The user explicitly
connects Google Forms and supplies a form ID. We read its questions, choices,
settings and structure to create a separate workspace copy. We do not modify the
original form or request Google Drive access.

**forms.responses.readonly:** Dynodoc lets researchers import responses from a
selected Google Form into a spreadsheet for review and analysis. The import starts
only when the user requests it. We need read access to retrieve answers and response
identifiers. We do not edit responses, synchronize in the background or use the data
for advertising.

For a requested demonstration video, use a synthetic form with non-sensitive sample
answers: show workspace sign-in, the Google Forms connection button, Google's consent
screen with the client identity/scopes, questionnaire import and its conversion notes,
response import and resulting sheet, then disconnect. Show the published privacy
policy. Never show client secrets, real participant records or private access tokens.
Complete the operator/contact details and policy review before presenting the draft
policy as adopted. Google decides whether the submitted demonstration and information
satisfy its review; these explanations are not a guarantee of verification.
