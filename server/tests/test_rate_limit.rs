use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use server::RateLimiter;

#[test]
fn allows_requests_under_limit() {
    let limiter = RateLimiter::new();
    let ip: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    for _ in 0..5 {
        assert!(!limiter.too_many_attempts(ip, 5, Duration::from_secs(60)));
    }
}

#[test]
fn blocks_requests_over_limit() {
    let limiter = RateLimiter::new();
    let ip: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

    // First 5 are allowed
    for _ in 0..5 {
        limiter.too_many_attempts(ip, 5, Duration::from_secs(60));
    }
    // 6th exceeds the limit
    assert!(limiter.too_many_attempts(ip, 5, Duration::from_secs(60)));
}

#[test]
fn different_ips_are_tracked_independently() {
    let limiter = RateLimiter::new();
    let ip1: IpAddr = IpAddr::V4(Ipv4Addr::new(1, 2, 3, 4));
    let ip2: IpAddr = IpAddr::V4(Ipv4Addr::new(5, 6, 7, 8));

    // Exhaust ip1's limit
    for _ in 0..5 {
        limiter.too_many_attempts(ip1, 5, Duration::from_secs(60));
    }
    assert!(limiter.too_many_attempts(ip1, 5, Duration::from_secs(60)));

    // ip2 is unaffected
    assert!(!limiter.too_many_attempts(ip2, 5, Duration::from_secs(60)));
}
