# ODK Central and KoboToolbox imports

In Dynodoc, open **Workspace → Import from research tool**. Choose the platform,
enter its HTTPS server address and a token, then choose **Find questionnaires**.
For Central, choose the project first. Select a questionnaire and either its
questionnaire source or its current responses, then review the import preview.

## KoboToolbox

- Use `https://kf.kobotoolbox.org`, `https://eu.kobotoolbox.org`, or your own
  publicly reachable Kobo server. Enter the server origin, without an API path.
- Find the API key in your Kobo account settings. The account must be allowed to
  read the questionnaire and, for response imports, its submissions.
- The connector retrieves questionnaire sources through Kobo's XLSForm renderer
  and responses through the paginated v2 data API.

See [Kobo's API guide](https://support.kobotoolbox.org/api). The API's historical
`.xls` questionnaire renderer currently returns XLSX; this is documented in
[Kobo's API implementation](https://github.com/kobotoolbox/kpi/blob/master/kpi/views/v2/asset.py).

## ODK Central

- Use your Central HTTPS origin, for example `https://study.getodk.cloud`.
- Use a current **Web User session token** from Central's authentication API.
  The account must be able to read the project, forms and requested submissions.
  An App User token used by Collect is not equivalent to a Web User session.
- Central can return the editable XLSX source only when an XLSX source was
  uploaded. Forms created from XML or legacy XLS require a separate XLSX copy.
- Responses are read through Central's paginated OData endpoint.

Ask your Central administrator to provide the pilot account/token through your
usual secure channel. See [Central authentication](https://docs.getodk.org/central-api-authentication/),
[form sources](https://docs.getodk.org/central-api-form-management/#retrieving-form-xls-x),
and [OData](https://docs.getodk.org/central-api-odata-endpoints/).

## What an import means

Each import creates a new Dynodoc copy with a preserved source export and a
record of the server, form and fetch time. **File → Original uploads & import
report** shows that record. Tokens are used only for the current connection;
they are not saved in the database, browser storage, source metadata or export.

Response imports support 2,999 records plus a header, 100 columns and 3 MB (3,000,000 bytes).
Pagination is followed within the chosen server; the connector refuses an
oversized export rather than silently truncating it. Nested objects become
columns; JSON arrays stay as text. Separate ODK repeat tables, attachments,
encrypted responses and external datasets are not downloaded. Questionnaire
logic is preserved for validation and execution in the collection platform.

This is an API-backed import. Scheduled refresh, incremental reconciliation,
publishing form revisions and writing response edits back to the collection
platform are not implemented. Repeat the import for a fresh copy, or export a
reviewed XLSForm and validate it in your platform before use.

Connector tests use provider fixtures and verify the GUI import path. A real
Kobo HTTPS request with an invalid test token returned the expected 401 response.
A successful study-account import still needs the owner's server and token.
The pilot connector supports publicly reachable HTTPS servers on port 443 with
IPv4 DNS records; private-network and IPv6-only deployments need a separate
networking design.
