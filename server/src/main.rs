use std::net::IpAddr;
use std::time::Duration;

use chrono::{Datelike, NaiveDate};

use maud::html;
use rocket::http::{ContentType, Status};
use rocket::serde::json::Json;
use server::{
    Db, RateLimiter, Reading, TokenAuthenticated, ensure_token,
    generate_day_json, generate_month_json, generate_semester_json, generate_triennium_json,
    generate_week_json,
    get_all_dates, get_all_months, get_all_semesters, get_all_triennia, get_all_weeks,
    insert_reading, migrate, monday_of, month_of, semester_start_of, triennium_start_of,
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
            let _ = generate_week_json(&db.0, &monday_of(&reading.hour)).await;
            let _ = generate_month_json(&db.0, &month_of(&reading.hour)).await;
            let _ = generate_semester_json(&db.0, &semester_start_of(&reading.hour)).await;
            let _ = generate_triennium_json(&db.0, &triennium_start_of(&reading.hour)).await;
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

fn static_exists(span: &str, key: &str) -> bool {
    std::path::Path::new(&format!("data/static/{}/{}.json", span, key)).exists()
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

    let monday = monday_of(&format!("{}T00", date));
    let week_label = format!("Week of {}", monday);

    let parsed = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok();
    let prev = parsed
        .map(|d| (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        .filter(|d| static_exists("day", d));
    let next = parsed
        .map(|d| (d + chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        .filter(|d| static_exists("day", d));

    let markup = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toro — " (date) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { (date) }

                    // Week button — centered
                    div."uk-text-center"."uk-margin-small-bottom" {
                        a."uk-button"."uk-button-default" href={ "/week/" (monday) } {
                            (week_label)
                        }
                    }

                    // Prev / Next day buttons
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-default" href={ "/day/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-default" href={ "/day/" (n) } {
                                    (n) " →"
                                }
                            }
                        }
                    }
                    div style="clear:both" {}

                    ul uk-tab="" {
                        li."uk-active" { a href="#" { "Temperature" } }
                        li { a href="#" { "Humidity" } }
                        li { a href="#" { "Wind Speed" } }
                        li { a href="#" { "Wind Direction" } }
                        li { a href="#" { "Luminosity" } }
                        li { a href="#" { "Rainfall" } }
                    }
                    ul."uk-switcher"."uk-margin" {
                        li { div #chart-temperature {} }
                        li { div #chart-humidity {} }
                        li { div #chart-wind_speed {} }
                        li { div #chart-wind_direction {} }
                        li { div #chart-luminosity {} }
                        li { div #chart-rainfall {} }
                    }
                }
                script {
                    (maud::PreEscaped(format!(r##"
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
    "width": 600, "height": 300,
    "data": {{ "values": chartData }},
    "mark": {{ "type": m.mark, "tooltip": true }},
    "encoding": {{
      "x": {{ "field": "hour", "type": "ordinal", "title": "Hour" }},
      "y": {{ "field": m.field, "type": "quantitative", "title": m.title }}
    }}
  }};
  vegaEmbed("#chart-" + m.field, spec, {{ "actions": false }});
}}
fetch("/api/day/{date}")
  .then(function(r) {{ return r.json(); }})
  .then(function(data) {{
    data.forEach(function(d) {{ d.hour = d.hour.substring(11); }});
    chartData = data;
    for (var i = 0; i < metrics.length; i++) {{ renderChart(i); }}
  }})
  .catch(function(err) {{
    document.getElementById("chart-temperature").textContent = "Error: " + err;
  }});
                    "##)))
                }
            }
        }
    };
    (Status::Ok, markup)
}

#[rocket::get("/api/week/<monday>")]
async fn api_week(monday: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/week/{}.json", monday);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

#[rocket::get("/week/<monday>")]
async fn week(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    monday: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }

    let parsed = NaiveDate::parse_from_str(monday, "%Y-%m-%d").ok();

    // Up: month containing this week's Wednesday
    let wednesday = parsed.map(|d| d + chrono::Duration::days(2));
    let month_key = wednesday.map(|w| w.format("%Y-%m").to_string());

    // Prev / next week
    let prev = parsed
        .map(|d| (d - chrono::Duration::weeks(1)).format("%Y-%m-%d").to_string())
        .filter(|k| static_exists("week", k));
    let next = parsed
        .map(|d| (d + chrono::Duration::weeks(1)).format("%Y-%m-%d").to_string())
        .filter(|k| static_exists("week", k));

    // 7 day buttons
    let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let days: Vec<(String, String, bool)> = (0..7)
        .map(|i| {
            let date = parsed
                .map(|d| (d + chrono::Duration::days(i)).format("%Y-%m-%d").to_string())
                .unwrap_or_default();
            let label = format!("{} {}", day_names[i as usize], &date[8..]);
            let exists = static_exists("day", &date);
            (date, label, exists)
        })
        .collect();

    let markup = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toro — Week of " (monday) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { "Week of " (monday) }

                    // Up: month button
                    @if let Some(ref mk) = month_key {
                        div."uk-text-center"."uk-margin-small-bottom" {
                            a."uk-button"."uk-button-default" href={ "/month/" (mk) } {
                                (mk)
                            }
                        }
                    }

                    // Prev / Next week
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-default" href={ "/week/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-default" href={ "/week/" (n) } {
                                    (n) " →"
                                }
                            }
                        }
                    }
                    div style="clear:both" {}

                    // 7 day buttons
                    div."uk-text-center"."uk-margin-small-bottom" {
                        @for (date, label, exists) in &days {
                            @if *exists {
                                a."uk-button"."uk-button-default"."uk-button-small"."uk-margin-small-right" href={ "/day/" (date) } {
                                    (label)
                                }
                            }
                        }
                    }

                    ul uk-tab="" {
                        li."uk-active" { a href="#" { "Temperature" } }
                        li { a href="#" { "Humidity" } }
                        li { a href="#" { "Wind Speed" } }
                        li { a href="#" { "Wind Direction" } }
                        li { a href="#" { "Luminosity" } }
                        li { a href="#" { "Rainfall" } }
                    }
                    ul."uk-switcher"."uk-margin" {
                        li { div #chart-temperature {} }
                        li { div #chart-humidity {} }
                        li { div #chart-wind_speed {} }
                        li { div #chart-wind_direction {} }
                        li { div #chart-luminosity {} }
                        li { div #chart-rainfall {} }
                    }
                }
                script {
                    (maud::PreEscaped(week_chart_script(monday)))
                }
            }
        }
    };
    (Status::Ok, markup)
}

fn week_chart_script(monday: &str) -> String {
    format!(r##"
var errorBarMetrics = [
  {{ field: "temperature", title: "Temperature (\u00b0C)" }},
  {{ field: "humidity", title: "Humidity (%)" }},
  {{ field: "wind_speed", title: "Wind Speed (km/h)" }},
  {{ field: "luminosity", title: "Luminosity (lux)" }}
];
fetch("/api/week/{monday}")
  .then(function(r) {{ return r.json(); }})
  .then(function(data) {{
    data.forEach(function(d, i) {{ d._index = i; }});
    errorBarMetrics.forEach(function(m) {{
      var transformed = data.map(function(d) {{
        return {{
          label: d.label, _index: d._index,
          mean: d[m.field + "_mean"],
          lo: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
            ? d[m.field + "_mean"] - d[m.field + "_std"] : null,
          hi: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
            ? d[m.field + "_mean"] + d[m.field + "_std"] : null
        }};
      }});
      vegaEmbed("#chart-" + m.field, {{
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": 700, "height": 300,
        "data": {{ "values": transformed }},
        "encoding": {{
          "x": {{ "field": "_index", "type": "quantitative", "title": "Quarter",
                   "axis": {{ "values": [0,4,8,12,16,20,24],
                              "labelExpr": "['Mon','Tue','Wed','Thu','Fri','Sat','Sun'][floor(datum.value/4)]" }} }}
        }},
        "layer": [
          {{ "mark": {{ "type": "line", "tooltip": true }},
             "encoding": {{ "y": {{ "field": "mean", "type": "quantitative", "title": m.title }} }} }},
          {{ "mark": {{ "type": "errorbar" }},
             "encoding": {{ "y": {{ "field": "lo", "type": "quantitative", "title": m.title }}, "y2": {{ "field": "hi" }} }} }}
        ]
      }}, {{ "actions": false }});
    }});
    vegaEmbed("#chart-wind_direction", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, _index: d._index, direction: d.wind_direction_mean }}; }}) }},
      "mark": {{ "type": "line", "tooltip": true, "point": true }},
      "encoding": {{
        "x": {{ "field": "_index", "type": "quantitative", "title": "Quarter",
                 "axis": {{ "values": [0,4,8,12,16,20,24],
                            "labelExpr": "['Mon','Tue','Wed','Thu','Fri','Sat','Sun'][floor(datum.value/4)]" }} }},
        "y": {{ "field": "direction", "type": "quantitative", "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
      }}
    }}, {{ "actions": false }});
    vegaEmbed("#chart-rainfall", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, _index: d._index, sum: d.rainfall_sum, max: d.rainfall_max }}; }}) }},
      "encoding": {{
        "x": {{ "field": "_index", "type": "quantitative", "title": "Quarter",
                 "axis": {{ "values": [0,4,8,12,16,20,24],
                            "labelExpr": "['Mon','Tue','Wed','Thu','Fri','Sat','Sun'][floor(datum.value/4)]" }} }}
      }},
      "layer": [
        {{ "mark": {{ "type": "bar", "tooltip": true }},
           "encoding": {{ "y": {{ "field": "sum", "type": "quantitative", "title": "Rainfall (mm)" }} }} }},
        {{ "mark": {{ "type": "point", "color": "red", "tooltip": true }},
           "encoding": {{ "y": {{ "field": "max", "type": "quantitative" }} }} }}
      ]
    }}, {{ "actions": false }});
  }})
  .catch(function(err) {{
    document.getElementById("chart-temperature").textContent = "Error: " + err;
  }});
    "##)
}

#[rocket::get("/api/month/<month>")]
async fn api_month(month: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/month/{}.json", month);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

/// Returns all Mondays whose week touches the given month.
fn weeks_touching_month(month: &str) -> Vec<String> {
    let year: i32 = month[..4].parse().unwrap();
    let mo: u32 = month[5..7].parse().unwrap();
    let first_day = NaiveDate::from_ymd_opt(year, mo, 1).unwrap();
    let last_day = NaiveDate::from_ymd_opt(
        if mo == 12 { year + 1 } else { year },
        if mo == 12 { 1 } else { mo + 1 },
        1,
    )
    .unwrap()
        - chrono::Duration::days(1);

    // Monday of the week containing the first day
    let first_monday = first_day
        - chrono::Duration::days(first_day.weekday().num_days_from_monday() as i64);
    // Monday of the week containing the last day
    let last_monday = last_day
        - chrono::Duration::days(last_day.weekday().num_days_from_monday() as i64);

    let mut mondays = vec![];
    let mut cursor = first_monday;
    while cursor <= last_monday {
        mondays.push(cursor.format("%Y-%m-%d").to_string());
        cursor += chrono::Duration::weeks(1);
    }
    mondays
}

fn prev_month(month: &str) -> String {
    let year: i32 = month[..4].parse().unwrap();
    let mo: u32 = month[5..7].parse().unwrap();
    if mo == 1 {
        format!("{}-12", year - 1)
    } else {
        format!("{}-{:02}", year, mo - 1)
    }
}

fn next_month(month: &str) -> String {
    let year: i32 = month[..4].parse().unwrap();
    let mo: u32 = month[5..7].parse().unwrap();
    if mo == 12 {
        format!("{}-01", year + 1)
    } else {
        format!("{}-{:02}", year, mo + 1)
    }
}

#[rocket::get("/month/<month>")]
async fn month(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    month: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }

    // Up: semester containing the 15th of this month
    let sem_key = semester_start_of(&format!("{}-15T00", month));

    // Prev / next month
    let prev = {
        let p = prev_month(month);
        if static_exists("month", &p) { Some(p) } else { None }
    };
    let next = {
        let n = next_month(month);
        if static_exists("month", &n) { Some(n) } else { None }
    };

    // Week buttons
    let weeks: Vec<(String, bool)> = weeks_touching_month(month)
        .into_iter()
        .map(|m| { let e = static_exists("week", &m); (m, e) })
        .collect();

    let markup = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toro — " (month) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { (month) }

                    // Up: semester button
                    div."uk-text-center"."uk-margin-small-bottom" {
                        a."uk-button"."uk-button-default" href={ "/semester/" (sem_key) } {
                            "Semester of " (sem_key)
                        }
                    }

                    // Prev / Next month
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-default" href={ "/month/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-default" href={ "/month/" (n) } {
                                    (n) " →"
                                }
                            }
                        }
                    }
                    div style="clear:both" {}

                    // Week buttons
                    div."uk-text-center"."uk-margin-small-bottom" {
                        @for (monday, exists) in &weeks {
                            @if *exists {
                                a."uk-button"."uk-button-default"."uk-button-small"."uk-margin-small-right" href={ "/week/" (monday) } {
                                    "W " (monday)
                                }
                            }
                        }
                    }

                    ul uk-tab="" {
                        li."uk-active" { a href="#" { "Temperature" } }
                        li { a href="#" { "Humidity" } }
                        li { a href="#" { "Wind Speed" } }
                        li { a href="#" { "Wind Direction" } }
                        li { a href="#" { "Luminosity" } }
                        li { a href="#" { "Rainfall" } }
                    }
                    ul."uk-switcher"."uk-margin" {
                        li { div #chart-temperature {} }
                        li { div #chart-humidity {} }
                        li { div #chart-wind_speed {} }
                        li { div #chart-wind_direction {} }
                        li { div #chart-luminosity {} }
                        li { div #chart-rainfall {} }
                    }
                }
                script {
                    (maud::PreEscaped(month_chart_script(month)))
                }
            }
        }
    };
    (Status::Ok, markup)
}

fn month_chart_script(month: &str) -> String {
    format!(r##"
var errorBarMetrics = [
  {{ field: "temperature", title: "Temperature (\u00b0C)" }},
  {{ field: "humidity", title: "Humidity (%)" }},
  {{ field: "wind_speed", title: "Wind Speed (km/h)" }},
  {{ field: "luminosity", title: "Luminosity (lux)" }}
];
fetch("/api/month/{month}")
  .then(function(r) {{ return r.json(); }})
  .then(function(data) {{
    errorBarMetrics.forEach(function(m) {{
      var transformed = data.map(function(d) {{
        return {{
          label: d.label, mean: d[m.field + "_mean"],
          lo: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
            ? d[m.field + "_mean"] - d[m.field + "_std"] : null,
          hi: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
            ? d[m.field + "_mean"] + d[m.field + "_std"] : null
        }};
      }});
      vegaEmbed("#chart-" + m.field, {{
        "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
        "width": 700, "height": 300,
        "data": {{ "values": transformed }},
        "encoding": {{
          "x": {{ "field": "label", "type": "ordinal", "title": "Day", "axis": {{ "labelAngle": -45 }} }}
        }},
        "layer": [
          {{ "mark": {{ "type": "line", "tooltip": true }},
             "encoding": {{ "y": {{ "field": "mean", "type": "quantitative", "title": m.title }} }} }},
          {{ "mark": {{ "type": "errorbar" }},
             "encoding": {{ "y": {{ "field": "lo", "type": "quantitative", "title": m.title }}, "y2": {{ "field": "hi" }} }} }}
        ]
      }}, {{ "actions": false }});
    }});
    vegaEmbed("#chart-wind_direction", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, direction: d.wind_direction_mean }}; }}) }},
      "mark": {{ "type": "line", "tooltip": true, "point": true }},
      "encoding": {{
        "x": {{ "field": "label", "type": "ordinal", "title": "Day", "axis": {{ "labelAngle": -45 }} }},
        "y": {{ "field": "direction", "type": "quantitative", "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
      }}
    }}, {{ "actions": false }});
    vegaEmbed("#chart-rainfall", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, sum: d.rainfall_sum, max: d.rainfall_max }}; }}) }},
      "encoding": {{
        "x": {{ "field": "label", "type": "ordinal", "title": "Day", "axis": {{ "labelAngle": -45 }} }}
      }},
      "layer": [
        {{ "mark": {{ "type": "bar", "tooltip": true }},
           "encoding": {{ "y": {{ "field": "sum", "type": "quantitative", "title": "Rainfall (mm)" }} }} }},
        {{ "mark": {{ "type": "point", "color": "red", "tooltip": true }},
           "encoding": {{ "y": {{ "field": "max", "type": "quantitative" }} }} }}
      ]
    }}, {{ "actions": false }});
  }})
  .catch(function(err) {{
    document.getElementById("chart-temperature").textContent = "Error: " + err;
  }});
    "##)
}

#[rocket::get("/api/semester/<start>")]
async fn api_semester(start: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/semester/{}.json", start);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

#[rocket::get("/semester/<start>")]
async fn semester(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    start: &str,
) -> (Status, (ContentType, String)) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, (ContentType::HTML, "Too many requests".to_string()));
    }
    let page = format!(r##"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Toro — Semester {start}</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css">
  <script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js"></script>
</head>
<body>
  <div class="uk-container uk-margin-top">
    <h1 class="uk-heading-small">Semester from {start}</h1>
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
    var errorBarMetrics = [
      {{ field: "temperature", title: "Temperature (\u00b0C)" }},
      {{ field: "humidity", title: "Humidity (%)" }},
      {{ field: "wind_speed", title: "Wind Speed (km/h)" }},
      {{ field: "luminosity", title: "Luminosity (lux)" }}
    ];

    fetch("/api/semester/{start}")
      .then(function(r) {{ return r.json(); }})
      .then(function(data) {{
        errorBarMetrics.forEach(function(m) {{
          var transformed = data.map(function(d) {{
            return {{
              label: d.label,
              mean: d[m.field + "_mean"],
              lo: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
                ? d[m.field + "_mean"] - d[m.field + "_std"] : null,
              hi: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
                ? d[m.field + "_mean"] + d[m.field + "_std"] : null
            }};
          }});
          vegaEmbed('#chart-' + m.field, {{
            "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
            "width": 700, "height": 300,
            "data": {{ "values": transformed }},
            "encoding": {{
              "x": {{ "field": "label", "type": "ordinal", "title": "Week",
                       "axis": {{ "labelAngle": -45 }} }}
            }},
            "layer": [
              {{
                "mark": {{ "type": "line", "tooltip": true }},
                "encoding": {{ "y": {{ "field": "mean", "type": "quantitative", "title": m.title }} }}
              }},
              {{
                "mark": {{ "type": "errorbar" }},
                "encoding": {{
                  "y": {{ "field": "lo", "type": "quantitative", "title": m.title }},
                  "y2": {{ "field": "hi" }}
                }}
              }}
            ]
          }}, {{ "actions": false }});
        }});

        vegaEmbed('#chart-wind_direction', {{
          "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
          "width": 700, "height": 300,
          "data": {{ "values": data.map(function(d) {{
            return {{ label: d.label, direction: d.wind_direction_mean }};
          }}) }},
          "mark": {{ "type": "line", "tooltip": true, "point": true }},
          "encoding": {{
            "x": {{ "field": "label", "type": "ordinal", "title": "Week",
                     "axis": {{ "labelAngle": -45 }} }},
            "y": {{ "field": "direction", "type": "quantitative",
                     "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
          }}
        }}, {{ "actions": false }});

        vegaEmbed('#chart-rainfall', {{
          "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
          "width": 700, "height": 300,
          "data": {{ "values": data.map(function(d) {{
            return {{ label: d.label, sum: d.rainfall_sum, max: d.rainfall_max }};
          }}) }},
          "encoding": {{
            "x": {{ "field": "label", "type": "ordinal", "title": "Week",
                     "axis": {{ "labelAngle": -45 }} }}
          }},
          "layer": [
            {{
              "mark": {{ "type": "bar", "tooltip": true }},
              "encoding": {{ "y": {{ "field": "sum", "type": "quantitative", "title": "Rainfall (mm)" }} }}
            }},
            {{
              "mark": {{ "type": "point", "color": "red", "tooltip": true }},
              "encoding": {{ "y": {{ "field": "max", "type": "quantitative" }} }}
            }}
          ]
        }}, {{ "actions": false }});
      }})
      .catch(function(err) {{
        document.getElementById('chart-temperature').textContent = 'Error: ' + err;
      }});
  </script>
</body>
</html>"##);
    (Status::Ok, (ContentType::HTML, page))
}

#[rocket::get("/api/triennium/<start>")]
async fn api_triennium(start: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/triennium/{}.json", start);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

#[rocket::get("/triennium/<start>")]
async fn triennium(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    start: &str,
) -> (Status, (ContentType, String)) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, (ContentType::HTML, "Too many requests".to_string()));
    }
    let page = format!(r##"<!DOCTYPE html>
<html>
<head>
  <meta charset="utf-8">
  <title>Toro — Triennium {start}</title>
  <script src="https://cdn.jsdelivr.net/npm/vega@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-lite@5"></script>
  <script src="https://cdn.jsdelivr.net/npm/vega-embed@6"></script>
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css">
  <script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js"></script>
</head>
<body>
  <div class="uk-container uk-margin-top">
    <h1 class="uk-heading-small">Triennium from {start}</h1>
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
    var errorBarMetrics = [
      {{ field: "temperature", title: "Temperature (\u00b0C)" }},
      {{ field: "humidity", title: "Humidity (%)" }},
      {{ field: "wind_speed", title: "Wind Speed (km/h)" }},
      {{ field: "luminosity", title: "Luminosity (lux)" }}
    ];

    fetch("/api/triennium/{start}")
      .then(function(r) {{ return r.json(); }})
      .then(function(data) {{
        errorBarMetrics.forEach(function(m) {{
          var transformed = data.map(function(d) {{
            return {{
              label: d.label,
              mean: d[m.field + "_mean"],
              lo: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
                ? d[m.field + "_mean"] - d[m.field + "_std"] : null,
              hi: d[m.field + "_mean"] !== null && d[m.field + "_std"] !== null
                ? d[m.field + "_mean"] + d[m.field + "_std"] : null
            }};
          }});
          vegaEmbed('#chart-' + m.field, {{
            "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
            "width": 700, "height": 300,
            "data": {{ "values": transformed }},
            "encoding": {{
              "x": {{ "field": "label", "type": "ordinal", "title": "Month",
                       "axis": {{ "labelAngle": -45 }} }}
            }},
            "layer": [
              {{
                "mark": {{ "type": "line", "tooltip": true }},
                "encoding": {{ "y": {{ "field": "mean", "type": "quantitative", "title": m.title }} }}
              }},
              {{
                "mark": {{ "type": "errorbar" }},
                "encoding": {{
                  "y": {{ "field": "lo", "type": "quantitative", "title": m.title }},
                  "y2": {{ "field": "hi" }}
                }}
              }}
            ]
          }}, {{ "actions": false }});
        }});

        vegaEmbed('#chart-wind_direction', {{
          "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
          "width": 700, "height": 300,
          "data": {{ "values": data.map(function(d) {{
            return {{ label: d.label, direction: d.wind_direction_mean }};
          }}) }},
          "mark": {{ "type": "line", "tooltip": true, "point": true }},
          "encoding": {{
            "x": {{ "field": "label", "type": "ordinal", "title": "Month",
                     "axis": {{ "labelAngle": -45 }} }},
            "y": {{ "field": "direction", "type": "quantitative",
                     "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
          }}
        }}, {{ "actions": false }});

        vegaEmbed('#chart-rainfall', {{
          "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
          "width": 700, "height": 300,
          "data": {{ "values": data.map(function(d) {{
            return {{ label: d.label, sum: d.rainfall_sum, max: d.rainfall_max }};
          }}) }},
          "encoding": {{
            "x": {{ "field": "label", "type": "ordinal", "title": "Month",
                     "axis": {{ "labelAngle": -45 }} }}
          }},
          "layer": [
            {{
              "mark": {{ "type": "bar", "tooltip": true }},
              "encoding": {{ "y": {{ "field": "sum", "type": "quantitative", "title": "Rainfall (mm)" }} }}
            }},
            {{
              "mark": {{ "type": "point", "color": "red", "tooltip": true }},
              "encoding": {{ "y": {{ "field": "max", "type": "quantitative" }} }}
            }}
          ]
        }}, {{ "actions": false }});
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
        .mount("/", rocket::routes![index, post_reading, day, api_day, week, api_week, month, api_month, semester, api_semester, triennium, api_triennium])
        .ignite()
        .await?;

    let db = Db::fetch(&rocket).expect("Database not initialized");
    ensure_token(db).await.expect("Failed to ensure API token");

    if std::env::args().any(|a| a == "--regenerate") {
        let dates = get_all_dates(db).await.expect("Failed to get dates");
        println!("Regenerating {} day files...", dates.len());
        for date in &dates {
            generate_day_json(db, date).await.expect("Failed to generate day JSON");
        }

        let weeks = get_all_weeks(db).await.expect("Failed to get weeks");
        println!("Regenerating {} week files...", weeks.len());
        for monday in &weeks {
            generate_week_json(db, monday).await.expect("Failed to generate week JSON");
        }

        let months = get_all_months(db).await.expect("Failed to get months");
        println!("Regenerating {} month files...", months.len());
        for month in &months {
            generate_month_json(db, month).await.expect("Failed to generate month JSON");
        }

        let semesters = get_all_semesters(db).await.expect("Failed to get semesters");
        println!("Regenerating {} semester files...", semesters.len());
        for sem in &semesters {
            generate_semester_json(db, sem).await.expect("Failed to generate semester JSON");
        }

        let triennia = get_all_triennia(db).await.expect("Failed to get triennia");
        println!("Regenerating {} triennium files...", triennia.len());
        for tri in &triennia {
            generate_triennium_json(db, tri).await.expect("Failed to generate triennium JSON");
        }

        println!("Done.");
        return Ok(());
    }

    rocket.launch().await?;

    Ok(())
}
