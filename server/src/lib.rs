use rand::RngCore;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket::Request;
use rocket_db_pools::{Database, sqlx};

#[derive(Database)]
#[database("db")]
pub struct Db(pub sqlx::SqlitePool);

pub async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS meta (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await?;

    let version: Option<String> =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'schema_version'")
            .fetch_optional(pool)
            .await?;

    match version.as_deref().unwrap_or("0") {
        "0" => {
            let sql = std::fs::read_to_string("migrations/001-init.sql")
                .expect("migrations/001-init.sql not found");
            sqlx::query(&sql).execute(pool).await?;
        }
        "1" => {}
        v => panic!("Unknown schema version: {}", v),
    }

    Ok(())
}

pub async fn ensure_token(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
    let existing: Option<String> =
        sqlx::query_scalar("SELECT value FROM meta WHERE key = 'token_hash'")
            .fetch_optional(pool)
            .await?;

    if existing.is_some() {
        return Ok(());
    }

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);

    let hash = bcrypt::hash(&token, bcrypt::DEFAULT_COST)
        .expect("Failed to hash token");

    sqlx::query("INSERT INTO meta (key, value) VALUES ('token_hash', ?)")
        .bind(&hash)
        .execute(pool)
        .await?;

    println!("===========================================");
    println!("  API token (save this, shown only once):");
    println!("  {}", token);
    println!("===========================================");

    Ok(())
}

pub struct TokenAuthenticated;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for TokenAuthenticated {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, ()> {
        let pool = match req.rocket().state::<Db>() {
            Some(db) => &db.0,
            None => return Outcome::Error((Status::InternalServerError, ())),
        };

        let token = match req.headers().get_one("Authorization") {
            Some(header) if header.starts_with("Bearer ") => &header[7..],
            _ => return Outcome::Error((Status::Unauthorized, ())),
        };

        let hash: String = match sqlx::query_scalar(
            "SELECT value FROM meta WHERE key = 'token_hash'",
        )
        .fetch_optional(pool)
        .await
        {
            Ok(Some(h)) => h,
            _ => return Outcome::Error((Status::InternalServerError, ())),
        };

        match bcrypt::verify(token, &hash) {
            Ok(true) => Outcome::Success(TokenAuthenticated),
            _ => Outcome::Error((Status::Unauthorized, ())),
        }
    }
}
