"""
Generate ~3 years of fake hourly weather readings for Goias, Brazil.
Outputs SQL INSERT statements to stdout.

Usage: python3 generate_fake.py | sqlite3 db.sqlite
"""

import math
import random
from datetime import datetime, timedelta

# Fixed seed for reproducibility
random.seed(42)

# Date range: 2023-06-01 to 2026-05-31 (3 years)
start = datetime(2023, 6, 1)
end = datetime(2026, 6, 1)

# --- Climate parameters for Goias ---

def day_of_year_frac(dt):
    """Day of year as fraction [0, 1)."""
    return dt.timetuple().tm_yday / 365.25

def hour_frac(dt):
    """Hour as fraction [0, 1)."""
    return dt.hour / 24.0

def seasonal(dt):
    """Seasonal factor: 1.0 = peak wet season (Jan), -1.0 = peak dry season (Jul)."""
    # Shifted so Jan=peak wet, Jul=peak dry
    return math.cos(2 * math.pi * (day_of_year_frac(dt) - 0.04))

def diurnal(dt):
    """Diurnal factor: -1.0 = coolest (5am), 1.0 = hottest (2pm)."""
    return math.sin(2 * math.pi * (hour_frac(dt) - 0.23))

def is_wet_season(dt):
    """Oct-Apr is wet season."""
    return dt.month >= 10 or dt.month <= 4

def temperature(dt):
    """Temperature in C. Goias: 18-35C range."""
    base = 26.0
    seasonal_amp = 3.0  # warmer in wet season
    diurnal_amp = 6.0   # hot afternoons, cool nights
    t = base + seasonal_amp * seasonal(dt) + diurnal_amp * diurnal(dt)
    t += random.gauss(0, 1.2)
    return round(t, 1)

def humidity(dt):
    """Humidity %. Wet season: 60-95%, Dry season: 20-55%."""
    if is_wet_season(dt):
        base = 78.0
        amp = 12.0
    else:
        base = 38.0
        amp = 10.0
    # Higher at night, lower in afternoon
    h = base - amp * diurnal(dt)
    h += random.gauss(0, 5)
    return round(max(10, min(100, h)), 1)

def wind_speed(dt):
    """Wind speed km/h. Generally light in Goias, gustier in dry season."""
    base = 6.0 if is_wet_season(dt) else 9.0
    # Slightly windier in afternoon
    w = base + 3.0 * max(0, diurnal(dt))
    w += random.gauss(0, 2)
    return round(max(0, w), 1)

def wind_direction(dt):
    """Wind direction degrees. Prevailing NW in wet, SE in dry."""
    if is_wet_season(dt):
        base = 315  # NW
    else:
        base = 135  # SE
    d = base + random.gauss(0, 30)
    return round(d % 360, 1)

def luminosity(dt):
    """Luminosity (lux-like). 0 at night, peaks midday. Clouds in wet season."""
    if dt.hour < 5 or dt.hour > 19:
        return 0.0
    # Bell curve centered on noon
    solar = math.exp(-0.5 * ((dt.hour - 12) / 3.0) ** 2)
    peak = 950 if not is_wet_season(dt) else 700
    lux = peak * solar
    # Cloud cover randomness (more in wet season)
    cloud_factor = random.uniform(0.5, 1.0) if is_wet_season(dt) else random.uniform(0.8, 1.0)
    lux *= cloud_factor
    lux += random.gauss(0, 20)
    return round(max(0, lux), 1)

def rainfall(dt):
    """Rainfall mm. Mostly afternoon storms in wet season, near zero in dry."""
    if not is_wet_season(dt):
        # Rare dry season rain
        if random.random() < 0.005:
            return round(random.uniform(0.1, 2.0), 1)
        return 0.0
    # Wet season: afternoon storms (12-20h), ~30% chance per hour
    if 12 <= dt.hour <= 20 and random.random() < 0.25:
        return round(random.expovariate(0.3), 1)
    # Light drizzle chance
    if random.random() < 0.05:
        return round(random.uniform(0.1, 1.0), 1)
    return 0.0


# --- Gap and error simulation ---

def should_skip(dt):
    """Simulate sensor downtime: occasional gaps of 1-72 hours."""
    # Use a deterministic approach: hash the day to decide if a gap starts
    day_hash = hash((dt.year, dt.month, dt.day))
    # ~3% of days have some downtime
    if day_hash % 100 < 3:
        # Gap centered around a random hour
        gap_start = (day_hash % 24)
        gap_len = (abs(day_hash) % 48) + 1
        offset = dt.hour - gap_start
        if 0 <= offset < gap_len:
            return True
    return False

def maybe_null(value):
    """~1% chance of sensor error -> null."""
    if random.random() < 0.01:
        return None
    return value


# --- Generate SQL ---

print("BEGIN TRANSACTION;")

current = start
count = 0
while current < end:
    if not should_skip(current):
        hour_str = current.strftime("%Y-%m-%dT%H")
        t = maybe_null(temperature(current))
        h = maybe_null(humidity(current))
        ws = maybe_null(wind_speed(current))
        wd = maybe_null(wind_direction(current))
        lux = maybe_null(luminosity(current))
        r = maybe_null(rainfall(current))

        def sql_val(v):
            return "NULL" if v is None else str(v)

        print(f"INSERT INTO hourly_readings (hour, temperature, humidity, wind_speed, wind_direction, luminosity, rainfall) "
              f"VALUES ('{hour_str}', {sql_val(t)}, {sql_val(h)}, {sql_val(ws)}, {sql_val(wd)}, {sql_val(lux)}, {sql_val(r)});")
        count += 1

    current += timedelta(hours=1)

print("COMMIT;")

import sys
print(f"-- Generated {count} readings from {start.date()} to {(end - timedelta(days=1)).date()}", file=sys.stderr)
