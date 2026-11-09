use std::ops::RangeInclusive;
use std::path::Path;

use punchcard_core::day::DayStart;

use super::{TimerError, read_lines};
use crate::note;

pub(super) const SETTINGS: &str = "_Meta/Settings.md";
pub(super) const DAY_START_KEY: &str = "day_start_hour";
pub(super) const REJOIN_KEY: &str = "block_rejoin_minutes";
pub(super) const MIN_BLOCK_KEY: &str = "block_min_minutes";
pub(super) const TIMELINE_BLOCK_KEY: &str = "timeline_block_minutes";
const MINUTES_IN_A_DAY: RangeInclusive<u32> = 0..=1440;

#[derive(Debug)]
pub struct Settings {
    pub day_start: DayStart,
    pub block_rejoin_minutes: u32,
    pub block_min_minutes: u32,
    pub timeline_block_minutes: u32,
}

pub fn read(vault: &Path) -> Result<Settings, TimerError> {
    let path = vault.join(SETTINGS);
    let lines = read_lines(&path).map_err(|error| {
        let message = format!("{}: {error}", path.display());
        TimerError(message)
    })?;

    let fields = note::parse_frontmatter(&lines).unwrap_or_default().fields;
    let setting = |key: &str, unit: &str, range: RangeInclusive<u32>| {
        let value = fields
            .get(key)
            .and_then(|values| values.first()?.parse().ok());
        value.filter(|value| range.contains(value)).ok_or_else(|| {
            let (low, high) = range.into_inner();
            let message = format!(
                "{} has no {key} that is {unit} from {low} to {high}",
                path.display()
            );
            TimerError(message)
        })
    };
    let day_start = setting(DAY_START_KEY, "an hour", DayStart::HOURS)?;
    Ok(Settings {
        day_start: DayStart::new(day_start).expect("the setting is in DayStart::HOURS"),
        block_rejoin_minutes: setting(REJOIN_KEY, "a number of minutes", MINUTES_IN_A_DAY)?,
        block_min_minutes: setting(MIN_BLOCK_KEY, "a number of minutes", MINUTES_IN_A_DAY)?,
        // A new block must end after its start, so it is at least a minute long.
        timeline_block_minutes: setting(
            TIMELINE_BLOCK_KEY,
            "a number of minutes",
            1..=*MINUTES_IN_A_DAY.end(),
        )?,
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::tests::TempVault;
    use super::*;

    #[test]
    fn day_start_hour_must_be_an_hour_of_the_day() {
        let vault = TempVault::new("day-start");
        fs::write(vault.path.join(SETTINGS), "---\nday_start_hour: 24\n---\n").unwrap();

        let error = read(&vault.path).unwrap_err();

        assert!(error.to_string().ends_with("an hour from 0 to 23"));
    }

    #[test]
    fn block_minutes_must_be_set() {
        let vault = TempVault::new("block-minutes");
        fs::write(vault.path.join(SETTINGS), "---\nday_start_hour: 4\n---\n").unwrap();

        let error = read(&vault.path).unwrap_err();

        assert!(
            error.to_string().ends_with(
                "has no block_rejoin_minutes that is a number of minutes from 0 to 1440"
            )
        );
    }

    #[test]
    fn timeline_block_minutes_must_be_at_least_one() {
        let vault = TempVault::new("timeline-block-zero");
        let text = "---\nday_start_hour: 4\nblock_rejoin_minutes: 5\nblock_min_minutes: 3\ntimeline_block_minutes: 0\n---\n";
        fs::write(vault.path.join(SETTINGS), text).unwrap();

        let error = read(&vault.path).unwrap_err();

        assert!(
            error.to_string().ends_with(
                "has no timeline_block_minutes that is a number of minutes from 1 to 1440"
            )
        );
    }
}
