use solite_core::{NAME, VERSION, banner};

#[test]
fn banner_shows_the_name_and_the_version() {
    assert_eq!(NAME, "solite");
    assert_eq!(banner(), format!("solite {VERSION}"));
}
