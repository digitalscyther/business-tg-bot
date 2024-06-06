use std::env;
use serde_json::Value;
use sqlx::{Error, Pool, Postgres};
use sqlx::postgres::PgPoolOptions;
use crate::user::{User, Openai};

pub struct UserRow {
    id: i64,
    business_id: String,
    openai: Value,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        User::new(
            row.id,
            row.business_id,
            row.openai.into(),
        )
    }
}

pub async fn create_pool() -> Pool<Postgres> {
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("Failed to create pool")
}

pub async fn insert_or_update_user(pool: &Pool<Postgres>, id: i64, business_id: &str) -> Result<(), Error> {
    let openai: Value = Openai::default().into();
    sqlx::query!(
        r#"
        INSERT INTO users (id, business_id, openai)
        VALUES ($1, $2, $3)
        ON CONFLICT (id)
        DO UPDATE SET
            business_id = EXCLUDED.business_id
        "#,
        id,
        business_id,
        openai,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn load_user_from_chat_id(pool: &Pool<Postgres>, value: i64) -> Result<User, Error> {
    let row = sqlx::query_as!(
        UserRow,
        r#"
        SELECT id, business_id, openai
        FROM users
        WHERE id = $1
        "#,
        value
    )
        .fetch_one(pool)
        .await?;

    Ok(row.into())
}

pub async fn load_user_from_business_id(pool: &Pool<Postgres>, value: &str) -> Result<User, Error> {
    let row = sqlx::query_as!(
        UserRow,
        r#"
        SELECT id, business_id, openai
        FROM users
        WHERE business_id = $1
        "#,
        value
    )
        .fetch_one(pool)
        .await?;

    Ok(row.into())
}

pub async fn delete_user_by_id(pool: &Pool<Postgres>, value: i64) -> Result<(), Error> {
    sqlx::query!(
        r#"
        DELETE FROM users
        WHERE id = $1
        "#,
        value
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn update_openai_by_id(pool: &Pool<Postgres>, id: i64, openai: Openai) -> Result<(), Error> {
    let openai: Value = openai.into();

    sqlx::query!(
        r#"
        UPDATE users
        SET openai = $1
        WHERE id = $2
        "#,
        openai,
        id,
    )
    .execute(pool)
    .await?;

    Ok(())
}