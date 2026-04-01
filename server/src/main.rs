use maud::html;
use server::{Db, ensure_token, migrate};

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
        .mount("/", rocket::routes![index])
        .ignite()
        .await?;

    let db = Db::fetch(&rocket).expect("Database not initialized");
    ensure_token(db).await.expect("Failed to ensure API token");

    rocket.launch().await?;

    Ok(())
}
