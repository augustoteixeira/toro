use std::net::IpAddr;
use std::time::Duration;

use maud::html;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use server::{
    Db, RateLimiter, Reading, TokenAuthenticated, ensure_token, generate_day_json,
    get_readings_for_day, insert_reading, migrate,
};

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
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    reading: Json<Reading>,
) -> Status {
    if limiter.too_many_attempts(ip, 10, Duration::from_secs(60)) {
        return Status::TooManyRequests;
    }
    let reading = reading.into_inner();
    let date = reading.hour.chars().take(10).collect::<String>();
    match insert_reading(&db.0, &reading).await {
        Ok(_) => {
            let _ = generate_day_json(&db.0, &date).await;
            Status::Created
        }
        Err(_) => Status::UnprocessableEntity,
    }
}

#[rocket::get("/api/day/<date>")]
async fn api_day(date: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/day/{}.json", date);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

#[rocket::get("/day/<date>")]
async fn day(
    db: &rocket::State<Db>,
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    date: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }
    let readings = get_readings_for_day(&db.0, date).await.unwrap_or_default();
    let markup = html! {
        html {
            head { title { "Toro — " (date) } }
            body {
                h1 { (date) }
                table {
                    tr {
                        th { "Hour" }
                        th { "Temp" }
                        th { "Humidity" }
                        th { "Wind" }
                        th { "Dir" }
                        th { "Light" }
                        th { "Rain" }
                    }
                    @for r in &readings {
                        tr {
                            td { (r.hour.chars().skip(11).collect::<String>()) }
                            td { (r.temperature.map(|v| format!("{v:.1}")).unwrap_or_default()) }
                            td { (r.humidity.map(|v| format!("{v:.1}")).unwrap_or_default()) }
                            td { (r.wind_speed.map(|v| format!("{v:.1}")).unwrap_or_default()) }
                            td { (r.wind_direction.map(|v| format!("{v:.0}")).unwrap_or_default()) }
                            td { (r.luminosity.map(|v| format!("{v:.0}")).unwrap_or_default()) }
                            td { (r.rainfall.map(|v| format!("{v:.1}")).unwrap_or_default()) }
                        }
                    }
                }
            }
        }
    };
    (Status::Ok, markup)
}

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    use rocket_db_pools::Database;

    let rocket = rocket::build()
        .manage(RateLimiter::new())
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
        .mount("/", rocket::routes![index, post_reading, day, api_day])
        .ignite()
        .await?;

    let db = Db::fetch(&rocket).expect("Database not initialized");
    ensure_token(db).await.expect("Failed to ensure API token");

    rocket.launch().await?;

    Ok(())
}
