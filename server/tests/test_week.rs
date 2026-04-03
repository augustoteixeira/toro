mod common;

use server::{
    Reading, aggregate_month, aggregate_semester, aggregate_week,
    generate_month_json, generate_semester_json, generate_week_json,
    get_readings_for_week, insert_reading,
};

fn reading(hour: &str, temp: f64) -> Reading {
    Reading {
        hour: hour.to_string(),
        temperature: Some(temp),
        humidity: Some(50.0),
        wind_speed: Some(5.0),
        wind_direction: Some(180.0),
        luminosity: Some(500.0),
        rainfall: Some(0.0),
    }
}

#[test]
fn aggregate_week_produces_28_buckets() {
    let buckets = aggregate_week("2025-01-13", &[]);
    assert_eq!(buckets.len(), 28);
}

#[test]
fn aggregate_week_labels_are_correct() {
    let buckets = aggregate_week("2025-01-13", &[]);
    assert_eq!(buckets[0].label, "Mon 0-6");
    assert_eq!(buckets[1].label, "Mon 6-12");
    assert_eq!(buckets[2].label, "Mon 12-18");
    assert_eq!(buckets[3].label, "Mon 18-24");
    assert_eq!(buckets[4].label, "Tue 0-6");
    assert_eq!(buckets[27].label, "Sun 18-24");
}

#[test]
fn aggregate_week_empty_buckets_are_none() {
    let buckets = aggregate_week("2025-01-13", &[]);
    assert!(buckets[0].temperature_mean.is_none());
    assert!(buckets[0].temperature_std.is_none());
    assert!(buckets[0].wind_direction_mean.is_none());
    assert!(buckets[0].rainfall_sum.is_none());
    assert!(buckets[0].rainfall_max.is_none());
}

#[test]
fn aggregate_week_single_reading_has_zero_std() {
    let readings = vec![reading("2025-01-13T02", 25.0)];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(25.0));
    assert_eq!(buckets[0].temperature_std, Some(0.0));
}

#[test]
fn aggregate_week_multiple_readings_in_same_bucket() {
    let readings = vec![
        reading("2025-01-13T00", 20.0),
        reading("2025-01-13T01", 22.0),
        reading("2025-01-13T02", 24.0),
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(22.0));
    let std = buckets[0].temperature_std.unwrap();
    assert!((std - 1.63).abs() < 0.01, "std was {}", std);
}

#[test]
fn aggregate_week_readings_in_different_buckets() {
    let readings = vec![
        reading("2025-01-13T03", 20.0),    // Mon 0-6 (index 0)
        reading("2025-01-13T14", 30.0),    // Mon 12-18 (index 2)
        reading("2025-01-15T10", 25.0),    // Wed 6-12 (index 9)
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(20.0));
    assert_eq!(buckets[2].temperature_mean, Some(30.0));
    assert_eq!(buckets[9].temperature_mean, Some(25.0));
    assert!(buckets[1].temperature_mean.is_none());
}

#[test]
fn aggregate_week_handles_nulls() {
    let readings = vec![
        Reading {
            hour: "2025-01-13T02".to_string(),
            temperature: None,
            humidity: Some(60.0),
            wind_speed: None,
            wind_direction: None,
            luminosity: None,
            rainfall: None,
        },
        reading("2025-01-13T03", 25.0),
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(25.0));
    assert_eq!(buckets[0].humidity_mean, Some(55.0));
}

#[test]
fn aggregate_week_ignores_readings_outside_week() {
    let readings = vec![
        reading("2025-01-12T10", 99.0),    // Sunday before
        reading("2025-01-13T10", 25.0),    // Monday (in range)
        reading("2025-01-20T10", 99.0),    // Next Monday (out of range)
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[1].temperature_mean, Some(25.0));
}

#[test]
fn aggregate_week_wind_direction_vector_average() {
    // Two winds: 10 km/h from 350° and 10 km/h from 10°
    // Naive average = 180°, vector average ≈ 0° (north)
    let readings = vec![
        Reading {
            hour: "2025-01-13T00".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(10.0),
            wind_direction: Some(350.0),
            luminosity: Some(0.0),
            rainfall: Some(0.0),
        },
        Reading {
            hour: "2025-01-13T01".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(10.0),
            wind_direction: Some(10.0),
            luminosity: Some(0.0),
            rainfall: Some(0.0),
        },
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    let dir = buckets[0].wind_direction_mean.unwrap();
    // Should be close to 0° (north), not 180°
    assert!(dir < 5.0 || dir > 355.0, "direction was {} (expected ~0°)", dir);
}

#[test]
fn aggregate_week_wind_direction_requires_both_speed_and_direction() {
    // Speed present but direction null -> no direction computed
    let readings = vec![
        Reading {
            hour: "2025-01-13T00".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(10.0),
            wind_direction: None,
            luminosity: Some(0.0),
            rainfall: Some(0.0),
        },
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert!(buckets[0].wind_direction_mean.is_none());
    assert_eq!(buckets[0].wind_speed_mean, Some(10.0));
}

#[test]
fn aggregate_week_rainfall_sum_and_max() {
    let readings = vec![
        Reading {
            hour: "2025-01-13T00".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(5.0),
            wind_direction: Some(180.0),
            luminosity: Some(0.0),
            rainfall: Some(1.5),
        },
        Reading {
            hour: "2025-01-13T01".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(5.0),
            wind_direction: Some(180.0),
            luminosity: Some(0.0),
            rainfall: Some(3.2),
        },
        Reading {
            hour: "2025-01-13T02".to_string(),
            temperature: Some(20.0),
            humidity: Some(50.0),
            wind_speed: Some(5.0),
            wind_direction: Some(180.0),
            luminosity: Some(0.0),
            rainfall: Some(0.8),
        },
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].rainfall_sum, Some(5.5));
    assert_eq!(buckets[0].rainfall_max, Some(3.2));
}

// --- Database tests ---

#[tokio::test]
async fn get_readings_for_week_returns_only_that_week() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    // 2025-01-13 is a Monday
    let in_week = vec![
        reading("2025-01-13T08", 20.0),  // Monday
        reading("2025-01-15T14", 25.0),  // Wednesday
        reading("2025-01-19T22", 18.0),  // Sunday
    ];
    let out_of_week = vec![
        reading("2025-01-12T23", 99.0),  // Sunday before
        reading("2025-01-20T00", 99.0),  // Next Monday
    ];

    for r in in_week.iter().chain(out_of_week.iter()) {
        insert_reading(&pool, r).await.unwrap();
    }

    let readings = get_readings_for_week(&pool, "2025-01-13").await.unwrap();
    assert_eq!(readings.len(), 3);
    assert_eq!(readings[0].hour, "2025-01-13T08");
    assert_eq!(readings[1].hour, "2025-01-15T14");
    assert_eq!(readings[2].hour, "2025-01-19T22");
}

#[tokio::test]
async fn generate_week_json_writes_valid_file() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    insert_reading(&pool, &reading("2025-01-13T02", 20.0)).await.unwrap();
    insert_reading(&pool, &reading("2025-01-13T14", 30.0)).await.unwrap();

    generate_week_json(&pool, "2025-01-13").await.expect("generate failed");

    let path = "data/static/week/2025-01-13.json";
    let contents = std::fs::read_to_string(path).expect("file not found");
    assert!(contents.contains("Mon 0-6"));
    assert!(contents.contains("Mon 12-18"));
    assert!(contents.contains("20.0"));
    assert!(contents.contains("30.0"));

    std::fs::remove_file(path).ok();
}

// --- Semester tests ---

#[test]
fn aggregate_semester_produces_26_buckets() {
    // 2025-01-06 is a Monday
    let buckets = aggregate_semester("2025-01-06", &[]);
    assert_eq!(buckets.len(), 26);
}

#[test]
fn aggregate_semester_labels_are_week_mondays() {
    let buckets = aggregate_semester("2025-01-06", &[]);
    assert_eq!(buckets[0].label, "2025-01-06");
    assert_eq!(buckets[1].label, "2025-01-13");
    assert_eq!(buckets[25].label, "2025-06-30");
}

#[test]
fn aggregate_semester_buckets_readings_by_week() {
    // 2025-01-06 is Monday of week 0; 2025-01-20 is Monday of week 2
    let readings = vec![
        reading("2025-01-06T10", 20.0),
        reading("2025-01-07T14", 22.0),  // same week (week 0)
        reading("2025-01-20T10", 30.0),  // week 2
    ];
    let buckets = aggregate_semester("2025-01-06", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(21.0));  // (20+22)/2
    assert_eq!(buckets[2].temperature_mean, Some(30.0));
    assert!(buckets[1].temperature_mean.is_none());
}

#[test]
fn aggregate_semester_ignores_readings_outside_range() {
    let readings = vec![
        reading("2025-01-05T10", 99.0),  // Sunday before semester
        reading("2025-01-06T10", 25.0),  // week 0 (in range)
        reading("2025-07-14T10", 99.0),  // after 26 weeks (out of range)
    ];
    let buckets = aggregate_semester("2025-01-06", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(25.0));
    for i in 1..26 {
        assert!(buckets[i].temperature_mean.is_none());
    }
}

#[tokio::test]
async fn generate_semester_json_writes_valid_file() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    insert_reading(&pool, &reading("2025-01-06T10", 25.0)).await.unwrap();
    insert_reading(&pool, &reading("2025-03-10T14", 32.0)).await.unwrap();

    generate_semester_json(&pool, "2025-01-06").await.expect("generate failed");

    let path = "data/static/semester/2025-01-06.json";
    let contents = std::fs::read_to_string(path).expect("file not found");
    assert!(contents.contains("2025-01-06"));
    assert!(contents.contains("2025-03-10"));
    assert!(contents.contains("25.0"));
    assert!(contents.contains("32.0"));

    std::fs::remove_file(path).ok();
}


#[test]
fn aggregate_month_labels_are_full_dates() {
    let buckets = aggregate_month("2025-01", &[]);
    assert_eq!(buckets[0].label, "2025-01-01");
    assert_eq!(buckets[14].label, "2025-01-15");
    assert_eq!(buckets[30].label, "2025-01-31");
}

#[test]
fn aggregate_month_buckets_readings_by_day() {
    let readings = vec![
        reading("2025-01-05T08", 20.0),
        reading("2025-01-05T14", 30.0),  // same day -> bucket 4
        reading("2025-01-20T10", 25.0),  // bucket 19
    ];
    let buckets = aggregate_month("2025-01", &readings);

    assert_eq!(buckets[4].temperature_mean, Some(25.0));  // (20+30)/2
    assert_eq!(buckets[19].temperature_mean, Some(25.0));
    assert!(buckets[0].temperature_mean.is_none());
}

#[tokio::test]
async fn generate_month_json_writes_valid_file() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    insert_reading(&pool, &reading("2025-01-05T10", 28.0)).await.unwrap();
    insert_reading(&pool, &reading("2025-01-20T14", 32.0)).await.unwrap();

    generate_month_json(&pool, "2025-01").await.expect("generate failed");

    let path = "data/static/month/2025-01.json";
    let contents = std::fs::read_to_string(path).expect("file not found");
    assert!(contents.contains("2025-01-05"));
    assert!(contents.contains("2025-01-20"));
    assert!(contents.contains("28.0"));
    assert!(contents.contains("32.0"));

    std::fs::remove_file(path).ok();
}
