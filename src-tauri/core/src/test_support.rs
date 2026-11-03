use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;
use serde::de::DeserializeOwned;

pub const ISO_MINUTE: &str = "%Y-%m-%dT%H:%M";

#[derive(Deserialize)]
pub struct FixtureSettings {
    pub day_start_hour: u32,
    pub block_rejoin_minutes: u32,
    pub block_min_minutes: u32,
}

// Every fixture's expected JSON was written under these settings, in Rust and in the vitest suite alike.
pub static FIXTURE_SETTINGS: LazyLock<FixtureSettings> = LazyLock::new(|| {
    let json = fs::read_to_string(fixtures_dir().join("settings.json")).unwrap();
    serde_json::from_str(&json).unwrap()
});

pub fn for_each_fixture<Expected: DeserializeOwned>(
    subdir: &str,
    mut check: impl FnMut(&Path, &str, Expected),
) {
    let dir = fixtures_dir().join(subdir);
    let mut checked = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|extension| extension != "md") {
            continue;
        }

        let text = fs::read_to_string(&path).unwrap();
        let json = fs::read_to_string(path.with_extension("json")).unwrap();
        check(&path, &text, serde_json::from_str(&json).unwrap());
        checked += 1;
    }
    assert!(checked > 0, "no fixtures in {}", dir.display());
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures")
}
