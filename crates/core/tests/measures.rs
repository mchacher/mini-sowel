//! Reading measures from the standard clusters: bytes in, measures out.

use mini_sowel_core::radio::measures::{Measure, Value, decode};

const TEMPERATURE: u16 = 0x0402;
const HUMIDITY: u16 = 0x0405;
const PRESSURE: u16 = 0x0403;
const ILLUMINANCE: u16 = 0x0400;
const OCCUPANCY: u16 = 0x0406;
const ON_OFF: u16 = 0x0006;
const LEVEL: u16 = 0x0008;
const POWER: u16 = 0x0001;

/// The measure as (name, number, unit); panics when it is not a number or is missing.
fn number(measure: Option<Measure>) -> (&'static str, f64, &'static str) {
    let measure = measure.expect("a measure");
    let Value::Number(value) = measure.value else {
        panic!("not a number: {measure:?}");
    };
    (measure.kind, value, measure.unit)
}

fn flag(measure: Option<Measure>) -> (&'static str, bool) {
    let measure = measure.expect("a measure");
    let Value::Bool(value) = measure.value else {
        panic!("not a boolean: {measure:?}");
    };
    (measure.kind, value)
}

fn int16(cluster: u16, value: i16) -> Option<Measure> {
    decode(cluster, 0x0000, 0x29, &value.to_le_bytes())
}

fn uint16(cluster: u16, attribute: u16, value: u16) -> Option<Measure> {
    decode(cluster, attribute, 0x21, &value.to_le_bytes())
}

#[test]
fn temperature_1952_is_19_52_celsius() {
    assert_eq!(
        number(int16(TEMPERATURE, 1952)),
        ("temperature", 19.52, "°C")
    );
}

#[test]
fn temperature_minus_550_is_minus_5_5_celsius() {
    assert_eq!(
        number(int16(TEMPERATURE, -550)),
        ("temperature", -5.5, "°C")
    );
}

#[test]
fn temperature_0x8000_and_155_degrees_are_no_measure() {
    assert_eq!(int16(TEMPERATURE, i16::MIN), None);
    assert_eq!(int16(TEMPERATURE, 15500), None);
    assert_eq!(int16(TEMPERATURE, -5001), None);
}

#[test]
fn humidity_10100_is_101_percent_and_no_measure() {
    assert_eq!(uint16(HUMIDITY, 0x0000, 10100), None);
    assert_eq!(uint16(HUMIDITY, 0x0000, 0xFFFF), None);
}

#[test]
fn temperature_1950_is_shown_as_19_5_without_the_trailing_zero() {
    let measure = int16(TEMPERATURE, 1950).unwrap();
    assert_eq!(measure.to_string(), "temperature 19.5 °C");
    assert_eq!(
        int16(TEMPERATURE, 1952).unwrap().to_string(),
        "temperature 19.52 °C"
    );
}

#[test]
fn humidity_5410_is_54_1_percent() {
    assert_eq!(
        number(uint16(HUMIDITY, 0x0000, 5410)),
        ("humidity", 54.1, "%")
    );
    assert_eq!(
        uint16(HUMIDITY, 0x0000, 5410).unwrap().to_string(),
        "humidity 54.1 %"
    );
}

#[test]
fn pressure_1013_is_1013_hpa() {
    assert_eq!(number(int16(PRESSURE, 1013)), ("pressure", 1013.0, "hPa"));
}

#[test]
fn pressure_0x8000_is_no_measure() {
    assert_eq!(int16(PRESSURE, i16::MIN), None);
}

#[test]
fn illuminance_0_10001_30001_and_ffff_are_0_10_1000_lux_and_none() {
    let lux = |raw| uint16(ILLUMINANCE, 0x0000, raw);
    assert_eq!(number(lux(0)), ("luminosity", 0.0, "lx"));
    assert_eq!(number(lux(10001)), ("luminosity", 10.0, "lx"));
    assert_eq!(number(lux(30001)), ("luminosity", 1000.0, "lx"));
    assert_eq!(lux(0xFFFF), None);
}

#[test]
fn occupancy_1_is_occupied_and_0_is_not() {
    let occupancy = |bits| decode(OCCUPANCY, 0x0000, 0x18, &[bits]);
    assert_eq!(flag(occupancy(0x01)), ("motion", true));
    assert_eq!(flag(occupancy(0x00)), ("motion", false));
}

#[test]
fn on_off_1_and_0_are_on_and_off_and_level_254_and_127_are_100_and_50_percent() {
    let on_off = |value| decode(ON_OFF, 0x0000, 0x10, &[value]);
    assert_eq!(flag(on_off(1)), ("light_state", true));
    assert_eq!(flag(on_off(0)), ("light_state", false));
    assert_eq!(on_off(1).unwrap().to_string(), "light_state on");
    let level = |value| decode(LEVEL, 0x0000, 0x20, &[value]);
    assert_eq!(number(level(254)), ("light_brightness", 100.0, "%"));
    assert_eq!(number(level(127)), ("light_brightness", 50.0, "%"));
}

#[test]
fn battery_200_is_100_percent_ff_is_none_and_voltage_30_is_3_volts() {
    let battery = |raw| decode(POWER, 0x0021, 0x20, &[raw]);
    assert_eq!(number(battery(200)), ("battery", 100.0, "%"));
    assert_eq!(battery(0xFF), None);
    let voltage = |raw| decode(POWER, 0x0020, 0x20, &[raw]);
    assert_eq!(number(voltage(30)), ("voltage", 3.0, "V"));
    assert_eq!(voltage(0xFF), None);
}

#[test]
fn a_known_attribute_with_an_unexpected_type_or_size_is_none() {
    // Temperature sent as uint16, battery as int16, a humidity cut to one byte.
    assert_eq!(
        decode(TEMPERATURE, 0x0000, 0x21, &1952u16.to_le_bytes()),
        None
    );
    assert_eq!(decode(POWER, 0x0021, 0x29, &[200, 0]), None);
    assert_eq!(decode(HUMIDITY, 0x0000, 0x21, &[0x42]), None);
}

#[test]
fn an_unknown_cluster_or_attribute_is_none() {
    assert_eq!(decode(0x1234, 0x0000, 0x29, &[1, 0]), None);
    assert_eq!(decode(TEMPERATURE, 0x0001, 0x29, &[1, 0]), None);
    assert_eq!(decode(POWER, 0x0000, 0x20, &[1]), None);
}
