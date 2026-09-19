# Research platform connections and acceptance

All connections are read-only imports initiated by a signed-in researcher. They
create a new Dynodoc document or response spreadsheet and retain import provenance.
They do not modify the source project, schedule synchronization, or save credentials.

## Supported connection paths

| Platform | Credentials | What can be imported |
|---|---|---|
| ODK Central | HTTPS server and Web User bearer session token | Project/form discovery, XLSX form source, main OData response table. |
| KoboToolbox | HTTPS server and API key | Survey discovery, XLSForm source, paginated response snapshots. |
| REDCap | HTTPS server, API path (e.g. `/redcap/api/`), project API export token | Project discovery, metadata converted to a dictionary CSV, flat raw records. |
| SurveyCTO | `https://your-server.surveycto.com`, API username/password and form ID | Wide JSON response export converted to CSV; the platform's default review-status filter applies. Questionnaire editing uses XLSForm files. |
| SurveyMonkey | `https://api.surveymonkey.com` and a bearer access token | Survey discovery and paginated bulk response snapshots. Nested question/answer objects remain JSON-valued columns with original IDs. |
| CSPro | No live connection | Upload CSV/XLSX exported by CSPro. Native dictionaries, form applications, logic and CSDB files are unsupported. |

Provider permissions and subscription plans can limit API export access. Google
sign-in to Dynodoc does not authorize any research-platform API account.

## Transport and storage boundaries

Servers must be HTTPS, with publicly routable IPv4 addresses. DNS is resolved once
and the verified address is pinned to the TLS request. Private/reserved addresses,
credentials in URLs, redirects and pagination to another origin are refused.
REDCap export uses a token in an HTTPS form POST; SurveyCTO uses HTTP Basic over TLS;
other platforms use bearer/token headers. No upstream response body or credential is
returned in connection errors. Incoming connection details are limited to 8 KiB;
exports are bounded to 10 MiB, 2,999 response rows and 100 spreadsheet columns.
Exceeding a bound fails explicitly instead of silently truncating the study.

Original imports and source origin/form identifiers are retained in the research
document. Tokens and passwords are not retained. Response CSV conversion neutralizes
spreadsheet formula injection; raw arrays remain JSON text, not flattened repeat tables.

## Tested and still requiring study validation

Automated tests cover ODK/Kobo discovery and pagination, REDCap metadata field
mapping, SurveyCTO Basic authentication and response mapping, SurveyMonkey pagination,
credential exclusion, private address rejection, cross-host rejection and oversized
export refusal. A browser test covers connection selection, import review and creation
of a spreadsheet with a provider fixture. These checks validate our implementation;
they are not evidence that a particular live study account has been accepted.

Before a study uses a connection, its owner must:

- Use an export-scoped account and a synthetic or approved example instrument.
- Compare form structure, choice codes, translations, expressions and row counts.
- Review handling of repeats, longitudinal events, encrypted fields and media.
- Validate any exported questionnaire in the destination platform before fieldwork.
- Confirm that editing Dynodoc's imported copy has not changed the source project.

No real study tokens were provided during this implementation. Live study acceptance
remains open, including CSPro's export settings and SurveyMonkey plan-specific access.

## Respondent preview

The native preview executes a bounded XLSForm subset without JavaScript `eval`:
variable references, current-answer `.`, arithmetic, comparisons, boolean operators,
`selected`, `count-selected`, `if`, `not`, `true`, `false`, `number`, `int`, `string`,
`boolean`, `string-length`, `round`, `coalesce`, `concat`, `min` and `max`.
It applies relevance including parent groups, calculations, conditional/literal
required rules and constraints. Missing variables, cyclic calculations, invalid
syntax and unsupported constructs are reported. Answers stay in page memory and
are cleared on reload; the preview is not a response-collection service.

Repeat instances, dynamic external datasets, media collection, REDCap expression
execution, complex date/XPath functions and full vendor runtime parity are not
implemented. Import/export retains original vendor columns where supported. The
engine's deployment gate checks references/types/syntax, but it is not a substitute
for the destination platform's validator.

Reference APIs: [ODK Central](https://docs.getodk.org/central-api/),
[Kobo](https://kf.kobotoolbox.org/api/v2/),
[REDCap provider metadata implementation](https://github.com/vanderbilt-redcap/port_project_to_remote_redcap),
[SurveyCTO API export example](https://support.surveycto.com/hc/en-us/articles/360036181094-Integration-with-Power-BI-part-5-Using-an-R-script),
[SurveyMonkey](https://api.surveymonkey.com/v3/docs).
