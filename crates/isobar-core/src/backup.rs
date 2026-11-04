//! Nightly copies of the history and settings, for a backup program to pick
//! up. The history is open while Isobar runs, so copying its file directly
//! can catch it half written; SQLite's `VACUUM INTO` writes a complete,
//! consistent copy instead, which is then left alone.

use std::path::{Path, PathBuf};

use chrono::{Duration, NaiveDate, NaiveDateTime, NaiveTime};

use crate::{Result, Store};

/// "21:00", "9:30 pm", "9pm" or "0915"; `None` if it isn't a time.
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let t = s.trim().to_ascii_lowercase().replace('.', ":");
    let (t, pm) = if let Some(x) = t.strip_suffix("pm") {
        (x.trim().to_string(), Some(true))
    } else if let Some(x) = t.strip_suffix("am") {
        (x.trim().to_string(), Some(false))
    } else {
        (t, None)
    };
    let (h, m) = match t.split_once(':') {
        Some((h, m)) => (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?),
        None if t.len() == 4 && pm.is_none() => (t[..2].parse().ok()?, t[2..].parse().ok()?),
        None => (t.parse::<u32>().ok()?, 0),
    };
    let h = match pm {
        Some(_) if !(1..=12).contains(&h) => return None,
        Some(true) => h % 12 + 12,
        Some(false) => h % 12,
        None => h,
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

/// Whether a backup is owed at `now`: the latest time-of-day `at` that has
/// passed is later than the last backup. A computer that was off at that
/// time catches up when Isobar next runs.
pub fn due(now: NaiveDateTime, at: NaiveTime, last: Option<NaiveDateTime>) -> bool {
    let today = now.date().and_time(at);
    let latest = if now >= today {
        today
    } else {
        today - Duration::days(1)
    };
    last.is_none_or(|l| l < latest)
}

/// The history copy for `day`.
pub fn history_name(day: NaiveDate) -> String {
    format!("isobar-{}.sqlite", day.format("%Y-%m-%d"))
}

/// The settings copy for `day`.
pub fn settings_name(day: NaiveDate) -> String {
    format!("settings-{}.json", day.format("%Y-%m-%d"))
}

/// Copy the history (and `settings`, if it exists) into `dir`, named for
/// `day`, replacing that day's copies, then keep only the newest `keep` of
/// each. The copy is written under a temporary name and renamed when
/// complete, so a backup program never sees half of one.
pub fn run(
    store: &Store,
    settings: Option<&Path>,
    dir: &Path,
    day: NaiveDate,
    keep: usize,
) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let target = dir.join(history_name(day));
    let part = dir.join(format!("{}.part", history_name(day)));
    let _ = std::fs::remove_file(&part);
    if let Err(e) = store.copy_to(&part) {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, &target)?;
    if let Some(s) = settings.filter(|s| s.exists()) {
        std::fs::copy(s, dir.join(settings_name(day)))?;
    }
    prune(dir, "isobar-", ".sqlite", keep)?;
    prune(dir, "settings-", ".json", keep)?;
    Ok(target)
}

/// Remove all but the newest `keep` dated copies named `prefix` +
/// YYYY-MM-DD + `suffix`. Nothing else in the folder is touched.
fn prune(dir: &Path, prefix: &str, suffix: &str, keep: usize) -> Result<()> {
    let mut found: Vec<(NaiveDate, PathBuf)> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let date = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
            let d = NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?;
            Some((d, e.path()))
        })
        .collect();
    found.sort_by_key(|b| std::cmp::Reverse(b.0));
    for (_, path) in found.into_iter().skip(keep.max(1)) {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn times() {
        let t = |h, m| NaiveTime::from_hms_opt(h, m, 0);
        assert_eq!(parse_time("21:00"), t(21, 0));
        assert_eq!(parse_time("9pm"), t(21, 0));
        assert_eq!(parse_time("9:30 PM"), t(21, 30));
        assert_eq!(parse_time("12 am"), t(0, 0));
        assert_eq!(parse_time("12pm"), t(12, 0));
        assert_eq!(parse_time("0915"), t(9, 15));
        assert_eq!(parse_time("21.15"), t(21, 15));
        assert_eq!(parse_time("25:00"), None);
        assert_eq!(parse_time("13pm"), None);
        assert_eq!(parse_time("soon"), None);
    }

    #[test]
    fn when_due() {
        let nine = NaiveTime::from_hms_opt(21, 0, 0).unwrap();
        // Never backed up: straight away.
        assert!(due(at("2026-11-04 08:00"), nine, None));
        // Last night's done; not again until tonight.
        let last = Some(at("2026-11-03 21:00"));
        assert!(!due(at("2026-11-04 08:00"), nine, last));
        assert!(!due(at("2026-11-04 20:59"), nine, last));
        assert!(due(at("2026-11-04 21:00"), nine, last));
        // Off last night: catches up in the morning.
        assert!(due(
            at("2026-11-04 08:00"),
            nine,
            Some(at("2026-11-02 21:05"))
        ));
        // A "Back up now" at noon doesn't skip tonight's.
        assert!(due(
            at("2026-11-04 21:01"),
            nine,
            Some(at("2026-11-04 12:00"))
        ));
    }

    #[test]
    fn copies_and_keeps_the_newest() {
        let dir = std::env::temp_dir().join(format!("isobar-backup-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        let db = src.join("isobar.sqlite");
        let store = Store::open(&db).unwrap();
        store.meta_set("hello", "there").unwrap();
        let settings = src.join("settings.json");
        std::fs::write(&settings, "{}").unwrap();
        let out = dir.join("Backups");
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("notes.txt"), "mine").unwrap();
        let d = NaiveDate::from_ymd_opt(2026, 11, 1).unwrap();
        for i in 0..5 {
            run(&store, Some(&settings), &out, d + Duration::days(i), 3).unwrap();
        }
        // Again on the same day replaces that day's copy.
        let last = run(&store, Some(&settings), &out, d + Duration::days(4), 3).unwrap();
        let mut names: Vec<String> = std::fs::read_dir(&out)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "isobar-2026-11-03.sqlite",
                "isobar-2026-11-04.sqlite",
                "isobar-2026-11-05.sqlite",
                "notes.txt",
                "settings-2026-11-03.json",
                "settings-2026-11-04.json",
                "settings-2026-11-05.json",
            ]
        );
        let copy = Store::open(&last).unwrap();
        assert_eq!(copy.meta_get("hello").unwrap().as_deref(), Some("there"));
        drop(copy);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
