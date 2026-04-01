use maud::html;
use rand::RngCore;
use rocket_db_pools::{Database, sqlx};

#[derive(Database)]
#[database("db")]
struct Db(sqlx::SqlitePool);

async fn migrate(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
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

async fn ensure_token(pool: &sqlx::SqlitePool) -> Result<(), sqlx::Error> {
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

#[rocket::get("/")]
fn index() -> maud::Markup {
    html! {
        html {
            head { title { "Toro" } }
            body {
                h1 { "Hello, world!" }
            }
        }
    }
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    let rocket = rocket::build()
        .attach(Db::init())
        .attach(rocket::fairing::AdHoc::try_on_ignite("Migrations", |rocket| async {
            match Db::fetch(&rocket) {
                Some(db) => {
                    match migrate(db).await {
                        Ok(_) => Ok(rocket),
                        Err(e) => {
                            eprintln!("Migration failed: {}", e);
                            Err(rocket)
                        }
                    }
                }
                None => {
                    eprintln!("No database pool found");
                    Err(rocket)
                }
            }
        }))
        .mount("/", rocket::routes![index])
        .ignite()
        .await?;

    let db = Db::fetch(&rocket).expect("Database not initialized");
    ensure_token(db).await.expect("Failed to ensure API token");

    rocket.launch().await?;

    Ok(())
}
