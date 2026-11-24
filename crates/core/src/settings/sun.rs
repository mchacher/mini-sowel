//! Sunrise, sunset and daylight (spec 026 R2), as Sowel's `SunlightManager`
//! (`src/zones/sunlight-manager.ts`) computes them with `suncalc` 2: a port of suncalc's `getTimes`
//! (sunrise and sunset, the sun at −0.833°) and `getPosition` (the altitude), in `f64`. No clock of
//! its own: the caller gives `now`.

use std::f64::consts::PI;

use serde::Serialize;

use super::Settings;
use super::tz::TimeZone;
use crate::home::CLOCK_SET_AFTER_MS;

const RAD: f64 = PI / 180.0;
const DAY_MS: f64 = 86_400_000.0;
const J1970: f64 = 2_440_588.0;
const J2000: f64 = 2_451_545.0;
const J0: f64 = 0.0009;
/// The sun's altitude at sunrise and sunset, in degrees (suncalc's `sunrise`/`sunset` angle).
const SUNRISE_ANGLE: f64 = -0.833;

/// What the root zone shows (Sowel's `SunlightData`): local `HH:MM` (`"--:--"` where the sun
/// neither rises nor sets that day) and the daylight; all `None` without a location or before the
/// clock is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sunlight {
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub is_daylight: Option<bool>,
}

fn to_days(ms: f64) -> f64 {
    ms / DAY_MS - 0.5 + J1970 - J2000
}

fn from_julian(j: f64) -> f64 {
    (j + 0.5 - J1970) * DAY_MS
}

/// The difference between terrestrial and universal time, in seconds (suncalc's `deltaT`).
fn delta_t(d: f64) -> f64 {
    let y = 2000.0 + d / 365.2425;
    if y < 1920.0 {
        let t = y - 1900.0;
        return -2.79 + t * (1.494119 + t * (-0.0598939 + t * (0.0061966 - t * 197e-6)));
    }
    if y < 1941.0 {
        let t = y - 1920.0;
        return 21.2 + t * (0.84493 + t * (-0.0761 + t * 0.0020936));
    }
    if y < 1961.0 {
        let t = y - 1950.0;
        return 29.07 + t * (0.407 + t * (-1.0 / 233.0 + t / 2547.0));
    }
    if y < 1986.0 {
        let t = y - 1975.0;
        return 45.45 + t * (1.067 + t * (-1.0 / 260.0 - t / 718.0));
    }
    if y < 2005.0 {
        let t = y - 2000.0;
        return 63.86
            + t * (0.3345 + t * (-0.060374 + t * (0.0017275 + t * (651814e-9 + t * 2373599e-11))));
    }
    if y < 2050.0 {
        let t = y - 2000.0;
        return 62.92 + t * (0.32217 + t * 0.005589);
    }
    let t = (y - 1820.0) / 100.0;
    -20.0 + 32.0 * t * t - 0.5628 * (2150.0 - y)
}

fn to_days_tt(d: f64) -> f64 {
    d + delta_t(d) / 86_400.0
}

fn altitude(h: f64, phi: f64, dec: f64) -> f64 {
    (phi.sin() * dec.sin() + phi.cos() * dec.cos() * h.cos()).asin()
}

fn sidereal_time(d: f64, lw: f64) -> f64 {
    RAD * (280.46061837 + 360.98564736629 * d) - lw
}

fn astro_refraction(h: f64) -> f64 {
    let h = if h < 0.0 { 0.0 } else { h };
    2967e-7 / (h + 0.00312536 / (h + 0.08901179)).tan()
}

/// The sun's right ascension and declination.
fn sun_coords(d: f64) -> (f64, f64) {
    let t = d / 36525.0;
    let l0 = RAD * (280.46646 + t * (36000.76983 + t * 3032e-7));
    let m = RAD * (357.52911 + t * (35999.05029 - t * 1537e-7));
    let (sin_m, cos_m) = (m.sin(), m.cos());
    let c = RAD
        * ((1.914602 - t * (0.004817 + t * 14e-6)) * sin_m
            + (0.019993 - 101e-6 * t) * 2.0 * sin_m * cos_m
            + 289e-6 * sin_m * (3.0 - 4.0 * sin_m * sin_m));
    let om = RAD * (125.04 - 1934.136 * t);
    let l = l0 + c - RAD * (0.00569 + 0.00478 * om.sin());
    let e =
        RAD * (23.439291 - t * (0.0130042 + t * (16e-8 - t * 504e-9))) + RAD * 0.00256 * om.cos();
    let ra = (e.cos() * l.sin()).atan2(l.cos());
    let dec = (e.sin() * l.sin()).asin();
    (ra, dec)
}

/// The sun's altitude above the horizon, in degrees, refraction included (suncalc's
/// `getPosition(...).altitude`).
pub fn altitude_at(ms: u64, latitude: f64, longitude: f64) -> f64 {
    let lw = RAD * -longitude;
    let phi = RAD * latitude;
    let d = to_days(ms as f64);
    let (ra, dec) = sun_coords(to_days_tt(d));
    let h = sidereal_time(d, lw) - ra;
    let a = altitude(h, phi, dec);
    (a + astro_refraction(a)) / RAD
}

/// JavaScript's `Math.round`: halves go up.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

fn wrap_pi(a: f64) -> f64 {
    a - 2.0 * PI * js_round(a / (2.0 * PI))
}

fn solar_transit(mut dt: f64, lw: f64) -> f64 {
    for _ in 0..3 {
        let h = wrap_pi(sidereal_time(dt, lw) - sun_coords(to_days_tt(dt)).0);
        dt -= h / (2.0 * PI);
    }
    dt
}

/// The day (in days since J2000) the sun crosses `h0` (radians), `None` where it never does.
fn get_set_j(h0: f64, dt: f64, sign: f64, lw: f64, phi: f64, dec_t: f64) -> Option<f64> {
    let cos_h0 = (h0.sin() - phi.sin() * dec_t.sin()) / (phi.cos() * dec_t.cos());
    if !(-1.0..=1.0).contains(&cos_h0) {
        return None;
    }
    let mut d = dt + sign * cos_h0.acos() / (2.0 * PI);
    for _ in 0..2 {
        let (ra, dec) = sun_coords(to_days_tt(d));
        let h = wrap_pi(sidereal_time(d, lw) - ra);
        let alt = altitude(h, phi, dec);
        let sin_h = phi.cos() * dec.cos() * h.sin();
        if sin_h.abs() < 1e-6 {
            break;
        }
        d += (alt - h0) / (2.0 * PI * sin_h);
    }
    if d.is_finite() { Some(d) } else { None }
}

/// Sunrise and sunset of the solar day nearest to `ms`, in milliseconds since 1970 (UTC), as
/// suncalc's `getTimes(new Date(ms), lat, lon)` gives them (a `Date` keeps whole milliseconds);
/// `None` where the sun neither rises nor sets that day.
pub fn sun_times(ms: u64, latitude: f64, longitude: f64) -> (Option<i64>, Option<i64>) {
    let lw = RAD * -longitude;
    let phi = RAD * latitude;
    let lon = J0 + lw / (2.0 * PI);
    let dt = solar_transit(js_round(to_days(ms as f64) - lon) + lon, lw);
    let dec = sun_coords(to_days_tt(dt)).1;
    let h0 = SUNRISE_ANGLE * RAD;
    let time = |j: Option<f64>| j.map(|j| from_julian(j + J2000).trunc() as i64);
    (
        time(get_set_j(h0, dt, -1.0, lw, phi, dec)),
        time(get_set_j(h0, dt, 1.0, lw, phi, dec)),
    )
}

/// `HH:MM` of an instant in a time zone, seconds cut (JavaScript's `getHours`, `getMinutes`).
pub fn local_hh_mm(ms: i64, zone: &TimeZone) -> String {
    let seconds = (ms.div_euclid(1000) + zone.offset_at(ms)).rem_euclid(86_400);
    format!("{:02}:{:02}", seconds / 3600, seconds % 3600 / 60)
}

/// The home's sun data at `now` (Sowel's `compute` and `daylightAt`): `None` everywhere without a
/// latitude and a longitude, or before the clock is set; else the local times and whether `now`
/// is from sunrise + the sunrise offset until sunset − the sunset offset (end excluded); where the
/// sun neither rises nor sets, `"--:--"` and the altitude above 0.
pub fn sunlight(settings: &Settings, now: u64) -> Sunlight {
    let Some((latitude, longitude)) = settings.location() else {
        return Sunlight::default();
    };
    if now < CLOCK_SET_AFTER_MS {
        return Sunlight::default();
    }
    let zone = settings.time_zone();
    let at = now as i64;
    match sun_times(now, latitude, longitude) {
        (Some(rise), Some(set)) => {
            let from = rise + settings.sunrise_offset() * 60_000;
            let until = set - settings.sunset_offset() * 60_000;
            Sunlight {
                sunrise: Some(local_hh_mm(rise, &zone)),
                sunset: Some(local_hh_mm(set, &zone)),
                is_daylight: Some(at >= from && at < until),
            }
        }
        _ => Sunlight {
            sunrise: Some("--:--".into()),
            sunset: Some("--:--".into()),
            is_daylight: Some(altitude_at(now, latitude, longitude) > 0.0),
        },
    }
}
