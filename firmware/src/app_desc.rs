//! The ESP application descriptor of every image, with the firmware crate's own version (spec 032
//! R7): ESP-IDF's default one (weak) holds the git description of the checkout, which is not the
//! version the update's signed header carries. `scripts/validate.sh` checks this crate's `version`
//! equals the workspace's, the version `solite_core::VERSION` reports.

// The macro's own cfg names (`esp_idf_version_at_least_*`) come from the ESP-IDF build, and its
// link attributes are what the `unsafe_code` lint finds.
#![allow(unexpected_cfgs, unsafe_code)]

esp_idf_svc::sys::esp_app_desc! {}
