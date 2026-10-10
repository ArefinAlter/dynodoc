//! Account-owned profiles, readable only by the account or a shared project member.
use crate::{auth::AuthContext, error::ApiError, AppState};
use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/profile", get(own).post(save))
        .route("/profiles/:id", get(read))
}
async fn profile(state: &AppState, actor: Uuid, id: Uuid) -> Result<Json<Value>, ApiError> {
    let value: Option<Value> = sqlx::query_scalar("select jsonb_build_object('id',i.id,'name',coalesce(i.display_name,''),'bio',i.bio,'revision',i.profile_revision) from identity i where i.id=$2 and i.disabled_at is null and ($1=$2 or exists(select 1 from access_space s where s.kind='project' and inherited_space_role(s.id,$1) is not null and inherited_space_role(s.id,$2) is not null))")
        .bind(actor).bind(id).fetch_optional(&state.pool).await?;
    Ok(Json(value.ok_or(ApiError::NotFound)?))
}
#[utoipa::path(get, path="/profile", security(("paseto"=[])), responses((status=200,description="Own profile, excluding email",body=Value)))]
async fn own(State(state): State<AppState>, auth: AuthContext) -> Result<Json<Value>, ApiError> {
    profile(&state, auth.identity_id, auth.identity_id).await
}
#[utoipa::path(get, path="/profiles/{id}", params(("id"=Uuid,Path)), security(("paseto"=[])), responses((status=200,description="Profile visible to shared project members",body=Value),(status=404,description="Not accessible")))]
async fn read(
    State(state): State<AppState>,
    auth: AuthContext,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, ApiError> {
    profile(&state, auth.identity_id, id).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditProfile {
    name: String,
    bio: String,
    revision: i64,
}
#[utoipa::path(post, path="/profile", request_body=Value, security(("paseto"=[])), responses((status=200,description="Profile saved",body=Value),(status=400,description="Invalid profile"),(status=409,description="Profile changed; reload")))]
async fn save(
    State(state): State<AppState>,
    auth: AuthContext,
    Json(input): Json<EditProfile>,
) -> Result<Json<Value>, ApiError> {
    let name = input.name.trim();
    let bio = input.bio.trim();
    if name.is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
        || bio.chars().count() > 280
        || bio.contains('\0')
        || input.revision < 0
    {
        return Err(ApiError::BadRequest {
            reason: "Use a name of 1–80 characters and a bio of up to 280 characters".into(),
        });
    }
    let changed: Option<i64> = sqlx::query_scalar("update identity set display_name=$2,bio=$3,profile_revision=profile_revision+1 where id=$1 and disabled_at is null and profile_revision=$4 returning profile_revision")
        .bind(auth.identity_id).bind(name).bind(bio).bind(input.revision).fetch_optional(&state.pool).await?;
    let revision = changed.ok_or_else(|| ApiError::Conflict {
        reason: "Your profile changed in another tab. Reload before saving.".into(),
    })?;
    Ok(Json(
        json!({"id":auth.identity_id,"name":name,"bio":bio,"revision":revision}),
    ))
}
