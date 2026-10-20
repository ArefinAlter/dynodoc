# Device key project grants API

13 October 2026, schema 0040. Contract for the profile "Signing keys" screen and for
the command-line client. Source only; not deployed (production is schema 39).

A signing key file used to be one factor for the whole account: whoever held the file
could open a one-hour session for any project the account could reach and use
`history:write` (including review decisions). A key now carries a **project access
mode** and, in the default mode, **per-project grants** that only the signed-in
account holder can change, from a browser session. A key file alone can no longer
widen its own reach.

All routes below use the ordinary account login (`Authorization: Bearer <access
token>`). **Project device tokens, connector grants and unauthenticated calls get 401**
and never learn whether the key exists. Only the key's own account can read or change
its access; another account's key id is 404. Request bodies are JSON and unknown
fields are rejected (the framework's 422 for a malformed shape, 400 for a documented
value problem). Timestamps are RFC 3339.

## Concepts

| Name | Meaning |
|---|---|
| `project_access` | `granted` (default for every new key): the key reaches only projects with an active grant, and only the granted scopes. `all`: the key reaches every project the account can currently access with any scope, as before schema 40. Every key that existed before schema 40 is `all` until its owner changes it. |
| Scope | One of `project:read`, `history:read`, `history:write`, `history:review`. Independent: none implies another. `history:review` is new in schema 40 and is the only scope that may record a review decision (`POST /projects/{id}/history/reviews`). |
| Grant | An active row per (key, project): a non-empty set of scopes. At most 50 active grants per key. Revocation keeps the row and records when. |
| Membership | A grant never replaces the account's project role. Every request still re-reads the role, file floors, policy and review rules. A grant for a project the account has left is inert. |

Why `granted` is the default: the old behaviour was convenient but made a stolen file
equal to the whole account. New keys are enrolled by an authenticated person who can
add the one project they mean to use at the same moment, so the safe mode costs one
click and the unsafe mode needs an explicit choice (`"project_access":"all"` at
enrolment, or `confirm_all_projects` when switching later). Existing keys are not
changed behind their owner's back.

## Enrolment (changed)

`POST /devices` (unchanged challenge flow) accepts one more optional field next to
`challenge` and `signature`:

```json
{ "challenge": {...}, "signature": "...", "project_access": "granted" }
```

`project_access` is `granted` when omitted. Any other value is 400. The field is not
part of the signed proof. An exact retry of an already enrolled active key returns the
existing key and **does not** change its mode. Response:
`{ "id": uuid, "public_key": "hex", "project_access": "granted" | "all" }`.

## `GET /devices` (extended, backward compatible)

Existing members are unchanged. New members:

```json
{
  "authority": "uuid", "identity": "uuid", "active_limit": 20, "total_limit": 128,
  "grant_limit": 50,
  "scopes": ["project:read", "history:read", "history:write", "history:review"],
  "keys": [{
    "id": "uuid", "public_key": "hex", "registered_at": "...", "revoked_at": null,
    "project_access": "granted",
    "grants": [{
      "project_id": "uuid",
      "project_name": "Board packs",
      "role": "owner",
      "scopes": ["history:read", "history:write"],
      "created_at": "..."
    }]
  }]
}
```

- `grants` lists **active** grants, newest first, at most 50, and is empty for a revoked
  key.
- `project_name` and `role` are `null` when the account is no longer a current member
  of that project (the grant is inert; show it as "no access" and offer removal). The
  name of a project the account cannot see is never returned.
- With `project_access: "all"` grants may still exist (they take effect again if the
  owner switches back to `granted`). Show them as "not used while All projects".

## `PUT /devices/{key_id}/projects/{project_id}`

Create the grant, or replace the scopes of the active grant (idempotent).

Request: `{ "scopes": ["history:read", "history:write"] }`. Scopes are de-duplicated and
sorted. Response 200:
`{ "key_id", "project_id", "scopes": [...], "created_at", "changed": true|false }`.
`changed:false` means the active grant already had exactly these scopes (no audit row).

| Status | When |
|---|---|
| 400 | Empty set or an unknown scope |
| 401 | No account login, or a device token |
| 403 | The account is not a current member of the project (the same answer for an unknown project) |
| 404 | The key does not exist or belongs to another account |
| 409 | The key is revoked, or it already has 50 active grants and this would be the 51st project |

A scope the member's role cannot use (for example `history:review` for a contributor)
is accepted but useless: the role is rechecked on every request. The UI should show
the current `role` next to the grant and may hide scopes the role cannot use.

## `DELETE /devices/{key_id}/projects/{project_id}`

Revoke the active grant. Allowed even after the account left the project and after
the key was revoked. Response 200: `{ "key_id", "project_id", "revoked": true,
"changed": true|false }` (`changed:false` when there was no active grant). 401 as
above; 404 for a key that is not the caller's. Applies to the key's very next
request (see "Effect on running sessions").

## `PATCH /devices/{key_id}`

Request: `{ "project_access": "granted" | "all", "confirm_all_projects": true }`.

- Switching to `all` **requires** `"confirm_all_projects": true`; otherwise 400
  (also when the key is already `all`, so a client cannot depend on the no-op).
  Show a warning that a copy of this key file can then open every project the
  account can access.
- Switching to `granted` needs no confirmation and keeps existing grants (a switch
  back to `all` and again to `granted` loses nothing).
- `confirm_all_projects` is ignored for `granted`.

Response 200: `{ "key_id", "project_access", "changed": true|false }`. 404 not the
caller's key; 409 revoked key. Revocation of the key itself is still
`POST /devices/{id}/revoke`.

## `GET /profiles/{id}/starred`

Projects the profile's account has starred **and** the signed-in viewer can access
(a current member, directly or through a parent space). Uses the same visibility rule
as `GET /profiles/{id}`: an account that is not the viewer and shares no project with
the viewer is 404. A project the viewer cannot access is never listed and never
counted, even when the profile starred it. Stars from accounts that are no longer
members of the project, or are disabled, are not listed (the same rule as
`star_count`).

Response 200: `{ "items": [{ "id", "name", "star_count", "open_issue_count" }],
"truncated": false }`, newest star first, at most 100 (`truncated:true` when more
exist). No pagination in this version.

## What a device sees

`POST /projects/{id}/device-sessions` (unchanged shape) additionally refuses, with
the existing uniform `403`, a key that is `granted` and has no active grant for that
project or a grant that does not include every requested scope. The response is
identical to the one for a non-member, so it reveals nothing about which part failed.
A `granted` key's previously issued token fails its next request with `403` when its
grant is removed or narrowed below the scope the route needs, with no caching. `all`
keys behave exactly as before.

`POST /projects/{id}/history/reviews` now needs `history:review`, not `history:write`.
Browser sessions are unaffected. A token issued before the upgrade that has only
`history:write` gets `403` there until it expires (at most one hour); push, stage,
publish and ref advancement keep working with `history:write`.

## Effect on running sessions

Grant changes serialize with in-flight device requests through the same account row
lock that key revocation uses. A token already issued stops working for the affected
project/scope on its **next** request. Nothing is cached except the token's own
one-hour lifetime.

## Audit

One operation-audit row per effective change, resource id = key id, actor = the
account, detail field `key_id` (never `key`, which the administrator view redacts):

| Action | Detail |
|---|---|
| `device.grant.set` | `key_id`, `project_id`, `scopes`, `previous_scopes` (null for a new grant) |
| `device.grant.revoked` | `key_id`, `project_id`, `scopes` |
| `device.access.changed` | `key_id`, `from`, `to` |
| `device.enrolled` | existing detail plus `project_access` |

No row is written for a no-op (`changed:false`).

## Suggested screen behaviour

1. Per key: mode badge ("Selected projects" / "All projects (broad)"), a list of
   grants with scope checkboxes, an "Add project" picker limited to projects the
   account is a member of, and a revoke button. Show the grant count against
   `grant_limit`.
2. After enrolment of a `granted` key, take the person straight to "Add project".
3. Switching to "All projects" needs a confirmation step and sends
   `confirm_all_projects: true`.
4. A key with `project_access: "all"` should carry a visible suggestion to switch to
   selected projects.
5. The command line learns of a refusal only as `403`; tell the person to check this
   screen.

## Key class and the agent scope (schema 42)

A key is enrolled for a person (the default) or for an AI agent. The class is chosen once,
at enrolment, and never changes (`device_key_class`, immutable). It decides which scopes the
key may ever hold, so a signing key file cannot be re-purposed.

- `POST /devices` accepts two more optional fields next to `project_access`: `class`
  (`person` or `agent`) and `label` (1 to 80 characters, no control characters, shown in the
  listing; never put a secret in it). A `label` without a `class`, an unknown class or a bad
  label is 400. The response gains `class`. An exact retry returns the class the key has.
- `GET /devices` gains `class` (`person` unless the key was enrolled as an agent key) and
  `label` (null when none) on every key. The `scopes` list is unchanged: it names the scopes
  of a person's key.
- The agent scope is `content:read` (docs/AI-AGENT-PROTOCOL-SPEC.md section 14.2). It is
  independent of the other scopes. `PUT /devices/{key}/projects/{project}` with `content:read`
  on a person's key is 400, and with any other scope on an agent key is 400. Issuance
  (`POST /projects/{id}/device-sessions`) gives the same mix the uniform 403.
- The `all` mode never covers `content:read`: even an agent key in `all` mode needs an
  active grant for the project, checked at issuance and on every request.
- A session with `content:read` is accepted only by the agent routes
  (`/projects/{id}/agent/...`, behind `AGENT_GATEWAY=on`); every other route refuses it, and
  the agent routes refuse every other credential.
- `device.enrolled` audit rows carry `class` when the enrolment named one.
- Schema 43: a grant and a session are necessary but not sufficient. The agent routes also
  need the project's agent policy to be enabled by an owner or manager (`PUT
  /projects/{id}/agent-policy`); until then every request is the uniform `403`, whoever holds
  the key. Creating a key or a grant, and opening a session, are not refused meanwhile. The
  project's kill switch (`POST /projects/{id}/agent-policy/kill`) revokes every active grant of
  an agent key for the project (one `device.grant.revoked` audit row each, detail `reason:
  agent_killswitch`, actor the manager) and refuses every session issued before it, so granting
  the key again does not revive them. An ordinary revocation (`DELETE`) does not cut sessions
  off: granting again revives an unexpired session.

## Not in this contract

No per-grant expiry, no device names, no grant approval workflow for another
administrator, no listing of issued sessions, and no change to what a key proves
(possession). The key file itself stays unencrypted; see
[project-scoped device sessions](PROJECT-SCOPED-DEVICE-SESSIONS.md).
