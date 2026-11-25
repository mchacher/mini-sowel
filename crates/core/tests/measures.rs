//! Reading measures from the standard clusters: bytes in, measures out.

use solite_core::radio::measures::{Measure, REPORTING, Value, cluster_name, decode};

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

const COLOR: u16 = 0x0300;

#[test]
fn color_temperature_370_is_370_mired_and_0_1001_or_a_wrong_type_give_none() {
    assert_eq!(
        number(uint16(COLOR, 0x0007, 370)),
        ("light_color_temp", 370.0, "mired")
    );
    assert_eq!(uint16(COLOR, 0x0007, 0), None);
    assert_eq!(uint16(COLOR, 0x0007, 1001), None);
    assert_eq!(number(uint16(COLOR, 0x0007, 1000)).1, 1000.0);
    // uint8 instead of uint16.
    assert_eq!(decode(COLOR, 0x0007, 0x20, &[100]), None);
}

#[test]
fn color_x_and_y_are_the_raw_value_over_65535_on_four_decimals() {
    assert_eq!(
        number(uint16(COLOR, 0x0003, 0x5000)),
        ("color_x", 0.3125, "")
    );
    assert_eq!(
        number(uint16(COLOR, 0x0004, 0x5500)),
        ("color_y", 0.332, "")
    );
    assert_eq!(number(uint16(COLOR, 0x0003, 0xFEFF)).1, 0.9961);
    assert_eq!(uint16(COLOR, 0x0003, 0xFF00), None);
    assert_eq!(uint16(COLOR, 0x0004, 0xFFFF), None);
    assert_eq!(decode(COLOR, 0x0003, 0x20, &[0x50]), None);
}

#[test]
fn hue_127_and_saturation_254_are_180_degrees_and_100_percent_and_255_is_none() {
    let hue = |raw| decode(COLOR, 0x0000, 0x20, &[raw]);
    let saturation = |raw| decode(COLOR, 0x0001, 0x20, &[raw]);
    assert_eq!(number(hue(127)), ("color_hue", 180.0, "°"));
    assert_eq!(number(saturation(254)), ("color_saturation", 100.0, "%"));
    assert_eq!(hue(255), None);
    assert_eq!(saturation(255), None);
    assert_eq!(decode(COLOR, 0x0000, 0x21, &[127, 0]), None);
}

#[test]
fn the_colour_measures_are_shown_with_their_unit() {
    assert_eq!(
        uint16(COLOR, 0x0007, 370).unwrap().to_string(),
        "light_color_temp 370 mired"
    );
    assert_eq!(
        uint16(COLOR, 0x0003, 0x5000).unwrap().to_string(),
        "color_x 0.3125"
    );
    assert_eq!(
        decode(COLOR, 0x0000, 0x20, &[127]).unwrap().to_string(),
        "color_hue 180 °"
    );
    assert_eq!(
        decode(COLOR, 0x0001, 0x20, &[254]).unwrap().to_string(),
        "color_saturation 100 %"
    );
}

#[test]
fn the_reporting_setup_has_nine_entries_with_colour_after_level_and_before_power() {
    assert_eq!(REPORTING.len(), 9);
    let clusters: Vec<u16> = REPORTING.iter().map(|r| r.cluster).collect();
    let position = |cluster| clusters.iter().position(|c| *c == cluster).unwrap();
    assert_eq!(position(COLOR), position(LEVEL) + 1);
    assert_eq!(position(POWER), position(COLOR) + 1);
    let attributes: Vec<u16> = REPORTING[position(COLOR)]
        .attributes
        .iter()
        .map(|a| a.attribute)
        .collect();
    assert_eq!(attributes, vec![0x0007, 0x0003, 0x0004]);
    assert_eq!(cluster_name(COLOR), "light_color");
}
