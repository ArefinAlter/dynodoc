//! Opt-in shared draft bases with legacy JSON compatibility. Callers already own
//! document authorization/locks and the transaction; no new permission surface.
use crate::error::ApiError;
use engine_core::{
    materializer::DocumentState,
    shared_checkpoint::{postgres::PgStore, typed, Address, Limits},
};
use engine_shared::DocumentId;
use serde_json::{json, Value};
use sqlx::PgConnection;

fn internal(error: impl std::fmt::Display) -> ApiError {
    ApiError::Internal(error.to_string())
}

pub(crate) async fn encode(
    connection: &mut PgConnection,
    document: DocumentId,
    state: &DocumentState,
    shared: bool,
) -> Result<Value, ApiError> {
    if !shared {
        return Ok(json!(state));
    }
    // Serialize overlapping graph insertions with the periodic/explicit writer.
    // The caller's content lock is compatible with its KEY SHARE erasure guard.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("dynodoc.periodic-checkpoint:{}", document.0))
        .execute(&mut *connection)
        .await?;
    let mut store = PgStore::new(connection);
    let (root, _) = typed::write_state(
        &mut store,
        document,
        state,
        Limits::default(),
        Default::default(),
    )
    .await
    .map_err(ApiError::from)?;
    if typed::read_state(
        &mut store,
        document,
        &root,
        Limits::default(),
        Default::default(),
    )
    .await
    .map_err(internal)?
        != *state
    {
        return Err(internal("draft base reconstruction differs"));
    }
    Ok(json!({"$dynodoc_state":2,"root":root}))
}

pub(crate) async fn decode(
    connection: &mut PgConnection,
    document: DocumentId,
    value: &Value,
) -> Result<DocumentState, ApiError> {
    if value.get("$dynodoc_state").is_some() {
        if value["$dynodoc_state"] != 2 || value.as_object().is_none_or(|object| object.len() != 2)
        {
            return Err(internal("invalid draft state reference version/shape"));
        }
        let root = Address::try_from(
            value["root"]
                .as_str()
                .ok_or_else(|| internal("missing draft state root"))?
                .to_owned(),
        )
        .map_err(internal)?;
        typed::read_state(
            &mut PgStore::new(connection),
            document,
            &root,
            Limits::default(),
            Default::default(),
        )
        .await
        .map_err(internal)
    } else {
        serde_json::from_value(value.clone()).map_err(internal)
    }
}
