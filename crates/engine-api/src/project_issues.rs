//! Project issues, labels and stars. See docs/PROJECT-ISSUES-API.md.
//!
//! An issue is a tracked question or task about a project; it never touches the
//! event log. Membership is read from the live tables on every request, writes hold
//! the project row `FOR SHARE` (member and settings changes take it `FOR UPDATE`),
//! and each query runs under a 3 second database deadline and the existing
//! per-account/per-project admission budget. Project device tokens are not accepted:
//! every handler takes the plain account extractor.
use crate::{
    auth::AuthContext, error::ApiError, product::audit, project_activity::person, projects,
    workloads::ReadPermit, AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State},
    routing::{get, post, put},
    Json, Router,
};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use std::collections::BTreeSet;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/:id/issues", get(list_issues).post(create_issue))
        .route(
            "/projects/:id/issues/:number",
            get(get_issue).patch(update_issue),
        )
        .route(
            "/projects/:id/issues/:number/comments",
            post(create_comment),
        )
        .route("/projects/:id/labels", get(list_labels).post(create_label))
        .route(
            "/projects/:id/labels/:label",
            axum::routing::patch(update_label).delete(delete_label),
        )
        .route("/projects/:id/star", put(star).delete(unstar))
        .layer(DefaultBodyLimit::max(128 * 1024))
}

const MAX_ISSUES: i32 = 5000;
const MAX_LABELS: i64 = 100;
const MAX_ISSUE_LABELS: usize = 20;
const MAX_COMMENTS: i32 = 1000;
const TITLE_CHARS: usize = 200;
const BODY_CHARS: usize = 20_000;
const COMMENT_CHARS: usize = 10_000;
const LABEL_NAME_CHARS: usize = 50;
const LABEL_DESCRIPTION_CHARS: usize = 200;
const PRIOR_BODY_CHARS: usize = 2000;
const DEFAULT_LIMIT: i64 = 30;
const MAX_LIMIT: i64 = 100;
const DEFAULT_COMMENTS: i64 = 50;
/// Advisory-lock salt: serialises label creation, rename and deletion per project.
const LABEL_LOCK: i64 = 390_001;

type Tx = Transaction<'static, Postgres>;
/// `id, title, body, state, author_id, assignee_id, revision` of a locked issue.
type Current = (Uuid, String, String, String, Uuid, Option<Uuid>, i64);

fn bad(reason: &str) -> ApiError {
    ApiError::BadRequest {
        reason: reason.into(),
    }
}
fn denied(reason: &str) -> ApiError {
    ApiError::Denied {
        reason: reason.into(),
    }
}
fn conflict(reason: &str) -> ApiError {
    ApiError::Conflict {
        reason: reason.into(),
    }
}
fn unique_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.code().as_deref() == Some("23505"))
}

/// The caller's current project role. Rank: owner 5, manager 4, editor 3,
/// approver/reviewer 2, viewer 1.
struct Member {
    role: String,
    rank: u8,
}
impl Member {
    fn can_write(&self) -> bool {
        self.rank >= 2
    }
    fn can_manage(&self) -> bool {
        self.rank >= 4
    }
}
fn rank(role: &str) -> u8 {
    match role {
        "owner" => 5,
        "manager" => 4,
        "editor" => 3,
        "approver" | "reviewer" => 2,
        "viewer" => 1,
        _ => 0,
    }
}

/// Start a request: deadline, current membership and admission. A write also holds
/// the project row `FOR SHARE` so a concurrent removal or role change waits for it
/// (and the reverse). Unknown projects and non-members are the same 403.
async fn begin(
    state: &AppState,
    project: Uuid,
    actor: Uuid,
    write: bool,
) -> Result<(Tx, Member, ReadPermit), ApiError> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("set local statement_timeout='3s'")
        .execute(&mut *tx)
        .await?;
    if write {
        sqlx::query("set local lock_timeout='3s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query_scalar::<_, Uuid>(
            "select id from access_space where id=$1 and kind='project' for share",
        )
        .bind(project)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(ApiError::Forbidden)?;
    }
    let role = projects::require(&mut tx, project, actor).await?;
    let budget = state.workloads.enter(actor, Some(project))?;
    let rank = rank(&role);
    Ok((tx, Member { role, rank }, budget))
}

// --- input -----------------------------------------------------------------

fn one_line(value: &str, what: &str, min: usize, max: usize) -> Result<String, ApiError> {
    let value = value.trim();
    let count = value.chars().count();
    if count < min || count > max || value.chars().any(char::is_control) {
        return Err(bad(&format!(
            "{what} must be {min}-{max} characters on a single line"
        )));
    }
    Ok(value.to_owned())
}
fn text(value: &str, what: &str, min: usize, max: usize) -> Result<String, ApiError> {
    let value = value.replace("\r\n", "\n");
    let value = value.trim();
    let count = value.chars().count();
    if count < min
        || count > max
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err(bad(&format!(
            "{what} must be {min}-{max} characters without control characters"
        )));
    }
    Ok(value.to_owned())
}
fn color(value: &str) -> Result<String, ApiError> {
    let value = value.trim();
    let value = value
        .strip_prefix('#')
        .unwrap_or(value)
        .to_ascii_lowercase();
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(bad("Use a 6-digit hexadecimal colour such as 0969da"));
    }
    Ok(value)
}
fn label_set(labels: &[Uuid]) -> Result<Vec<Uuid>, ApiError> {
    let set: BTreeSet<Uuid> = labels.iter().copied().collect();
    if set.len() > MAX_ISSUE_LABELS {
        return Err(bad("An issue can have at most 20 labels"));
    }
    Ok(set.into_iter().collect())
}
/// Distinguishes an absent field from an explicit `null`.
fn present<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

// --- shapes (documentation only; responses are built by the database) --------

/// A person as shown to project members.
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssuePerson {
    id: Uuid,
    /// Chosen name, else the email's local part; `Deleted account` after erasure.
    name: Option<String>,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueAssignee {
    id: Uuid,
    name: Option<String>,
    /// False when the assignee has since left the project.
    member: bool,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueLabel {
    id: Uuid,
    name: String,
    /// Six lowercase hexadecimal digits, no `#`.
    color: String,
    description: String,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueViewer {
    /// The caller's project role.
    role: String,
    can_comment: bool,
    /// Title, description, labels, assignee and state.
    can_edit: bool,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueComment {
    id: Uuid,
    /// 1, 2, 3... within the issue; page with `after`.
    seq: i32,
    author: IssuePerson,
    body: String,
    created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueCommentPage {
    items: Vec<IssueComment>,
    next_after: Option<i32>,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueSummary {
    id: Uuid,
    number: i32,
    title: String,
    /// `open` or `closed`.
    state: String,
    author: IssuePerson,
    assignee: Option<IssueAssignee>,
    labels: Vec<IssueLabel>,
    comment_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueCounts {
    open: i64,
    closed: i64,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssuePage {
    items: Vec<IssueSummary>,
    /// Send as `before` for the next page; null on the last page.
    next_before: Option<i32>,
    /// Project totals, ignoring the filters.
    counts: IssueCounts,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct Issue {
    id: Uuid,
    number: i32,
    title: String,
    state: String,
    author: IssuePerson,
    assignee: Option<IssueAssignee>,
    labels: Vec<IssueLabel>,
    comment_count: i32,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
    closed_at: Option<chrono::DateTime<chrono::Utc>>,
    body: String,
    /// Send with edits of title or description; changes on every edit.
    revision: i64,
    closed_by: Option<IssuePerson>,
    viewer: IssueViewer,
    /// Present on `GET` of one issue only.
    comments: Option<IssueCommentPage>,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct IssueCommentCreated {
    comment: IssueComment,
    comment_count: i32,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct LabelWithCount {
    id: Uuid,
    name: String,
    color: String,
    description: String,
    open_issue_count: i64,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct LabelList {
    items: Vec<LabelWithCount>,
    can_manage: bool,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct LabelDeleted {
    ok: bool,
    issues_updated: i64,
}
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
struct StarState {
    starred: bool,
    star_count: i64,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct NewIssue {
    /// 1-200 characters, one line.
    title: String,
    /// Up to 20,000 characters.
    #[serde(default)]
    body: String,
    /// Up to 20 labels of this project.
    #[serde(default)]
    labels: Vec<Uuid>,
    /// A current project member.
    #[serde(default)]
    assignee_id: Option<Uuid>,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct IssueChange {
    title: Option<String>,
    body: Option<String>,
    /// `open` or `closed`.
    state: Option<String>,
    /// The complete new label set; an empty list clears.
    labels: Option<Vec<Uuid>>,
    /// A current project member, or `null` to unassign.
    #[serde(default, deserialize_with = "present")]
    #[schema(value_type = Option<Uuid>)]
    assignee_id: Option<Option<Uuid>>,
    /// Required with `title` or `body`: the issue's current revision.
    revision: Option<i64>,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct NewComment {
    /// 1-10,000 characters.
    body: String,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct NewLabel {
    /// 1-50 characters, unique in the project ignoring case.
    name: String,
    /// Six hexadecimal digits; a leading `#` is accepted.
    color: String,
    /// Up to 200 characters.
    #[serde(default)]
    description: String,
}
#[derive(Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
struct LabelChange {
    name: Option<String>,
    color: Option<String>,
    description: Option<String>,
}

// --- queries ---------------------------------------------------------------

const FROM: &str = "from project_issue x join identity a on a.id=x.author_id left join identity s on s.id=x.assignee_id left join identity c on c.id=x.closed_by";

/// The list-item fields. Rows are aliased `x` (issue), `a` (author), `s` (assignee).
fn summary_sql() -> String {
    format!(
        "jsonb_build_object('id',x.id,'number',x.number,'title',x.title,'state',x.state,\
         'author',jsonb_build_object('id',x.author_id,'name',{author}),\
         'assignee',case when x.assignee_id is null then null else jsonb_build_object('id',x.assignee_id,'name',{assignee},\
           'member',(s.disabled_at is null and exists(select 1 from space_member m where m.space_id=x.project_id and m.identity_id=x.assignee_id))) end,\
         'labels',coalesce((select jsonb_agg(jsonb_build_object('id',l.id,'name',l.name,'color',l.color,'description',l.description) order by lower(l.name),l.id) \
           from project_issue_label il join project_label l on l.id=il.label_id where il.issue_id=x.id),'[]'::jsonb),\
         'comment_count',x.comment_count,'created_at',x.created_at,'updated_at',x.updated_at,'closed_at',x.closed_at)",
        author = person("a"),
        assignee = person("s"),
    )
}

/// One issue with its text, revision and the caller's abilities (no comments).
async fn issue_json(
    db: &mut PgConnection,
    project: Uuid,
    number: i32,
    actor: Uuid,
    member: &Member,
) -> Result<Value, ApiError> {
    let sql = format!(
        "select {summary}||jsonb_build_object('body',x.body,'revision',x.revision,\
         'closed_by',case when c.id is null then null else jsonb_build_object('id',c.id,'name',{closer}) end),x.author_id \
         {FROM} where x.project_id=$1 and x.number=$2",
        summary = summary_sql(),
        closer = person("c"),
    );
    let (mut value, author): (Value, Uuid) = sqlx::query_as(&sql)
        .bind(project)
        .bind(number)
        .fetch_optional(db)
        .await?
        .ok_or(ApiError::NotFound)?;
    value["viewer"] = json!({
        "role": member.role,
        "can_comment": member.can_write(),
        "can_edit": member.can_write() && (author == actor || member.can_manage()),
    });
    Ok(value)
}

async fn star_count(db: &mut PgConnection, project: Uuid) -> Result<i64, ApiError> {
    Ok(sqlx::query_scalar(
        "select count(*) from project_star st join space_member m on m.space_id=st.project_id and m.identity_id=st.identity_id join identity i on i.id=st.identity_id and i.disabled_at is null where st.project_id=$1",
    )
    .bind(project)
    .fetch_one(db)
    .await?)
}

/// An id, `me`, or (when allowed) `none`.
fn who(value: Option<&str>, me: Uuid, allow_none: bool) -> Result<(Option<Uuid>, bool), ApiError> {
    match value.map(str::trim).filter(|v| !v.is_empty()) {
        None => Ok((None, false)),
        Some("me") => Ok((Some(me), false)),
        Some("none") if allow_none => Ok((None, true)),
        Some(id) => Uuid::parse_str(id)
            .map(|id| (Some(id), false))
            .map_err(|_| bad("Filter people by account id or me")),
    }
}

#[derive(Deserialize)]
struct ListQuery {
    state: Option<String>,
    label: Option<String>,
    assignee: Option<String>,
    author: Option<String>,
    q: Option<String>,
    before: Option<i64>,
    limit: Option<i64>,
}

#[utoipa::path(operation_id="project_issues_list", get, path="/projects/{id}/issues", tag="project issues", params(("id"=Uuid,Path,description="Project"),("state"=Option<String>,Query,description="open (default), closed or all"),("label"=Option<String>,Query,description="A label id or name"),("assignee"=Option<String>,Query,description="An account id, me or none"),("author"=Option<String>,Query,description="An account id or me"),("q"=Option<String>,Query,description="Title substring, up to 100 characters"),("before"=Option<i32>,Query,description="Issue number: only smaller numbers"),("limit"=Option<i64>,Query,description="1-100 (default 30)")), security(("paseto"=[])), responses((status=200,description="Newest issue number first, with the next page marker and project open/closed totals",body=IssuePage),(status=400,description="Invalid filter"),(status=403,description="Project membership required"),(status=429,description="Too many reads"),(status=504,description="Deadline exceeded")))]
async fn list_issues(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let filter = match query.state.as_deref().unwrap_or("open") {
        s @ ("open" | "closed" | "all") => s.to_owned(),
        _ => return Err(bad("Filter by state open, closed or all")),
    };
    let (assignee, unassigned) = who(query.assignee.as_deref(), auth.identity_id, true)?;
    let (author, _) = who(query.author.as_deref(), auth.identity_id, false)?;
    let q = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .map(str::to_owned);
    let label = query
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned);
    if q.as_ref().is_some_and(|q| q.chars().count() > 100)
        || label.as_ref().is_some_and(|l| l.chars().count() > 64)
        || query
            .before
            .is_some_and(|b| !(1..=i64::from(MAX_ISSUES) + 1).contains(&b))
    {
        return Err(bad("Invalid issue search or page marker"));
    }
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let (mut tx, _, _budget) = begin(&state, project, auth.identity_id, false).await?;
    let sql = format!(
        "select {summary} {FROM} where x.project_id=$1 and ($2::text='all' or x.state=$2) \
         and ($3::uuid is null or x.assignee_id=$3) and (not $4::boolean or x.assignee_id is null) \
         and ($5::uuid is null or x.author_id=$5) \
         and ($6::text is null or strpos(lower(x.title),lower($6))>0) \
         and ($7::text is null or exists(select 1 from project_issue_label il join project_label l on l.id=il.label_id \
           where il.issue_id=x.id and (l.id::text=lower($7) or lower(l.name)=lower($7)))) \
         and ($8::integer is null or x.number<$8) order by x.number desc limit $9",
        summary = summary_sql(),
    );
    let mut items: Vec<Value> = sqlx::query_scalar(&sql)
        .bind(project)
        .bind(&filter)
        .bind(assignee)
        .bind(unassigned)
        .bind(author)
        .bind(q)
        .bind(label)
        .bind(query.before.map(|b| b as i32))
        .bind(limit + 1)
        .fetch_all(&mut *tx)
        .await?;
    let (open, closed): (i64, i64) = sqlx::query_as(
        "select count(*) filter (where state='open'),count(*) filter (where state='closed') from project_issue where project_id=$1",
    )
    .bind(project)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = items.len() as i64 > limit;
    items.truncate(limit as usize);
    let next_before = if more {
        items.last().map(|i| i["number"].clone())
    } else {
        None
    };
    Ok(Json(json!({
        "items": items,
        "next_before": next_before,
        "counts": {"open": open, "closed": closed},
    })))
}

/// Every label must exist in this project; the rows are share-locked so a
/// concurrent delete cannot orphan the assignment mid-request.
async fn require_labels(
    db: &mut PgConnection,
    project: Uuid,
    labels: &[Uuid],
) -> Result<(), ApiError> {
    if labels.is_empty() {
        return Ok(());
    }
    let found: Vec<Uuid> = sqlx::query_scalar(
        "select id from project_label where project_id=$1 and id=any($2) order by id for share",
    )
    .bind(project)
    .bind(labels)
    .fetch_all(db)
    .await?;
    if found.len() != labels.len() {
        return Err(bad("Choose labels that exist in this project"));
    }
    Ok(())
}
async fn require_assignee(
    db: &mut PgConnection,
    project: Uuid,
    assignee: Uuid,
) -> Result<(), ApiError> {
    let role: Option<String> = sqlx::query_scalar("select inherited_space_role($1,$2)")
        .bind(project)
        .bind(assignee)
        .fetch_one(db)
        .await?;
    if role.is_none() {
        return Err(bad("Assign issues to current project members"));
    }
    Ok(())
}

#[utoipa::path(operation_id="project_issues_create", post, path="/projects/{id}/issues", tag="project issues", params(("id"=Uuid,Path,description="Project")), request_body=NewIssue, security(("paseto"=[])), responses((status=200,description="The new issue (numbered per project, never reused)",body=Issue),(status=400,description="Invalid text, label or assignee"),(status=403,description="Project membership with a non-viewer role required"),(status=409,description="The project has 5,000 issues"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn create_issue(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<NewIssue>,
) -> Result<Json<Value>, ApiError> {
    let title = one_line(&input.title, "The title", 1, TITLE_CHARS)?;
    let body = text(&input.body, "The description", 0, BODY_CHARS)?;
    let labels = label_set(&input.labels)?;
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_write() {
        return Err(denied("Viewers can read issues but not open them"));
    }
    require_labels(&mut tx, project, &labels).await?;
    if let Some(assignee) = input.assignee_id {
        require_assignee(&mut tx, project, assignee).await?;
    }
    // The counter row lock serialises numbering; its increment commits with the issue.
    sqlx::query("insert into project_issue_counter(project_id) values($1) on conflict do nothing")
        .bind(project)
        .execute(&mut *tx)
        .await?;
    let number: i32 = sqlx::query_scalar(
        "update project_issue_counter set next_number=next_number+1 where project_id=$1 and next_number<=$2 returning next_number-1",
    )
    .bind(project)
    .bind(MAX_ISSUES)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| conflict("This project has reached the 5,000 issue limit"))?;
    let id: Uuid = sqlx::query_scalar(
        "insert into project_issue(project_id,number,title,body,author_id,assignee_id) values($1,$2,$3,$4,$5,$6) returning id",
    )
    .bind(project)
    .bind(number)
    .bind(&title)
    .bind(&body)
    .bind(auth.identity_id)
    .bind(input.assignee_id)
    .fetch_one(&mut *tx)
    .await?;
    if !labels.is_empty() {
        sqlx::query("insert into project_issue_label(issue_id,project_id,label_id) select $1,$2,u from unnest($3::uuid[]) as u")
            .bind(id)
            .bind(project)
            .bind(&labels)
            .execute(&mut *tx)
            .await?;
    }
    audit(
        &mut tx,
        auth.identity_id,
        "issue.created",
        project,
        json!({"issue_id":id,"number":number,"labels":labels,"assignee_id":input.assignee_id}),
    )
    .await?;
    let issue = issue_json(&mut tx, project, number, auth.identity_id, &member).await?;
    tx.commit().await?;
    Ok(Json(issue))
}

#[derive(Deserialize)]
struct CommentPage {
    after: Option<i32>,
    limit: Option<i64>,
}

#[utoipa::path(operation_id="project_issue_get", get, path="/projects/{id}/issues/{number}", tag="project issues", params(("id"=Uuid,Path,description="Project"),("number"=i32,Path,description="Issue number"),("after"=Option<i32>,Query,description="Comment seq to continue after"),("limit"=Option<i64>,Query,description="Comments per page, 1-100 (default 50)")), security(("paseto"=[])), responses((status=200,description="The issue with its oldest-first page of comments",body=Issue),(status=403,description="Project membership required"),(status=404,description="No such issue"),(status=429,description="Too many reads"),(status=504,description="Deadline exceeded")))]
async fn get_issue(
    State(state): State<AppState>,
    Path((project, number)): Path<(Uuid, i32)>,
    auth: AuthContext,
    Query(page): Query<CommentPage>,
) -> Result<Json<Value>, ApiError> {
    let after = page.after.unwrap_or(0);
    if !(0..=MAX_COMMENTS).contains(&after) {
        return Err(bad("Invalid comment marker"));
    }
    let limit = page.limit.unwrap_or(DEFAULT_COMMENTS).clamp(1, MAX_LIMIT);
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, false).await?;
    let mut issue = issue_json(&mut tx, project, number, auth.identity_id, &member).await?;
    let id = issue["id"]
        .as_str()
        .and_then(|id| Uuid::parse_str(id).ok())
        .ok_or_else(|| ApiError::Internal("issue id missing".into()))?;
    let mut items: Vec<Value> = sqlx::query_scalar(&format!(
        "select jsonb_build_object('id',c.id,'seq',c.seq,'author',jsonb_build_object('id',c.author_id,'name',{who}),'body',c.body,'created_at',c.created_at) \
         from project_issue_comment c join identity a on a.id=c.author_id \
         where c.issue_id=$1 and c.seq>$2 order by c.seq limit $3",
        who = person("a"),
    ))
    .bind(id)
    .bind(after)
    .bind(limit + 1)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let more = items.len() as i64 > limit;
    items.truncate(limit as usize);
    let next_after = if more {
        items.last().map(|c| c["seq"].clone())
    } else {
        None
    };
    issue["comments"] = json!({"items": items, "next_after": next_after});
    Ok(Json(issue))
}

#[utoipa::path(operation_id="project_issue_update", patch, path="/projects/{id}/issues/{number}", tag="project issues", params(("id"=Uuid,Path,description="Project"),("number"=i32,Path,description="Issue number")), request_body=IssueChange, security(("paseto"=[])), responses((status=200,description="The issue after the change (no comments)",body=Issue),(status=400,description="Invalid change, or revision missing"),(status=403,description="Only the author, a manager or an owner may change an issue"),(status=404,description="No such issue"),(status=409,description="The issue changed since this revision"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn update_issue(
    State(state): State<AppState>,
    Path((project, number)): Path<(Uuid, i32)>,
    auth: AuthContext,
    Json(input): Json<IssueChange>,
) -> Result<Json<Value>, ApiError> {
    if input.title.is_none()
        && input.body.is_none()
        && input.state.is_none()
        && input.labels.is_none()
        && input.assignee_id.is_none()
    {
        return Err(bad("Send at least one field to change"));
    }
    if (input.title.is_some() || input.body.is_some()) && input.revision.is_none() {
        return Err(bad(
            "Send the issue revision when editing its title or description",
        ));
    }
    let title = input
        .title
        .as_deref()
        .map(|t| one_line(t, "The title", 1, TITLE_CHARS))
        .transpose()?;
    let body = input
        .body
        .as_deref()
        .map(|b| text(b, "The description", 0, BODY_CHARS))
        .transpose()?;
    let state_change = match input.state.as_deref() {
        None => None,
        Some(s @ ("open" | "closed")) => Some(s),
        Some(_) => return Err(bad("State must be open or closed")),
    };
    let labels = input.labels.as_deref().map(label_set).transpose()?;
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_write() {
        return Err(denied("Viewers can read issues but not change them"));
    }
    let row: Option<Current> = sqlx::query_as(
        "select id,title,body,state,author_id,assignee_id,revision from project_issue where project_id=$1 and number=$2 for update",
    )
    .bind(project)
    .bind(number)
    .fetch_optional(&mut *tx)
    .await?;
    let (id, old_title, old_body, old_state, author, old_assignee, revision) =
        row.ok_or(ApiError::NotFound)?;
    if author != auth.identity_id && !member.can_manage() {
        return Err(denied(
            "Only the issue's author, a manager or an owner can change it",
        ));
    }
    if input.revision.is_some_and(|r| r != revision) {
        return Err(conflict(
            "This issue changed since you opened it. Reload it before saving.",
        ));
    }
    let new_title = title.unwrap_or_else(|| old_title.clone());
    let new_body = body.unwrap_or_else(|| old_body.clone());
    let new_state = state_change.unwrap_or(&old_state).to_owned();
    let new_assignee = input.assignee_id.unwrap_or(old_assignee);
    if new_assignee != old_assignee {
        if let Some(assignee) = new_assignee {
            require_assignee(&mut tx, project, assignee).await?;
        }
    }
    let current: Vec<Uuid> = sqlx::query_scalar(
        "select label_id from project_issue_label where issue_id=$1 order by label_id",
    )
    .bind(id)
    .fetch_all(&mut *tx)
    .await?;
    let (added, removed) = match &labels {
        Some(wanted) => {
            require_labels(&mut tx, project, wanted).await?;
            (
                wanted
                    .iter()
                    .copied()
                    .filter(|l| !current.contains(l))
                    .collect::<Vec<_>>(),
                current
                    .iter()
                    .copied()
                    .filter(|l| !wanted.contains(l))
                    .collect::<Vec<_>>(),
            )
        }
        None => (Vec::new(), Vec::new()),
    };
    let text_changed = new_title != old_title || new_body != old_body;
    let state_changed = new_state != old_state;
    let assignee_changed = new_assignee != old_assignee;
    let labels_changed = !added.is_empty() || !removed.is_empty();
    if text_changed || state_changed || assignee_changed || labels_changed {
        sqlx::query(
            "update project_issue set title=$3,body=$4,state=$5,assignee_id=$6,\
             closed_at=case when $5='closed' then coalesce(closed_at,now()) else null end,\
             closed_by=case when $5='closed' then coalesce(closed_by,$7) else null end,\
             revision=revision+1,updated_at=now() where id=$1 and project_id=$2",
        )
        .bind(id)
        .bind(project)
        .bind(&new_title)
        .bind(&new_body)
        .bind(&new_state)
        .bind(new_assignee)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?;
        if !added.is_empty() {
            sqlx::query("insert into project_issue_label(issue_id,project_id,label_id) select $1,$2,u from unnest($3::uuid[]) as u")
                .bind(id)
                .bind(project)
                .bind(&added)
                .execute(&mut *tx)
                .await?;
        }
        if !removed.is_empty() {
            sqlx::query("delete from project_issue_label where issue_id=$1 and label_id=any($2)")
                .bind(id)
                .bind(&removed)
                .execute(&mut *tx)
                .await?;
        }
    }
    let actor = auth.identity_id;
    if text_changed {
        let mut fields = Vec::new();
        let mut detail = json!({"issue_id":id,"number":number});
        if new_title != old_title {
            fields.push("title");
            detail["previous_title"] = json!(old_title);
        }
        if new_body != old_body {
            fields.push("body");
            detail["previous_body"] =
                json!(old_body.chars().take(PRIOR_BODY_CHARS).collect::<String>());
            detail["previous_body_chars"] = json!(old_body.chars().count());
        }
        detail["fields"] = json!(fields);
        audit(&mut tx, actor, "issue.edited", project, detail).await?;
    }
    if state_changed {
        let action = if new_state == "closed" {
            "issue.closed"
        } else {
            "issue.reopened"
        };
        audit(
            &mut tx,
            actor,
            action,
            project,
            json!({"issue_id":id,"number":number}),
        )
        .await?;
    }
    if assignee_changed {
        let action = if new_assignee.is_some() {
            "issue.assigned"
        } else {
            "issue.unassigned"
        };
        audit(
            &mut tx,
            actor,
            action,
            project,
            json!({"issue_id":id,"number":number,"assignee_id":new_assignee,"previous_assignee_id":old_assignee}),
        )
        .await?;
    }
    if labels_changed {
        audit(
            &mut tx,
            actor,
            "issue.labels_changed",
            project,
            json!({"issue_id":id,"number":number,"added":added,"removed":removed}),
        )
        .await?;
    }
    let issue = issue_json(&mut tx, project, number, actor, &member).await?;
    tx.commit().await?;
    Ok(Json(issue))
}

#[utoipa::path(operation_id="project_issue_comment_create", post, path="/projects/{id}/issues/{number}/comments", tag="project issues", params(("id"=Uuid,Path,description="Project"),("number"=i32,Path,description="Issue number")), request_body=NewComment, security(("paseto"=[])), responses((status=200,description="The immutable comment",body=IssueCommentCreated),(status=400,description="Invalid comment"),(status=403,description="Project membership with a non-viewer role required"),(status=404,description="No such issue"),(status=409,description="The issue has 1,000 comments"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn create_comment(
    State(state): State<AppState>,
    Path((project, number)): Path<(Uuid, i32)>,
    auth: AuthContext,
    Json(input): Json<NewComment>,
) -> Result<Json<Value>, ApiError> {
    let body = text(&input.body, "The comment", 1, COMMENT_CHARS)?;
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_write() {
        return Err(denied("Viewers can read issues but not comment on them"));
    }
    let (id, count): (Uuid, i32) = sqlx::query_as(
        "select id,comment_count from project_issue where project_id=$1 and number=$2 for update",
    )
    .bind(project)
    .bind(number)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    if count >= MAX_COMMENTS {
        return Err(conflict("This issue has reached the 1,000 comment limit"));
    }
    let seq = count + 1;
    sqlx::query("update project_issue set comment_count=$2,updated_at=now() where id=$1")
        .bind(id)
        .bind(seq)
        .execute(&mut *tx)
        .await?;
    let comment: Uuid = sqlx::query_scalar(
        "insert into project_issue_comment(issue_id,seq,author_id,body) values($1,$2,$3,$4) returning id",
    )
    .bind(id)
    .bind(seq)
    .bind(auth.identity_id)
    .bind(&body)
    .fetch_one(&mut *tx)
    .await?;
    audit(
        &mut tx,
        auth.identity_id,
        "issue.commented",
        project,
        json!({"issue_id":id,"number":number,"comment_id":comment,"seq":seq}),
    )
    .await?;
    let item: Value = sqlx::query_scalar(&format!(
        "select jsonb_build_object('id',c.id,'seq',c.seq,'author',jsonb_build_object('id',c.author_id,'name',{who}),'body',c.body,'created_at',c.created_at) \
         from project_issue_comment c join identity a on a.id=c.author_id where c.id=$1",
        who = person("a"),
    ))
    .bind(comment)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"comment": item, "comment_count": seq})))
}

// --- labels ----------------------------------------------------------------

const LABEL_JSON: &str =
    "jsonb_build_object('id',l.id,'name',l.name,'color',l.color,'description',l.description)";

#[utoipa::path(operation_id="project_labels_list", get, path="/projects/{id}/labels", tag="project issues", params(("id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200,description="The project's labels (at most 100) by name, with open-issue counts",body=LabelList),(status=403,description="Project membership required"),(status=429,description="Too many reads"),(status=504,description="Deadline exceeded")))]
async fn list_labels(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, false).await?;
    let items: Vec<Value> = sqlx::query_scalar(&format!(
        "select {LABEL_JSON}||jsonb_build_object('open_issue_count',(select count(*) from project_issue_label il join project_issue i on i.id=il.issue_id and i.state='open' where il.label_id=l.id)) \
         from project_label l where l.project_id=$1 order by lower(l.name),l.id limit $2",
    ))
    .bind(project)
    .bind(MAX_LABELS)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(
        json!({"items": items, "can_manage": member.can_manage()}),
    ))
}

async fn label_lock(db: &mut PgConnection, project: Uuid) -> Result<(), ApiError> {
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text,$2))")
        .bind(project.to_string())
        .bind(LABEL_LOCK)
        .execute(db)
        .await?;
    Ok(())
}

#[utoipa::path(operation_id="project_label_create", post, path="/projects/{id}/labels", tag="project issues", params(("id"=Uuid,Path,description="Project")), request_body=NewLabel, security(("paseto"=[])), responses((status=200,description="The new label",body=IssueLabel),(status=400,description="Invalid name, colour or description"),(status=403,description="Managers and owners only"),(status=409,description="Duplicate name or 100 labels"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn create_label(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
    Json(input): Json<NewLabel>,
) -> Result<Json<Value>, ApiError> {
    let name = one_line(&input.name, "The label name", 1, LABEL_NAME_CHARS)?;
    let color = color(&input.color)?;
    let description = one_line(
        &input.description,
        "The label description",
        0,
        LABEL_DESCRIPTION_CHARS,
    )?;
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_manage() {
        return Err(denied("Only managers and owners can manage labels"));
    }
    label_lock(&mut tx, project).await?;
    let count: i64 = sqlx::query_scalar("select count(*) from project_label where project_id=$1")
        .bind(project)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_LABELS {
        return Err(conflict("This project has reached the 100 label limit"));
    }
    let label: Value = sqlx::query_scalar(&format!(
        "with l as (insert into project_label(project_id,name,color,description,created_by) values($1,$2,$3,$4,$5) returning *) select {LABEL_JSON} from l",
    ))
    .bind(project)
    .bind(&name)
    .bind(&color)
    .bind(&description)
    .bind(auth.identity_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        if unique_violation(&e) {
            conflict("A label with this name already exists")
        } else {
            e.into()
        }
    })?;
    audit(
        &mut tx,
        auth.identity_id,
        "label.created",
        project,
        json!({"label_id":label["id"],"name":name,"color":color}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(label))
}

#[utoipa::path(operation_id="project_label_update", patch, path="/projects/{id}/labels/{label_id}", tag="project issues", params(("id"=Uuid,Path,description="Project"),("label_id"=Uuid,Path,description="Label")), request_body=LabelChange, security(("paseto"=[])), responses((status=200,description="The label after the change",body=IssueLabel),(status=400,description="Invalid name, colour or description"),(status=403,description="Managers and owners only"),(status=404,description="No such label"),(status=409,description="Duplicate name"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn update_label(
    State(state): State<AppState>,
    Path((project, label)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
    Json(input): Json<LabelChange>,
) -> Result<Json<Value>, ApiError> {
    if input.name.is_none() && input.color.is_none() && input.description.is_none() {
        return Err(bad("Send at least one field to change"));
    }
    let name = input
        .name
        .as_deref()
        .map(|n| one_line(n, "The label name", 1, LABEL_NAME_CHARS))
        .transpose()?;
    let new_color = input.color.as_deref().map(color).transpose()?;
    let description = input
        .description
        .as_deref()
        .map(|d| one_line(d, "The label description", 0, LABEL_DESCRIPTION_CHARS))
        .transpose()?;
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_manage() {
        return Err(denied("Only managers and owners can manage labels"));
    }
    label_lock(&mut tx, project).await?;
    let (old_name, old_color, old_description): (String, String, String) = sqlx::query_as(
        "select name,color,description from project_label where id=$1 and project_id=$2 for update",
    )
    .bind(label)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    let name = name.unwrap_or_else(|| old_name.clone());
    let new_color = new_color.unwrap_or_else(|| old_color.clone());
    let description = description.unwrap_or_else(|| old_description.clone());
    let changed = name != old_name || new_color != old_color || description != old_description;
    if changed {
        sqlx::query("update project_label set name=$3,color=$4,description=$5 where id=$1 and project_id=$2")
            .bind(label)
            .bind(project)
            .bind(&name)
            .bind(&new_color)
            .bind(&description)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if unique_violation(&e) {
                    conflict("A label with this name already exists")
                } else {
                    e.into()
                }
            })?;
        audit(
            &mut tx,
            auth.identity_id,
            "label.edited",
            project,
            json!({"label_id":label,"name":name,"color":new_color,"description":description,
                   "previous":{"name":old_name,"color":old_color,"description":old_description}}),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(
        json!({"id":label,"name":name,"color":new_color,"description":description}),
    ))
}

#[utoipa::path(operation_id="project_label_delete", delete, path="/projects/{id}/labels/{label_id}", tag="project issues", params(("id"=Uuid,Path,description="Project"),("label_id"=Uuid,Path,description="Label")), security(("paseto"=[])), responses((status=200,description="The label is removed from every issue",body=LabelDeleted),(status=403,description="Managers and owners only"),(status=404,description="No such label"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn delete_label(
    State(state): State<AppState>,
    Path((project, label)): Path<(Uuid, Uuid)>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, member, _budget) = begin(&state, project, auth.identity_id, true).await?;
    if !member.can_manage() {
        return Err(denied("Only managers and owners can manage labels"));
    }
    label_lock(&mut tx, project).await?;
    let issues: i64 = sqlx::query_scalar(
        "select count(*) from project_issue_label where label_id=$1 and project_id=$2",
    )
    .bind(label)
    .bind(project)
    .fetch_one(&mut *tx)
    .await?;
    let (name, label_color): (String, String) = sqlx::query_as(
        "delete from project_label where id=$1 and project_id=$2 returning name,color",
    )
    .bind(label)
    .bind(project)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(ApiError::NotFound)?;
    audit(
        &mut tx,
        auth.identity_id,
        "label.deleted",
        project,
        json!({"label_id":label,"name":name,"color":label_color,"issues_updated":issues}),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({"ok": true, "issues_updated": issues})))
}

// --- stars -----------------------------------------------------------------

#[utoipa::path(operation_id="project_star_put", put, path="/projects/{id}/star", tag="project issues", params(("id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200,description="The caller's own star is set (idempotent)",body=StarState),(status=403,description="Project membership required"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn star(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, _, _budget) = begin(&state, project, auth.identity_id, true).await?;
    let added = sqlx::query(
        "insert into project_star(project_id,identity_id) values($1,$2) on conflict do nothing",
    )
    .bind(project)
    .bind(auth.identity_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if added > 0 {
        audit(
            &mut tx,
            auth.identity_id,
            "project.starred",
            project,
            json!({}),
        )
        .await?;
    }
    let count = star_count(&mut tx, project).await?;
    tx.commit().await?;
    Ok(Json(json!({"starred": true, "star_count": count})))
}

#[utoipa::path(operation_id="project_star_delete", delete, path="/projects/{id}/star", tag="project issues", params(("id"=Uuid,Path,description="Project")), security(("paseto"=[])), responses((status=200,description="The caller's own star is cleared (idempotent)",body=StarState),(status=403,description="Project membership required"),(status=429,description="Too many requests"),(status=504,description="Deadline exceeded")))]
async fn unstar(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
) -> Result<Json<Value>, ApiError> {
    let (mut tx, _, _budget) = begin(&state, project, auth.identity_id, true).await?;
    let removed = sqlx::query("delete from project_star where project_id=$1 and identity_id=$2")
        .bind(project)
        .bind(auth.identity_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if removed > 0 {
        audit(
            &mut tx,
            auth.identity_id,
            "project.unstarred",
            project,
            json!({}),
        )
        .await?;
    }
    let count = star_count(&mut tx, project).await?;
    tx.commit().await?;
    Ok(Json(json!({"starred": false, "star_count": count})))
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        list_issues,
        create_issue,
        get_issue,
        update_issue,
        create_comment,
        list_labels,
        create_label,
        update_label,
        delete_label,
        star,
        unstar
    ),
    components(schemas(
        NewIssue,
        IssueChange,
        NewComment,
        NewLabel,
        LabelChange,
        IssuePerson,
        IssueAssignee,
        IssueLabel,
        IssueViewer,
        IssueComment,
        IssueCommentPage,
        IssueSummary,
        IssueCounts,
        IssuePage,
        Issue,
        IssueCommentCreated,
        LabelWithCount,
        LabelList,
        LabelDeleted,
        StarState
    ))
)]
pub struct ProjectIssuesApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_rules_are_character_based_and_reject_control_characters() {
        assert_eq!(
            one_line("  Missing evidence  ", "t", 1, 200).unwrap(),
            "Missing evidence"
        );
        assert!(one_line("   ", "t", 1, 200).is_err());
        assert!(one_line("two\nlines", "t", 1, 200).is_err());
        assert!(one_line("nul\0byte", "t", 1, 200).is_err());
        assert!(one_line(&"é".repeat(200), "t", 1, 200).is_ok());
        assert!(one_line(&"é".repeat(201), "t", 1, 200).is_err());
        assert_eq!(text("a\r\nb\tc", "t", 0, 10).unwrap(), "a\nb\tc");
        assert!(text("bell\u{7}", "t", 0, 10).is_err());
        assert!(text("nul\0", "t", 0, 10).is_err());
        assert!(text("", "t", 1, 10).is_err());
        assert!(text("", "t", 0, 10).is_ok());
    }

    #[test]
    fn colour_labels_and_roles_are_normalised() {
        assert_eq!(color("#0969DA").unwrap(), "0969da");
        assert_eq!(color(" 1a7f37 ").unwrap(), "1a7f37");
        assert!(color("#fff").is_err());
        assert!(color("0969dz").is_err());
        assert!(color("##0969da").is_err());
        let a = Uuid::from_u128(1);
        assert_eq!(label_set(&[a, a]).unwrap(), vec![a]);
        let many: Vec<Uuid> = (1..=21).map(Uuid::from_u128).collect();
        assert!(label_set(&many).is_err());
        assert_eq!(rank("viewer"), 1);
        assert_eq!(rank("approver"), rank("reviewer"));
        assert!(rank("manager") > rank("editor"));
        assert_eq!(rank("unknown"), 0);
    }

    #[test]
    fn person_filters_accept_ids_me_and_none() {
        let me = Uuid::from_u128(7);
        let other = Uuid::from_u128(9);
        assert_eq!(who(None, me, true).unwrap(), (None, false));
        assert_eq!(who(Some("me"), me, true).unwrap(), (Some(me), false));
        assert_eq!(who(Some("none"), me, true).unwrap(), (None, true));
        assert!(who(Some("none"), me, false).is_err());
        assert_eq!(
            who(Some(&other.to_string()), me, true).unwrap(),
            (Some(other), false)
        );
        assert!(who(Some("someone"), me, true).is_err());
    }
}
