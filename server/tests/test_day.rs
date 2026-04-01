mod common;

use server::{Reading, get_readings_for_day, insert_reading};

#[tokio::test]
async fn get_readings_for_day_returns_matching_rows() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    let r1 = Reading {
        hour: "2026-03-15T08".to_string(),
        temperature: Some(21.0),
        humidity: Some(72.0),
        wind_speed: None,
        wind_direction: None,
        luminosity: None,
        rainfall: None,
    };
    let r2 = Reading {
        hour: "2026-03-15T09".to_string(),
        temperature: Some(23.2),
        humidity: Some(65.0),
        wind_speed: None,
        wind_direction: None,
        luminosity: None,
        rainfall: None,
    };
    let r3 = Reading {
        hour: "2026-03-16T08".to_string(),
        temperature: Some(19.8),
        humidity: Some(79.0),
        wind_speed: None,
        wind_direction: None,
        luminosity: None,
        rainfall: None,
    };

    insert_reading(&pool, &r1).await.unwrap();
    insert_reading(&pool, &r2).await.unwrap();
    insert_reading(&pool, &r3).await.unwrap();

    let day15 = get_readings_for_day(&pool, "2026-03-15").await.unwrap();
    assert_eq!(day15.len(), 2);
    assert_eq!(day15[0].hour, "2026-03-15T08");
    assert_eq!(day15[1].hour, "2026-03-15T09");

    let day16 = get_readings_for_day(&pool, "2026-03-16").await.unwrap();
    assert_eq!(day16.len(), 1);
    assert_eq!(day16[0].hour, "2026-03-16T08");
}

#[tokio::test]
async fn get_readings_for_day_returns_empty_for_no_data() {
    let pool = common::test_pool().await;
    server::migrate(&pool).await.expect("migration failed");

    let readings = get_readings_for_day(&pool, "2026-01-01").await.unwrap();
    assert!(readings.is_empty());
}
