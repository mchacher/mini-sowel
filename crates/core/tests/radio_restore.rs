//! The network start of a restored box (spec 035, R7): a radio with no network forms the one of
//! the backup file with its keys; the one-shot mark makes a radio on another network leave it
//! first. Against the scripted radio, EZSP frames checked.

use solite_core::radio::backup::{RestoredNetwork, parse, text};
use solite_core::radio::{Network, NetworkKeys, NetworkSettings, RadioError};
use solite_sim::{Harness, Ncp, StoredNetwork, harness};

const LEAVE: u16 = 0x20;
const FORM: u16 = 0x1E;
const SECURITY: u16 = 0x68;

/// A random source that must not be called.
fn no_random() -> impl FnMut(&mut [u8]) -> Result<(), String> {
    |_: &mut [u8]| Err("the restored network needs no random bytes".to_string())
}

fn restored() -> RestoredNetwork {
    let formed = Network {
        pan_id: 0x1A2B,
        extended_pan_id: [1, 2, 3, 4, 5, 6, 7, 8],
        channel: 22,
        formed: true,
        from_backup: false,
        keys: None,
    };
    let keys = NetworkKeys {
        network_key: [0x11; 16],
        trust_center_key: [0x22; 16],
    };
    parse(&text(&formed, &keys)).unwrap()
}

fn on_network(pan: u16, ext_wire: [u8; 8]) -> Ncp {
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: ext_wire,
        pan_id: pan,
        channel: 15,
        node_type: 1,
    });
    ncp
}

fn start(
    h: &mut Harness,
    restored: Option<&RestoredNetwork>,
    replace: bool,
) -> Result<Network, RadioError> {
    h.radio.identify()?;
    h.radio.start_network(
        &NetworkSettings::default(),
        restored,
        replace,
        &mut no_random(),
        &mut |_| Ok(()),
    )
}

fn commands(h: &Harness) -> Vec<(u16, Vec<u8>)> {
    h.ncp.borrow().commands.clone()
}

fn ids(h: &Harness) -> Vec<u16> {
    commands(h).iter().map(|c| c.0).collect()
}

fn params_of(h: &Harness, id: u16) -> Vec<u8> {
    commands(h)
        .into_iter()
        .find(|c| c.0 == id)
        .unwrap_or_else(|| panic!("command 0x{id:X} not sent"))
        .1
}

#[test]
fn a_radio_with_no_network_forms_the_backups_with_its_ids_channel_and_keys() {
    let mut h = harness(Ncp::recording());
    let formed_with = std::cell::RefCell::new(None);
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            Some(&restored()),
            false,
            &mut no_random(),
            &mut |n| {
                *formed_with.borrow_mut() = Some(n.clone());
                Ok(())
            },
        )
        .unwrap();
    assert!(network.formed);
    assert!(network.from_backup);
    assert!(
        solite_core::radio::network_line(&network, false)
            .starts_with("network: formed (restored from the backup), channel 22")
    );
    assert_eq!(network.pan_id, 0x1A2B);
    // As the file shows it (display order).
    assert_eq!(network.extended_pan_id, [1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(
        network.channel, 22,
        "the backup's channel, not the default 15"
    );
    let keys = network.keys.as_ref().unwrap();
    assert_eq!(keys.network_key, [0x11; 16]);
    assert_eq!(keys.trust_center_key, [0x22; 16]);
    assert_eq!(
        formed_with.into_inner().as_ref(),
        Some(&network),
        "on_formed called"
    );
    // The frames: the keys in the security state (trust centre key first, then network key)...
    let security = params_of(&h, SECURITY);
    assert_eq!(&security[2..18], &[0x22; 16]);
    assert_eq!(&security[18..34], &[0x11; 16]);
    // ...and the extended PAN id reversed (frame order), the PAN id and the channel.
    let form = params_of(&h, FORM);
    assert_eq!(
        &form[..8],
        &[8, 7, 6, 5, 4, 3, 2, 1],
        "extended PAN id in frame order"
    );
    assert_eq!(&form[8..10], &0x1A2Bu16.to_le_bytes());
    assert_eq!(form[11], 22);
    assert!(!ids(&h).contains(&LEAVE));
}

#[test]
fn a_radio_with_no_network_and_no_restored_network_forms_a_random_one_as_before() {
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let mut n = 0u8;
    let mut random = |b: &mut [u8]| {
        n += 1;
        b.fill(n);
        Ok(())
    };
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            false,
            &mut random,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert!(network.formed);
    assert_eq!(network.channel, 15);
    assert_eq!(network.keys.unwrap().network_key, [1; 16]);
    // The mark without a parsable file changes nothing either.
    let mut h = harness(Ncp::recording());
    h.radio.identify().unwrap();
    let network = h
        .radio
        .start_network(
            &NetworkSettings::default(),
            None,
            true,
            &mut random,
            &mut |_| Ok(()),
        )
        .unwrap();
    assert!(network.formed && network.channel == 15);
}

#[test]
fn the_mark_and_a_radio_on_another_network_leave_it_then_form_the_restored_one() {
    let mut h = harness(on_network(0x4321, [9, 9, 9, 9, 9, 9, 9, 0xAA]));
    let network = start(&mut h, Some(&restored()), true).unwrap();
    assert!(network.formed, "formed, not resumed");
    assert_eq!(network.pan_id, 0x1A2B);
    assert_eq!(network.channel, 22);
    let ids = ids(&h);
    let leave = ids
        .iter()
        .position(|i| *i == LEAVE)
        .expect("leaveNetwork sent");
    let form = ids
        .iter()
        .position(|i| *i == FORM)
        .expect("formNetwork sent");
    assert!(leave < form, "left first: {ids:?}");
    let form = params_of(&h, FORM);
    assert_eq!(&form[..8], &[8, 7, 6, 5, 4, 3, 2, 1]);
    // The radio now holds the restored network.
    let stored = h.ncp.borrow().network.clone().unwrap();
    assert_eq!(stored.pan_id, 0x1A2B);
    assert_eq!(stored.extended_pan_wire, [8, 7, 6, 5, 4, 3, 2, 1]);
}

#[test]
fn the_leave_command_has_an_options_byte_from_ezsp_14_and_none_before() {
    for (protocol, length) in [(13u8, 0usize), (14, 1), (19, 1)] {
        let mut ncp = on_network(0x4321, [9; 8]);
        ncp.protocol = protocol;
        let mut h = harness(ncp);
        start(&mut h, Some(&restored()), true).unwrap();
        assert_eq!(params_of(&h, LEAVE).len(), length, "EZSP {protocol}");
    }
}

#[test]
fn the_mark_and_a_radio_already_on_the_restored_network_resume_it_without_a_leave() {
    // The restored network's extended PAN id in frame order.
    let mut h = harness(on_network(0x1A2B, [8, 7, 6, 5, 4, 3, 2, 1]));
    let network = start(&mut h, Some(&restored()), true).unwrap();
    assert!(!network.formed, "resumed");
    assert_eq!(network.pan_id, 0x1A2B);
    let ids = ids(&h);
    assert!(!ids.contains(&LEAVE) && !ids.contains(&FORM), "{ids:?}");
}

#[test]
fn a_radio_on_another_network_without_the_mark_resumes_it_and_never_leaves_it() {
    let mut h = harness(on_network(0x4321, [9, 9, 9, 9, 9, 9, 9, 0xAA]));
    let network = start(&mut h, Some(&restored()), false).unwrap();
    assert!(!network.formed);
    assert_eq!(network.pan_id, 0x4321);
    let ids = ids(&h);
    assert!(!ids.contains(&LEAVE) && !ids.contains(&FORM), "{ids:?}");
}

#[test]
fn a_start_that_fails_after_the_leave_is_an_error_the_caller_keeps_its_mark_on() {
    let mut ncp = on_network(0x4321, [9; 8]);
    ncp.refuse_concentrator = true;
    let mut h = harness(ncp);
    let error = start(&mut h, Some(&restored()), true).unwrap_err();
    assert!(
        matches!(
            error,
            RadioError::Refused {
                command: "setConcentrator",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(
        ids(&h).contains(&LEAVE),
        "the left network is gone: the mark stays for the next start"
    );
}

#[test]
fn a_radio_that_is_only_a_member_of_the_restored_network_is_still_refused() {
    // A radio that is a router on the restored network is resumed, not left: the
    // coordinator check refuses it, as for any radio that is only a member.
    let mut ncp = Ncp::recording();
    ncp.network = Some(StoredNetwork {
        extended_pan_wire: [8, 7, 6, 5, 4, 3, 2, 1],
        pan_id: 0x1A2B,
        channel: 15,
        node_type: 2,
    });
    let mut h = harness(ncp);
    // On the restored network as a router: resumed, then the coordinator check refuses it, as
    // for any radio that is only a member.
    let error = start(&mut h, Some(&restored()), true).unwrap_err();
    assert!(matches!(error, RadioError::NotCoordinator), "{error:?}");
    assert!(!ids(&h).contains(&LEAVE));
}
