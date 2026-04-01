CREATE TABLE hourly_readings (
    hour           TEXT PRIMARY KEY,
    temperature    REAL,
    humidity       REAL,
    wind_speed     REAL,
    wind_direction REAL,
    luminosity     REAL,
    rainfall       REAL
);

INSERT INTO meta (key, value) VALUES ('schema_version', '1');
