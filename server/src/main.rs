use maud::html;
use rocket::http::Status;
use rocket::serde::json::Json;
use server::{Db, Reading, TokenAuthenticated, ensure_token, insert_reading, migrate};

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

#[rocket::post("/readings", data = "<reading>")]
async fn post_reading(
    _auth: TokenAuthenticated,
    db: &rocket::State<Db>,
    reading: Json<Reading>,
) -> Status {
    match insert_reading(&db.0, &reading.into_inner()).await {
        Ok(_) => Status::Created,
        Err(_) => Status::UnprocessableEntity,
    }
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    use rocket_db_pools::Database;

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
        .mount("/", rocket::routes![index, post_reading])
        .ignite()
        .await?;

    let db = Db::fetch(&rocket).expect("Database not initialized");
    ensure_token(db).await.expect("Failed to ensure API token");

    rocket.launch().await?;

    Ok(())
}
