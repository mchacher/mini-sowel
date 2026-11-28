//! Reading measures from the standard clusters: bytes in, measures out.

use solite_core::radio::measures::{
    Measure, REPORTING, Ratio, Scale, Value, cluster_name, decode, from_records_scaled,
};
use solite_core::radio::zcl::AttributeRecord;

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
fn the_reporting_setup_has_thirteen_entries_with_colour_after_level_and_before_power() {
    assert_eq!(REPORTING.len(), 13);
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

// Spec 048: power, energy and current, with the scale of the device.

const ELECTRICAL: u16 = 0x0B04;
const METERING: u16 = 0x0702;

fn record(attribute: u16, data_type: u8, value: &[u8]) -> AttributeRecord {
    AttributeRecord {
        attribute,
        data_type,
        value: value.to_vec(),
    }
}

fn ratio(multiplier: u32, divisor: u32) -> Ratio {
    Ratio {
        multiplier,
        divisor,
    }
}

fn electrical(power: Ratio, current: Ratio) -> Scale {
    Scale {
        electrical: Some((power, current)),
        metering: None,
    }
}

fn metering(ratio: Ratio) -> Scale {
    Scale {
        electrical: None,
        metering: Some(ratio),
    }
}

/// The only measure of the records, as (name, number, unit).
fn scaled(
    cluster: u16,
    records: &[AttributeRecord],
    scale: &Scale,
) -> (&'static str, f64, &'static str) {
    let (measures, missing) = from_records_scaled(cluster, records, scale);
    assert!(!missing);
    assert_eq!(measures.len(), 1, "{measures:?}");
    number(Some(measures[0]))
}

#[test]
fn active_power_1234_with_divisor_10_is_123_w() {
    let records = [record(0x050B, 0x29, &1234i16.to_le_bytes())];
    let scale = electrical(ratio(1, 10), ratio(1, 1));
    assert_eq!(scaled(ELECTRICAL, &records, &scale), ("power", 123.0, "W"));
}

#[test]
fn a_negative_active_power_is_kept() {
    let records = [record(0x050B, 0x29, &(-500i16).to_le_bytes())];
    let scale = electrical(ratio(1, 10), ratio(1, 1));
    assert_eq!(scaled(ELECTRICAL, &records, &scale), ("power", -50.0, "W"));
}

#[test]
fn rms_current_523_with_divisor_1000_is_0_52_a() {
    let records = [record(0x0508, 0x21, &523u16.to_le_bytes())];
    let scale = electrical(ratio(1, 1), ratio(1, 1000));
    assert_eq!(scaled(ELECTRICAL, &records, &scale), ("current", 0.52, "A"));
}

#[test]
fn summation_123456_with_divisor_1000_is_123_46_kwh() {
    let records = [record(0x0000, 0x25, &123_456u64.to_le_bytes()[..6])];
    let scale = metering(ratio(1, 1000));
    assert_eq!(
        scaled(METERING, &records, &scale),
        ("energy", 123.46, "kWh")
    );
}

#[test]
fn instantaneous_demand_of_a_metering_only_plug_is_power_in_w() {
    // 1500 with divisor 1000: 1.5 kW.
    let records = [record(0x0400, 0x2A, &[0xDC, 0x05, 0x00])];
    let scale = metering(ratio(1, 1000));
    assert_eq!(scaled(METERING, &records, &scale), ("power", 1500.0, "W"));
}

#[test]
fn the_demand_is_left_out_when_the_device_has_electrical_measurement() {
    let records = [record(0x0400, 0x2A, &[0xDC, 0x05, 0x00])];
    let scale = Scale {
        electrical: Some((ratio(1, 1), ratio(1, 1))),
        metering: Some(ratio(1, 1000)),
    };
    let (measures, missing) = from_records_scaled(METERING, &records, &scale);
    assert!(measures.is_empty() && !missing);
}

#[test]
fn a_report_before_the_scale_is_known_publishes_nothing_and_asks_for_it() {
    let records = [
        record(0x050B, 0x29, &1234i16.to_le_bytes()),
        record(0x0508, 0x21, &523u16.to_le_bytes()),
    ];
    let (measures, missing) = from_records_scaled(ELECTRICAL, &records, &Scale::default());
    assert!(measures.is_empty());
    assert!(missing);
    let energy = [record(0x0000, 0x25, &1u64.to_le_bytes()[..6])];
    let (measures, missing) = from_records_scaled(METERING, &energy, &Scale::default());
    assert!(measures.is_empty() && missing);
}

#[test]
fn a_divisor_of_zero_or_absent_counts_as_one() {
    // Read answer with a divisor of 0 and a multiplier absent for the power, nothing for the
    // current.
    let mut scale = Scale::default();
    scale.absorb(
        ELECTRICAL,
        &[record(0x0605, 0x21, &0u16.to_le_bytes())],
        true,
    );
    assert_eq!(scale.electrical, Some((ratio(1, 1), ratio(1, 1))));
    let records = [record(0x050B, 0x29, &1234i16.to_le_bytes())];
    assert_eq!(scaled(ELECTRICAL, &records, &scale), ("power", 1234.0, "W"));
    // The answer is an answer even with no attribute at all (all unsupported).
    let mut metering_scale = Scale::default();
    metering_scale.absorb(METERING, &[], true);
    assert_eq!(metering_scale.metering, Some(ratio(1, 1)));
}

#[test]
fn the_scale_is_read_from_an_answer_and_a_report_only_updates_a_known_one() {
    let mut scale = Scale::default();
    // A report of the scale attributes before any answer is not taken.
    scale.absorb(
        ELECTRICAL,
        &[record(0x0605, 0x21, &10u16.to_le_bytes())],
        false,
    );
    assert_eq!(scale.electrical, None);
    scale.absorb(
        METERING,
        &[
            record(0x0301, 0x22, &[2, 0, 0]),
            record(0x0302, 0x22, &[0xE8, 0x03, 0]),
        ],
        true,
    );
    assert_eq!(scale.metering, Some(ratio(2, 1000)));
    scale.absorb(METERING, &[record(0x0302, 0x22, &[100, 0, 0])], false);
    assert_eq!(scale.metering, Some(ratio(2, 100)));
}

#[test]
fn the_electrical_clusters_set_up_their_reporting_as_the_spec_says() {
    let find = |cluster| REPORTING.iter().find(|r| r.cluster == cluster).unwrap();
    let electrical = find(ELECTRICAL);
    let attributes: Vec<_> = electrical
        .attributes
        .iter()
        .map(|a| (a.attribute, a.min_interval, a.max_interval, a.change))
        .collect();
    assert_eq!(
        attributes,
        vec![(0x050B, 10, 300, Some(1)), (0x0508, 10, 300, Some(1))]
    );
    let summation = &find(METERING).attributes[0];
    assert_eq!(
        (
            summation.attribute,
            summation.min_interval,
            summation.max_interval
        ),
        (0x0000, 60, 600)
    );
}

#[test]
fn a_metering_plug_is_asked_to_report_its_demand_as_int24() {
    // Without it, a plug with Metering only would never send its power (spec 048 R1, R3).
    let metering = solite_core::radio::measures::reporting_cluster(0x0702).expect("metering");
    let demand = metering
        .attributes
        .iter()
        .find(|r| r.attribute == 0x0400)
        .expect("InstantaneousDemand is configured");
    assert_eq!(demand.data_type, 0x2A, "int24 is 0x2A in the ZCL");
    assert!(metering.attributes.iter().any(|r| r.attribute == 0x0000));
}

#[test]
fn a_lift_percentage_of_30_is_a_position_of_70_and_above_100_is_no_measure() {
    let lift = |value| decode(0x0102, 0x0008, 0x20, &[value]);
    assert_eq!(number(lift(30)), ("shutter_position", 70.0, "%"));
    assert_eq!(number(lift(0)), ("shutter_position", 100.0, "%"));
    assert_eq!(number(lift(100)), ("shutter_position", 0.0, "%"));
    assert!(lift(101).is_none());
    assert!(lift(0xFF).is_none());
}

#[test]
fn a_thermostat_gives_its_temperature_and_setpoint_and_drops_the_invalid() {
    let local = |raw: i16| decode(0x0201, 0x0000, 0x29, &raw.to_le_bytes());
    let setpoint = |raw: i16| decode(0x0201, 0x0012, 0x29, &raw.to_le_bytes());
    assert_eq!(number(local(2150)), ("local_temperature", 21.5, "°C"));
    assert_eq!(number(local(-250)), ("local_temperature", -2.5, "°C"));
    assert!(local(i16::MIN).is_none(), "0x8000 is invalid");
    assert_eq!(number(setpoint(1900)), ("setpoint", 19.0, "°C"));
    assert!(setpoint(i16::MIN).is_none());
    assert!(
        decode(0x0201, 0x0012, 0x20, &[19]).is_none(),
        "not an int16"
    );
}
