use server::{Reading, aggregate_week};

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
}

#[test]
fn aggregate_week_single_reading_has_zero_std() {
    // 2025-01-13 is a Monday, hour 02 -> bucket "Mon 0-6" (index 0)
    let readings = vec![reading("2025-01-13T02", 25.0)];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(25.0));
    assert_eq!(buckets[0].temperature_std, Some(0.0));
}

#[test]
fn aggregate_week_multiple_readings_in_same_bucket() {
    // Hours 00-05 all map to "Mon 0-6" (index 0)
    let readings = vec![
        reading("2025-01-13T00", 20.0),
        reading("2025-01-13T01", 22.0),
        reading("2025-01-13T02", 24.0),
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    // mean = 22.0
    assert_eq!(buckets[0].temperature_mean, Some(22.0));
    // std = sqrt(((20-22)^2 + (22-22)^2 + (24-22)^2) / 3) = sqrt(8/3) ≈ 1.63
    let std = buckets[0].temperature_std.unwrap();
    assert!((std - 1.63).abs() < 0.01, "std was {}", std);
}

#[test]
fn aggregate_week_readings_in_different_buckets() {
    let readings = vec![
        // Mon 0-6 (index 0)
        reading("2025-01-13T03", 20.0),
        // Mon 12-18 (index 2)
        reading("2025-01-13T14", 30.0),
        // Wed 6-12 (index 9) — Wed is day offset 2, quarter 1 -> 2*4+1 = 9
        reading("2025-01-15T10", 25.0),
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    assert_eq!(buckets[0].temperature_mean, Some(20.0));
    assert_eq!(buckets[2].temperature_mean, Some(30.0));
    assert_eq!(buckets[9].temperature_mean, Some(25.0));
    // Other buckets should be None
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

    // Temperature: only one non-null value (25.0)
    assert_eq!(buckets[0].temperature_mean, Some(25.0));
    // Humidity: two values (60.0 and 50.0 from helper)
    assert_eq!(buckets[0].humidity_mean, Some(55.0));
}

#[test]
fn aggregate_week_ignores_readings_outside_week() {
    let readings = vec![
        reading("2025-01-12T10", 99.0), // Sunday before
        reading("2025-01-13T10", 25.0), // Monday (in range)
        reading("2025-01-20T10", 99.0), // Next Monday (out of range)
    ];
    let buckets = aggregate_week("2025-01-13", &readings);

    // Only the Monday reading should be counted (bucket index 1 = Mon 6-12)
    assert_eq!(buckets[1].temperature_mean, Some(25.0));
}
