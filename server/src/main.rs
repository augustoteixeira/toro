use std::net::IpAddr;
use std::time::Duration;

use maud::html;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use server::{
    Db, RateLimiter, Reading, TokenAuthenticated, ensure_token, generate_day_json,
    get_all_dates, insert_reading, migrate,
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
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    date: &str,
) -> (Status, (ContentType, String)) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, (ContentType::HTML, "Too many requests".to_string()));
    }
    let page = format!(r##"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Toro — {date}</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
</head>
<body>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css">
  <script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js"></script>

  <div class="uk-container uk-margin-top">
    <h1 class="uk-heading-small">{date}</h1>
    <ul uk-tab>
      <li class="uk-active"><a href="#">Temperature</a></li>
      <li><a href="#">Humidity</a></li>
      <li><a href="#">Wind Speed</a></li>
      <li><a href="#">Wind Direction</a></li>
      <li><a href="#">Luminosity</a></li>
      <li><a href="#">Rainfall</a></li>
    </ul>
    <ul class="uk-switcher uk-margin">
      <li><div id="chart-temperature"></div></li>
      <li><div id="chart-humidity"></div></li>
      <li><div id="chart-wind_speed"></div></li>
      <li><div id="chart-wind_direction"></div></li>
      <li><div id="chart-luminosity"></div></li>
      <li><div id="chart-rainfall"></div></li>
    </ul>
  </div>
  <script>
    var metrics = [
      {{ field: "temperature", title: "Temperature (\u00b0C)", mark: "line" }},
      {{ field: "humidity", title: "Humidity (%)", mark: "line" }},
      {{ field: "wind_speed", title: "Wind Speed (km/h)", mark: "line" }},
      {{ field: "wind_direction", title: "Wind Direction (\u00b0)", mark: "line" }},
      {{ field: "luminosity", title: "Luminosity (lux)", mark: "line" }},
      {{ field: "rainfall", title: "Rainfall (mm)", mark: "bar" }}
    ];
    var chartData = null;

    function renderChart(index) {{
      var m = metrics[index];
      var spec = {{
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": 600,
        "height": 300,
        "data": {{ "values": chartData }},
        "mark": {{ "type": m.mark, "tooltip": true }},
        "encoding": {{
          "x": {{ "field": "hour", "type": "ordinal", "title": "Hour" }},
          "y": {{ "field": m.field, "type": "quantitative", "title": m.title }}
        }}
      }};
      vegaEmbed('#chart-' + m.field, spec, {{ "actions": false }});
    }}

    fetch("/api/day/{date}")
      .then(function(r) {{ return r.json(); }})
      .then(function(data) {{
        data.forEach(function(d) {{ d.hour = d.hour.substring(11); }});
        chartData = data;
        for (var i = 0; i < metrics.length; i++) {{
          renderChart(i);
        }}
      }})
      .catch(function(err) {{
        document.getElementById('chart-temperature').textContent = 'Error: ' + err;
      }});
  </script>
</body>
</html>"##);
    (Status::Ok, (ContentType::HTML, page))
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

    if std::env::args().any(|a| a == "--regenerate") {
        let dates = get_all_dates(db).await.expect("Failed to get dates");
        println!("Regenerating {} day files...", dates.len());
        for date in &dates {
            generate_day_json(db, date).await.expect("Failed to generate JSON");
        }
        println!("Done.");
        return Ok(());
    }

    rocket.launch().await?;

    Ok(())
}
