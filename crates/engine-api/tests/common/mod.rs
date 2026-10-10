//! Dedicated login in the disposable SQLx database; never tests via SET ROLE.
use sqlx::{postgres::PgPoolOptions, PgPool};

pub struct RuntimeDatabase {
    pub pool: PgPool,
    pub owner: PgPool,
    pub role: String,
}

impl RuntimeDatabase {
    pub async fn new(owner: PgPool) -> Self {
        let role = format!("runtime_test_{}", uuid::Uuid::new_v4().simple());
        let password = uuid::Uuid::new_v4().simple().to_string();
        sqlx::raw_sql(&format!("create role {role} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls"))
            .execute(&owner).await.unwrap();
        engine_core::database::grant_runtime(&owner, &role)
            .await
            .unwrap();
        let options = owner
            .connect_options()
            .as_ref()
            .clone()
            .username(&role)
            .password(&password);
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap();
        engine_core::database::check_runtime(&pool).await.unwrap();
        Self { pool, owner, role }
    }

    pub async fn close(self) {
        self.pool.close().await;
        // This generated role was checked to own nothing. DROP OWNED only revokes
        // its grants in the SQLx-isolated database; then drop the cluster login.
        sqlx::raw_sql(&format!(
            "drop owned by {}; drop role {}",
            self.role, self.role
        ))
        .execute(&self.owner)
        .await
        .unwrap();
    }
}
