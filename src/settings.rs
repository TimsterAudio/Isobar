use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Alerts {
    pub enabled: bool,
    /// Notify when a gust reaches this speed (station wind units).
    pub gust_above: Option<f64>,
    /// Notify once a day when the temperature drops to this value.
    pub frost_below: Option<f64>,
    /// Notify once a day when the temperature reaches this value.
    pub heat_above: Option<f64>,
    /// Notify when rain starts after a dry spell of at least three hours.
    pub rain_start: bool,
    /// Notify when MX says the station has stopped sending data.
    pub stopped: bool,
}

/// Units to show instead of the station's; empty for the station's own.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct UnitChoice {
    pub temp: String,
    pub rain: String,
    pub wind: String,
    pub press: String,
}

impl UnitChoice {
    /// The units shown, for a station sending `station`.
    pub fn apply(&self, station: &isobar_core::Units) -> isobar_core::Units {
        isobar_core::units::chosen(station, &self.temp, &self.rain, &self.wind, &self.press)
    }
}

/// The nightly copy of the history and settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Backup {
    pub enabled: bool,
    /// Local time of day, as typed ("21:00").
    pub time: String,
    /// How many nights' copies to keep.
    pub keep: u32,
}

impl Default for Backup {
    fn default() -> Self {
        Backup {
            enabled: true,
            time: "21:00".into(),
            keep: 7,
        }
    }
}

impl Default for Alerts {
    fn default() -> Self {
        Alerts {
            enabled: true,
            gust_above: Some(60.0),
            frost_below: Some(2.0),
            heat_above: Some(35.0),
            rain_start: true,
            stopped: true,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
    Warm,
    Contrast,
    Night,
    Blueprint,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 7] = [
        ThemeChoice::System,
        ThemeChoice::Light,
        ThemeChoice::Dark,
        ThemeChoice::Warm,
        ThemeChoice::Contrast,
        ThemeChoice::Night,
        ThemeChoice::Blueprint,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ThemeChoice::System => "Match system",
            ThemeChoice::Light => "Light",
            ThemeChoice::Dark => "Dark",
            ThemeChoice::Warm => "Warm paper",
            ThemeChoice::Contrast => "High contrast",
            ThemeChoice::Night => "Night",
            ThemeChoice::Blueprint => "Blueprint",
        }
    }
}

/// A card under the thermograph on Now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NowCard {
    Wind,
    Pressure,
    Rain,
    Humidity,
    Feels,
    Sun,
    Indoor,
    Tanks,
}

impl NowCard {
    /// The usual order.
    pub const ALL: [NowCard; 8] = [
        NowCard::Wind,
        NowCard::Pressure,
        NowCard::Rain,
        NowCard::Humidity,
        NowCard::Feels,
        NowCard::Sun,
        NowCard::Indoor,
        NowCard::Tanks,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NowCard::Wind => "Wind",
            NowCard::Pressure => "Pressure",
            NowCard::Rain => "Rain",
            NowCard::Humidity => "Humidity",
            NowCard::Feels => "Feels like",
            NowCard::Sun => "Sun",
            NowCard::Indoor => "Indoor",
            NowCard::Tanks => "Tanks",
        }
    }

    /// The name kept in settings.json.
    pub fn id(self) -> &'static str {
        match self {
            NowCard::Wind => "wind",
            NowCard::Pressure => "pressure",
            NowCard::Rain => "rain",
            NowCard::Humidity => "humidity",
            NowCard::Feels => "feels",
            NowCard::Sun => "sun",
            NowCard::Indoor => "indoor",
            NowCard::Tanks => "tanks",
        }
    }

    pub fn from_id(id: &str) -> Option<NowCard> {
        NowCard::ALL.into_iter().find(|c| c.id() == id)
    }
}

/// A card's place on Now and whether it shows. Kept by name, so a settings
/// file from a later version with cards this one doesn't know still loads.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardChoice {
    pub card: String,
    pub shown: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub station_name: String,
    /// Base address of the MX interface, e.g. `http://192.168.1.20:8998`.
    pub station_url: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// Optional path to a `dayfile.txt`, used when no station address is set.
    pub dayfile_path: Option<String>,
    pub poll_seconds: u64,
    pub theme: ThemeChoice,
    /// 24-hour times ("16:52") rather than 12-hour ("4:52 PM").
    pub clock_24h: bool,
    /// The side rail stays open; otherwise it slides out from the left edge.
    pub rail_pinned: bool,
    /// The station's latitude in degrees, south negative. Without it, Isobar
    /// works it out from the day's sunrise and sunset.
    pub latitude: Option<f64>,
    /// The station's longitude in degrees, west negative, for the forecast.
    pub longitude: Option<f64>,
    /// The place the location was found from ("Leongatha, Victoria,
    /// Australia"), to show beside the forecast.
    pub place: Option<String>,
    /// Rainwater tanks, once set up.
    pub tanks: Option<isobar_core::tanks::Tanks>,
    /// The cards on Now, in order. Empty means the usual order, all shown.
    pub now_cards: Vec<CardChoice>,
    pub alerts: Alerts,
    pub backup: Backup,
    pub units: UnitChoice,
    /// A readout in the system tray.
    pub tray: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            station_name: "My station".into(),
            station_url: String::new(),
            username: None,
            password: None,
            dayfile_path: None,
            poll_seconds: 10,
            theme: ThemeChoice::System,
            clock_24h: true,
            rail_pinned: true,
            latitude: None,
            longitude: None,
            place: None,
            tanks: None,
            now_cards: Vec::new(),
            alerts: Alerts::default(),
            backup: Backup::default(),
            units: UnitChoice::default(),
            tray: true,
        }
    }
}

impl Settings {
    pub fn load(path: &Path) -> Settings {
        Settings::read(path).unwrap_or_default().with_keychain(path)
    }

    /// Settings from the shared folder, with the password from this
    /// computer's keychain.
    pub fn read_shared(path: &Path) -> Option<Settings> {
        Settings::read(path).map(|s| s.with_keychain(path))
    }

    /// The file as it is, or None if it can't be read.
    fn read(path: &Path) -> Option<Settings> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    }

    /// The password from the keychain, or one still in the file moved into
    /// it (and out of the file at `path`).
    fn with_keychain(mut self, path: &Path) -> Settings {
        match self.password.clone() {
            Some(p) => {
                if crate::keychain::set(Some(&p)).is_ok() {
                    let _ = self.save(path);
                }
            }
            None => {
                if let Ok(Some(p)) = crate::keychain::get() {
                    self.password = Some(p);
                }
            }
        }
        self
    }

    /// Write the settings to `path`. The password goes to the keychain, and
    /// into the file only where there's no keychain.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut out = self.clone();
        if crate::keychain::set(self.password.as_deref()).is_ok() {
            out.password = None;
        }
        let json = serde_json::to_string_pretty(&out).map_err(std::io::Error::other)?;
        // A new file renamed into place, so a reader (another computer, over
        // the network) never sees half of it.
        let tmp = path.with_extension("json.part");
        std::fs::write(&tmp, json)?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    }

    /// The settings at start: from the shared folder when there is one and
    /// it can be reached (copied to this computer too), else this
    /// computer's own. With a note if the shared folder couldn't be read.
    pub fn load_from(paths: &Paths) -> (Settings, Option<String>) {
        let Some(shared) = paths.shared_settings() else {
            return (Settings::load(&paths.settings), None);
        };
        match Settings::read(&shared) {
            Some(s) => {
                let s = s.with_keychain(&shared);
                let _ = s.save(&paths.settings);
                (s, None)
            }
            None if !shared.exists() && shared.parent().is_some_and(|d| d.is_dir()) => {
                // The folder's there but empty: start it with ours.
                let s = Settings::load(&paths.settings);
                let _ = s.save(&shared);
                (s, None)
            }
            None => (
                Settings::load(&paths.settings),
                Some(format!(
                    "Couldn't read the shared settings at {}, so Isobar is using this computer's copy. Changes made here are saved to both once the folder's back.",
                    shared.display()
                )),
            ),
        }
    }

    /// Save to this computer's settings and, if there is one, the shared
    /// folder. An error says which failed.
    pub fn store(&self, paths: &Paths) -> Result<(), String> {
        self.save(&paths.settings).map_err(|e| e.to_string())?;
        if let Some(shared) = paths.shared_settings() {
            // Never made afresh here: an unmounted share would otherwise
            // become an ordinary folder on this computer.
            let there = shared.parent().is_some_and(|d| d.is_dir());
            let saved = if there {
                self.save(&shared)
            } else {
                Err(std::io::Error::other("the folder isn't there"))
            };
            saved.map_err(|e| {
                format!(
                    "Saved on this computer. The shared folder isn't reachable ({e}); the settings go across when it's back"
                )
            })?;
        }
        Ok(())
    }

    /// Every card in its order on Now, and whether it shows. Cards missing
    /// from the settings (new in this version) go at the end, shown.
    pub fn cards(&self) -> Vec<(NowCard, bool)> {
        let mut out: Vec<(NowCard, bool)> = Vec::new();
        for c in &self.now_cards {
            if let Some(card) = NowCard::from_id(&c.card) {
                if !out.iter().any(|(k, _)| *k == card) {
                    out.push((card, c.shown));
                }
            }
        }
        for card in NowCard::ALL {
            if !out.iter().any(|(k, _)| *k == card) {
                out.push((card, true));
            }
        }
        out
    }

    pub fn set_cards(&mut self, cards: &[(NowCard, bool)]) {
        self.now_cards = cards
            .iter()
            .map(|(c, shown)| CardChoice {
                card: c.id().to_string(),
                shown: *shown,
            })
            .collect();
    }

    pub fn has_station(&self) -> bool {
        !self.station_url.trim().is_empty()
    }

    pub fn dayfile(&self) -> Option<&str> {
        self.dayfile_path
            .as_deref()
            .filter(|p| !p.trim().is_empty())
    }
}

/// Where Isobar keeps its files: the usual per-user folders (the same ones
/// the first, Tauri, version used, so settings and history carry over), or
/// one folder the person chose with Settings, History and Reports inside.
/// The choice lives in a small `folder.json` at the usual settings place.
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    pub settings: PathBuf,
    pub database: PathBuf,
    pub reports: PathBuf,
    /// Where the nightly copies go.
    pub backups: PathBuf,
    /// The chosen folder, if not the usual ones.
    pub folder: Option<PathBuf>,
    /// A folder shared with other computers (a network share, say) for the
    /// settings and the tank record. The history stays on each computer.
    pub shared: Option<PathBuf>,
}

/// What `folder.json` holds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
struct FolderChoice {
    folder: Option<PathBuf>,
    /// A history database to move into place at the next start.
    move_history_from: Option<PathBuf>,
    /// The shared settings folder.
    shared: Option<PathBuf>,
}

const ID: &str = "app.isobar.desktop";

fn pointer() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(ID)
        .join("folder.json")
}

fn read_choice() -> FolderChoice {
    std::fs::read_to_string(pointer())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_choice(c: &FolderChoice) -> Result<(), String> {
    let p = pointer();
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, serde_json::to_string_pretty(c).unwrap_or_default())
        .map_err(|e| format!("couldn't write {}: {e}", p.display()))
}

/// The history database and the two files SQLite keeps beside it.
fn database_files(db: &Path) -> [PathBuf; 3] {
    let name = |ext: &str| {
        let mut s = db.as_os_str().to_os_string();
        s.push(ext);
        PathBuf::from(s)
    };
    [db.to_path_buf(), name("-wal"), name("-shm")]
}

impl Paths {
    /// The usual per-user folders.
    pub fn usual() -> Paths {
        let config = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(ID);
        let data = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(ID);
        Paths {
            settings: config.join("settings.json"),
            database: data.join("isobar.sqlite"),
            reports: dirs::document_dir()
                .or_else(dirs::home_dir)
                .map_or_else(|| data.join("reports"), |d| d.join("Isobar reports")),
            backups: dirs::document_dir()
                .or_else(dirs::home_dir)
                .map_or_else(|| data.join("Backups"), |d| d.join("Isobar backups")),
            folder: None,
            shared: None,
        }
    }

    /// Everything in one folder, in Settings, History and Reports.
    pub fn in_folder(folder: &Path) -> Paths {
        Paths {
            settings: folder.join("Settings").join("settings.json"),
            database: folder.join("History").join("isobar.sqlite"),
            reports: folder.join("Reports"),
            backups: folder.join("Backups"),
            folder: Some(folder.to_path_buf()),
            shared: None,
        }
    }

    pub fn locate() -> Paths {
        let choice = read_choice();
        let mut p = match &choice.folder {
            Some(f) => Paths::in_folder(f),
            None => Paths::usual(),
        };
        p.shared = choice.shared;
        p
    }

    /// The shared settings file, if settings are shared.
    pub fn shared_settings(&self) -> Option<PathBuf> {
        self.shared.as_ref().map(|d| d.join("settings.json"))
    }

    /// The shared tank record, if settings are shared.
    pub fn shared_tank_record(&self) -> Option<PathBuf> {
        self.shared.as_ref().map(|d| d.join("tank-record.json"))
    }

    /// Share the settings and tank record through `folder` from now on (or
    /// stop, for None). If the folder has settings in it already (another
    /// computer's), those are returned to use; otherwise this computer's
    /// are copied there.
    pub fn share(folder: Option<PathBuf>, settings: &Settings) -> Result<Option<Settings>, String> {
        let mut choice = read_choice();
        let Some(dir) = folder else {
            choice.shared = None;
            write_choice(&choice)?;
            return Ok(None);
        };
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("couldn't reach the folder {}: {e}", dir.display()))?;
        let file = dir.join("settings.json");
        let theirs = Settings::read(&file);
        if theirs.is_none() {
            settings
                .save(&file)
                .map_err(|e| format!("couldn't write {}: {e}", file.display()))?;
        }
        choice.shared = Some(dir);
        write_choice(&choice)?;
        Ok(theirs.map(|s| s.with_keychain(&file)))
    }

    /// A history move waiting for the next start, from where.
    pub fn pending_move() -> Option<PathBuf> {
        read_choice().move_history_from
    }

    /// A folder to suggest: Isobar in Documents.
    pub fn suggested_folder() -> PathBuf {
        dirs::document_dir()
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Isobar")
    }

    /// Keep Isobar's files in `folder` (or the usual folders, for None)
    /// from now on. Settings copy across at once; the history moves at the
    /// next start, as it's in use. Files already there (another computer's,
    /// or from before) are kept and used instead. Returns what happened, in
    /// words.
    pub fn choose(folder: Option<PathBuf>, current: &Paths) -> Result<String, String> {
        let target = folder
            .as_deref()
            .map_or_else(Paths::usual, Paths::in_folder);
        if target.database == current.database {
            return Ok("Isobar's files are already there.".into());
        }
        for dir in [
            target.settings.parent(),
            target.database.parent(),
            Some(target.reports.as_path()),
        ]
        .into_iter()
        .flatten()
        {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("couldn't make the folder {}: {e}", dir.display()))?;
        }
        let mut notes = Vec::new();
        if target.settings.exists() {
            notes.push("There are settings there already; Isobar will use those.");
        } else if current.settings.exists() {
            // Moved, not copied, so an old copy can't be picked up later.
            std::fs::copy(&current.settings, &target.settings)
                .map_err(|e| format!("couldn't copy the settings: {e}"))?;
            let _ = std::fs::remove_file(&current.settings);
        }
        let move_history_from = if target.database.exists() {
            notes.push("There's a history there already; Isobar will use that one and leave this one where it is.");
            None
        } else {
            current.database.exists().then(|| current.database.clone())
        };
        let moving = move_history_from.is_some();
        write_choice(&FolderChoice {
            folder,
            move_history_from,
            shared: read_choice().shared,
        })?;
        let mut out = String::from(if moving {
            "Done. Restart Isobar to finish: the history moves across as it starts."
        } else {
            "Done. Restart Isobar to start using them."
        });
        for n in notes {
            out.push(' ');
            out.push_str(n);
        }
        Ok(out)
    }

    /// At startup, before the history opens: move it into place if a move
    /// is waiting. A copy first, then the old files go, so a failure part
    /// way leaves the old history where it was. Returns a problem, if any.
    pub fn finish_move(&self) -> Option<String> {
        let mut choice = read_choice();
        let from = choice.move_history_from.clone()?;
        let done = |choice: &mut FolderChoice| {
            choice.move_history_from = None;
            let _ = write_choice(choice);
        };
        if self.database.exists() || !from.exists() || from == self.database {
            done(&mut choice);
            return None;
        }
        if let Some(dir) = self.database.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let pairs: Vec<(PathBuf, PathBuf)> = database_files(&from)
            .into_iter()
            .zip(database_files(&self.database))
            .filter(|(a, _)| a.exists())
            .collect();
        // The Isobar that asked for the restart may take a moment to let go
        // of the files.
        let mut last = String::new();
        for _ in 0..20 {
            match pairs
                .iter()
                .try_for_each(|(a, b)| std::fs::copy(a, b).map(|_| ()))
            {
                Ok(()) => {
                    for (a, _) in &pairs {
                        let _ = std::fs::remove_file(a);
                    }
                    done(&mut choice);
                    return None;
                }
                Err(e) => last = e.to_string(),
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        for (_, b) in &pairs {
            let _ = std::fs::remove_file(b);
        }
        Some(format!(
            "Couldn't move the history to {} ({last}). It's still at {}; Isobar will try again next time it starts.",
            self.database.display(),
            from.display()
        ))
    }
}

/// Why the shared settings couldn't be read at start, if they couldn't.
static SHARED_PROBLEM: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn set_shared_problem(p: Option<String>) {
    *SHARED_PROBLEM.lock().unwrap() = p;
}

pub fn shared_problem() -> Option<String> {
    SHARED_PROBLEM.lock().unwrap().clone()
}

/// True for a folder that looks like a network share, where the history
/// database shouldn't live.
pub fn looks_like_network(p: &Path) -> bool {
    let s = p.to_string_lossy();
    s.starts_with("\\\\")
        || s.starts_with("//")
        || s.starts_with("smb:")
        || s.starts_with("/mnt/smb")
        || s.contains("/gvfs/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_settings_from_the_first_version() {
        let json = r#"{"stationName":"Home","stationUrl":"http://pi:8998","username":null,
            "password":null,"dayfilePath":null,"pollSeconds":15,"theme":"dark",
            "alerts":{"enabled":true,"gustAbove":70,"frostBelow":null,"heatAbove":38,"rainStart":false}}"#;
        let s: Settings = serde_json::from_str(json).unwrap();
        assert_eq!(s.station_name, "Home");
        assert_eq!(s.theme, ThemeChoice::Dark);
        assert_eq!(s.alerts.gust_above, Some(70.0));
        assert_eq!(s.alerts.frost_below, None);
        assert!(s.has_station());
    }

    #[test]
    fn card_order_survives_unknown_and_missing_cards() {
        let s: Settings = serde_json::from_str(
            r#"{"nowCards":[{"card":"rain","shown":true},{"card":"radar","shown":true},
            {"card":"wind","shown":false},{"card":"rain","shown":false}]}"#,
        )
        .unwrap();
        let cards = s.cards();
        assert_eq!(cards.len(), NowCard::ALL.len());
        assert_eq!(cards[0], (NowCard::Rain, true));
        assert_eq!(cards[1], (NowCard::Wind, false));
        assert_eq!(cards[2], (NowCard::Pressure, true));
        assert_eq!(Settings::default().cards()[0], (NowCard::Wind, true));
    }

    #[test]
    fn missing_fields_take_defaults() {
        let s: Settings = serde_json::from_str(r#"{"stationUrl":"x"}"#).unwrap();
        assert_eq!(s.poll_seconds, 10);
        assert!(s.clock_24h);
        assert!(s.rail_pinned);
        assert_eq!(s.theme, ThemeChoice::System);
    }

    #[test]
    fn shared_settings() {
        let dir = std::env::temp_dir().join(format!("isobar-share-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut paths = Paths::in_folder(&dir.join("here"));
        paths.shared = Some(dir.join("nas"));
        let mine = Settings {
            station_name: "Mine".into(),
            ..Default::default()
        };
        mine.save(&paths.settings).unwrap();
        // The share isn't there: this computer's copy, with a note, and a
        // save doesn't make the folder.
        let (s, problem) = Settings::load_from(&paths);
        assert_eq!(s.station_name, "Mine");
        assert!(problem.is_some());
        let err = s.store(&paths).unwrap_err();
        assert!(err.starts_with("Saved on this computer"), "{err}");
        assert!(!dir.join("nas").exists());
        // An empty share is started with this computer's settings.
        std::fs::create_dir_all(dir.join("nas")).unwrap();
        let (s, problem) = Settings::load_from(&paths);
        assert!(problem.is_none());
        assert_eq!(s.station_name, "Mine");
        assert!(paths.shared_settings().unwrap().exists());
        // Another computer's change is what this one starts with next.
        let theirs = Settings {
            station_name: "Theirs".into(),
            ..Default::default()
        };
        theirs.save(&paths.shared_settings().unwrap()).unwrap();
        let (s, _) = Settings::load_from(&paths);
        assert_eq!(s.station_name, "Theirs");
        assert_eq!(Settings::load(&paths.settings).station_name, "Theirs");
        assert_eq!(
            paths.shared_tank_record().unwrap(),
            dir.join("nas").join("tank-record.json")
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_folder_for_everything() {
        let p = Paths::in_folder(Path::new("/home/me/Documents/Isobar"));
        assert_eq!(
            p.settings,
            Path::new("/home/me/Documents/Isobar/Settings/settings.json")
        );
        assert_eq!(
            p.database,
            Path::new("/home/me/Documents/Isobar/History/isobar.sqlite")
        );
        assert_eq!(p.reports, Path::new("/home/me/Documents/Isobar/Reports"));
        let files = database_files(&p.database);
        assert!(files[1].to_string_lossy().ends_with("isobar.sqlite-wal"));
        assert!(looks_like_network(Path::new(r"\\nas\backup\Isobar")));
        assert!(!looks_like_network(Path::new(
            r"C:\Users\me\Documents\Isobar"
        )));
    }
}
