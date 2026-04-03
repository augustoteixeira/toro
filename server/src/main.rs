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
    insert_reading, migrate, monday_of, month_of, semester_start_of, semesters_containing,
    triennia_containing,
};

#[rocket::get("/")]
async fn index(db: &rocket::State<Db>) -> maud::Markup {
    // Collect distinct years from the data
    let years: Vec<i32> = {
        let months = server::get_all_months(db).await.unwrap_or_default();
        let mut ys: Vec<i32> = months.iter()
            .map(|m| m[..4].parse().unwrap_or(0))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        ys
    };

    html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toró" }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { "Toro" }
                    div."uk-margin-top" {
                        @for year in &years {
                            a."uk-button"."uk-button-primary"."uk-margin-small-right"."uk-margin-small-bottom"
                              href={ "/triennium/" (year - 1) } {
                                (year)
                            }
                        }
                    }
                }
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
            for sem_start in semesters_containing(&reading.hour) {
                let _ = generate_semester_json(&db.0, &sem_start).await;
            }
            for tri_start in triennia_containing(&reading.hour) {
                let _ = generate_triennium_json(&db.0, &tri_start).await;
            }
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

/// JS snippet injected into every page's <head>.
/// Handles hash-based tab activation and propagation to nav links.
fn tab_nav_js() -> &'static str {
    r##"<script>
var TAB_SLUGS = ["temperature","humidity","wind-speed","wind-direction","luminosity","rainfall"];

function activeTabIndex() {
  var hash = window.location.hash.replace('#','');
  var idx = TAB_SLUGS.indexOf(hash);
  return idx >= 0 ? idx : 0;
}

function decorateLinks() {
  var hash = window.location.hash;
  var links = document.querySelectorAll('a.uk-button');
  links.forEach(function(a) {
    var href = a.getAttribute('href');
    if (!href) return;
    // Strip any existing hash then append current one
    var base = href.indexOf('#') >= 0 ? href.substring(0, href.indexOf('#')) : href;
    a.setAttribute('href', hash ? base + hash : base);
  });
}

document.addEventListener('DOMContentLoaded', function() {
  var idx = activeTabIndex();
  // Activate the correct tab via UIkit after it initialises
  UIkit.util.on(document, 'beforeshow', function(e) {}, '.uk-switcher');
  setTimeout(function() {
    var tabEl = document.querySelector('[uk-tab]');
    if (tabEl && idx > 0) {
      UIkit.tab(tabEl).show(idx);
    }
    decorateLinks();
  }, 0);

  // Update hash when tab changes
  UIkit.util.on(document, 'shown', function() {
    var tabEl = document.querySelector('[uk-tab]');
    if (!tabEl) return;
    var items = tabEl.querySelectorAll('li');
    for (var i = 0; i < items.length; i++) {
      if (items[i].classList.contains('uk-active')) {
        history.replaceState(null, '', '#' + TAB_SLUGS[i]);
        decorateLinks();
        break;
      }
    }
  });
});
</script>"##
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
                (maud::PreEscaped(tab_nav_js()))
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { (date) }

                    // Week button — centered
                    div."uk-text-center"."uk-margin-small-bottom" {
                        a."uk-button"."uk-button-primary" href={ "/week/" (monday) } {
                            (week_label)
                        }
                    }

                    // Prev / Next day buttons
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-primary" href={ "/day/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-primary" href={ "/day/" (n) } {
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
                (maud::PreEscaped(tab_nav_js()))
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { "Week of " (monday) }

                    // Up: month button
                    @if let Some(ref mk) = month_key {
                        div."uk-text-center"."uk-margin-small-bottom" {
                            a."uk-button"."uk-button-primary" href={ "/month/" (mk) } {
                                (mk)
                            }
                        }
                    }

                    // Prev / Next week
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-primary" href={ "/week/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-primary" href={ "/week/" (n) } {
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
                                a."uk-button"."uk-button-primary"."uk-button-small"."uk-margin-small-right" href={ "/day/" (date) } {
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
                (maud::PreEscaped(tab_nav_js()))
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { (month) }

                    // Up: semester button
                    div."uk-text-center"."uk-margin-small-bottom" {
                        a."uk-button"."uk-button-primary" href={ "/semester/" (sem_key) } {
                            "Semester of " (sem_key)
                        }
                    }

                    // Prev / Next month
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-primary" href={ "/month/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-primary" href={ "/month/" (n) } {
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
                                a."uk-button"."uk-button-primary"."uk-button-small"."uk-margin-small-right" href={ "/week/" (monday) } {
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

/// Months that a semester (26 weeks from the 1st of the given month) touches.
fn months_in_semester(start: &str) -> Vec<String> {
    let start = NaiveDate::parse_from_str(&format!("{}-01", start), "%Y-%m-%d").unwrap();
    let end = start + chrono::Duration::weeks(26) - chrono::Duration::days(1);
    let mut months = vec![];
    let mut cursor_y = start.year();
    let mut cursor_m = start.month();
    loop {
        months.push(format!("{}-{:02}", cursor_y, cursor_m));
        if cursor_y == end.year() && cursor_m == end.month() {
            break;
        }
        if cursor_m == 12 { cursor_y += 1; cursor_m = 1; } else { cursor_m += 1; }
    }
    months
}

/// The triennium whose middle year contains the given semester's midpoint.
/// Triennium key is "YYYY". Middle year of triennium Y is Y+1.
fn triennium_for_semester(start: &str) -> String {
    // Semester key is "YYYY-MM", start date is 1st of that month
    let start_date = NaiveDate::parse_from_str(&format!("{}-01", start), "%Y-%m-%d").unwrap();
    let midpoint = start_date + chrono::Duration::weeks(13);
    // The triennium whose middle year (Y+1) == midpoint.year() => Y = midpoint.year() - 1
    let tri_year = midpoint.year() - 1;
    tri_year.to_string()
}

fn shift_month(ym: &str, delta: i32) -> String {
    let year: i32 = ym[..4].parse().unwrap();
    let mo: u32 = ym[5..7].parse().unwrap();
    let total = year * 12 + mo as i32 + delta;
    let ny = (total - 1) / 12;
    let nm = ((total - 1) % 12 + 1) as u32;
    format!("{}-{:02}", ny, nm)
}

#[rocket::get("/semester/<start>")]
async fn semester(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    start: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }

    // Up: triennium
    let tri_key = triennium_for_semester(start);

    // Prev / next semester (±1 month)
    let prev = {
        let p = shift_month(start, -1);
        if static_exists("semester", &p) { Some(p) } else { None }
    };
    let next = {
        let n = shift_month(start, 1);
        if static_exists("semester", &n) { Some(n) } else { None }
    };

    // Month buttons
    let months: Vec<(String, bool)> = months_in_semester(start)
        .into_iter()
        .map(|m| { let e = static_exists("month", &m); (m, e) })
        .collect();

    let markup = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toro — Semester " (start) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
                (maud::PreEscaped(tab_nav_js()))
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { "Semester from " (start) }

                    // Up: triennium
                    div."uk-text-center"."uk-margin-small-bottom" {
                        a."uk-button"."uk-button-primary" href={ "/triennium/" (tri_key) } {
                            "Triennium " (tri_key)
                        }
                    }

                    // Prev / Next
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-primary" href={ "/semester/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-primary" href={ "/semester/" (n) } {
                                    (n) " →"
                                }
                            }
                        }
                    }
                    div style="clear:both" {}

                    // Month buttons
                    div."uk-text-center"."uk-margin-small-bottom" {
                        @for (mo, exists) in &months {
                            @if *exists {
                                a."uk-button"."uk-button-primary"."uk-button-small"."uk-margin-small-right" href={ "/month/" (mo) } {
                                    (mo)
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
                    (maud::PreEscaped(semester_chart_script(start)))
                }
            }
        }
    };
    (Status::Ok, markup)
}

fn semester_chart_script(start: &str) -> String {
    format!(r##"
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
        "encoding": {{ "x": {{ "field": "label", "type": "ordinal", "title": "Week", "axis": {{ "labelAngle": -45 }} }} }},
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
        "x": {{ "field": "label", "type": "ordinal", "title": "Week", "axis": {{ "labelAngle": -45 }} }},
        "y": {{ "field": "direction", "type": "quantitative", "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
      }}
    }}, {{ "actions": false }});
    vegaEmbed("#chart-rainfall", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, sum: d.rainfall_sum, max: d.rainfall_max }}; }}) }},
      "encoding": {{ "x": {{ "field": "label", "type": "ordinal", "title": "Week", "axis": {{ "labelAngle": -45 }} }} }},
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

#[rocket::get("/api/triennium/<start>")]
async fn api_triennium(start: &str) -> Result<(ContentType, String), Status> {
    let path = format!("data/static/triennium/{}.json", start);
    std::fs::read_to_string(&path)
        .map(|contents| (ContentType::JSON, contents))
        .map_err(|_| Status::NotFound)
}

/// Semester keys ("YYYY-MM") whose 26-week window overlaps with the triennium year range.
/// Triennium year key "YYYY" covers Jan YYYY through Dec YYYY+2.
fn semesters_in_triennium(year_key: &str) -> Vec<String> {
    let tri_year: i32 = year_key.parse().unwrap();
    // Triennium covers months tri_year*12+1 through (tri_year+3)*12
    let tri_start_total = tri_year * 12 + 1;
    let tri_end_total = (tri_year + 3) * 12;

    // A semester starting at month total T covers T to T+5 (approx 26 weeks ≈ 6 months).
    // It overlaps if T+5 >= tri_start_total and T <= tri_end_total.
    let mut semesters = vec![];
    let mut total = tri_start_total - 5;
    while total <= tri_end_total {
        if total > 0 {
            let sy = (total - 1) / 12;
            let sm = ((total - 1) % 12 + 1) as u32;
            // Only show semesters starting in January or June
            if sm == 1 || sm == 6 {
                semesters.push(format!("{}-{:02}", sy, sm));
            }
        }
        total += 1;
    }
    semesters
}

#[rocket::get("/triennium/<start>")]
async fn triennium(
    limiter: &rocket::State<RateLimiter>,
    ip: IpAddr,
    start: &str,
) -> (Status, maud::Markup) {
    if limiter.too_many_attempts(ip, 20, Duration::from_secs(60)) {
        return (Status::TooManyRequests, html! { "Too many requests" });
    }

    let year: i32 = start.parse().unwrap_or(0);

    // Sideways: ±1 year
    let prev_key = (year - 1).to_string();
    let prev = if static_exists("triennium", &prev_key) { Some(prev_key) } else { None };
    let next_key = (year + 1).to_string();
    let next = if static_exists("triennium", &next_key) { Some(next_key) } else { None };

    // Down: semesters
    let semesters: Vec<(String, bool)> = semesters_in_triennium(start)
        .into_iter()
        .map(|s| { let e = static_exists("semester", &s); (s, e) })
        .collect();

    let markup = html! {
        (maud::DOCTYPE)
        html {
            head {
                meta charset="utf-8";
                title { "Toro — Triennium " (start) }
                link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/css/uikit.min.css";
                script src="https://cdn.jsdelivr.net/npm/uikit@3.21.6/dist/js/uikit.min.js" {}
                script src="https://cdn.jsdelivr.net/npm/vega@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-lite@5" {}
                script src="https://cdn.jsdelivr.net/npm/vega-embed@6" {}
                (maud::PreEscaped(tab_nav_js()))
            }
            body {
                div.uk-container."uk-margin-top" {
                    h1."uk-heading-small" { "Triennium from " (start) }

                    // Sideways: prev / next (±1 year)
                    div."uk-margin-small-bottom" {
                        @if let Some(ref p) = prev {
                            a."uk-button"."uk-button-primary" href={ "/triennium/" (p) } {
                                "← " (p)
                            }
                        }
                        @if let Some(ref n) = next {
                            div style="float:right" {
                                a."uk-button"."uk-button-primary" href={ "/triennium/" (n) } {
                                    (n) " →"
                                }
                            }
                        }
                    }
                    div style="clear:both" {}

                    // Down: semester buttons
                    div."uk-text-center"."uk-margin-small-bottom" {
                        @for (sem, exists) in &semesters {
                            @if *exists {
                                a."uk-button"."uk-button-primary"."uk-button-small"."uk-margin-small-right" href={ "/semester/" (sem) } {
                                    "Sem " (sem)
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
                    (maud::PreEscaped(triennium_chart_script(start)))
                }
            }
        }
    };
    (Status::Ok, markup)
}

fn triennium_chart_script(start: &str) -> String {
    format!(r##"
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
        "encoding": {{ "x": {{ "field": "label", "type": "ordinal", "title": "Month", "axis": {{ "labelAngle": -45 }} }} }},
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
        "x": {{ "field": "label", "type": "ordinal", "title": "Month", "axis": {{ "labelAngle": -45 }} }},
        "y": {{ "field": "direction", "type": "quantitative", "title": "Wind Direction (\u00b0)", "scale": {{ "domain": [0, 360] }} }}
      }}
    }}, {{ "actions": false }});
    vegaEmbed("#chart-rainfall", {{
      "$schema": "https://vega.github.io/schema/vega-lite/v5.json",
      "width": 700, "height": 300,
      "data": {{ "values": data.map(function(d) {{ return {{ label: d.label, sum: d.rainfall_sum, max: d.rainfall_max }}; }}) }},
      "encoding": {{ "x": {{ "field": "label", "type": "ordinal", "title": "Month", "axis": {{ "labelAngle": -45 }} }} }},
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
