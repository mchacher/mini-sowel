use mini_sowel_core::{NAME, VERSION, banner};

#[test]
fn banner_shows_the_name_and_the_version() {
    assert_eq!(NAME, "mini-sowel");
    assert_eq!(banner(), format!("mini-sowel {VERSION}"));
}
