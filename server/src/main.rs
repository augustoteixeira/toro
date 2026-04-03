use std::net::IpAddr;
use std::time::Duration;

use maud::html;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use server::{
    Db, RateLimiter, Reading, TokenAuthenticated, ensure_token, generate_day_json,
    insert_reading, migrate,
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

fn day_chart_script(api_url: &str) -> String {
    let script = r##"
        const metrics = [
            { field: "temperature", title: "Temperature (\u00b0C)", mark: "line" },
            { field: "humidity", title: "Humidity (%)", mark: "line" },
            { field: "wind_speed", title: "Wind Speed (km/h)", mark: "line" },
            { field: "wind_direction", title: "Wind Direction (\u00b0)", mark: "line" },
            { field: "luminosity", title: "Luminosity (lux)", mark: "line" },
            { field: "rainfall", title: "Rainfall (mm)", mark: "bar" }
        ];

        fetch(API_URL).then(r => r.json()).then(data => {
            data.forEach(d => {
                d.hour = d.hour.substring(11);
            });
            metrics.forEach(m => {
                const spec = {
                    "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
                    "width": "container",
                    "height": 300,
                    "data": { "values": data },
                    "mark": { "type": m.mark, "tooltip": true },
                    "encoding": {
                        "x": { "field": "hour", "type": "ordinal", "title": "Hour" },
                        "y": { "field": m.field, "type": "quantitative", "title": m.title }
                    }
                };
                vegaEmbed("#chart-" + m.field, spec, { "actions": false });
            });
        });
    "##;
    format!("const API_URL = \"{}\";\n{}", api_url, script)
}

#[rocket::get("/day/<date>")]
async fn day(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    date: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }
    let api_url = format!("/api/day/{}", date);
    let markup = html! {
        html {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Toro — " (date) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
            }
            body {
                div.uk-container.uk-margin-top {
                    h1.uk-heading-small { (date) }
                    ul uk-tab="" {
                        li.uk-active { a href="#" { "Temperature" } }
                        li { a href="#" { "Humidity" } }
                        li { a href="#" { "Wind Speed" } }
                        li { a href="#" { "Wind Direction" } }
                        li { a href="#" { "Luminosity" } }
                        li { a href="#" { "Rainfall" } }
                    }
                    ul.uk-switcher.uk-margin {
                        li { div #chart-temperature {} }
                        li { div #chart-humidity {} }
                        li { div #chart-wind_speed {} }
                        li { div #chart-wind_direction {} }
                        li { div #chart-luminosity {} }
                        li { div #chart-rainfall {} }
                    }
                }
                script {
                    (maud::PreEscaped(day_chart_script(&api_url)))
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
