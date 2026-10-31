//! Versioned data read from LocalAppData. Validate at the file boundary before
//! launching a process, writing a profile, or applying Windows display changes.
use crate::display::Display;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn schema_one() -> u32 {
    1
}

#[derive(Deserialize, Serialize)]
pub struct Settings {
    #[serde(default = "schema_one")]
    pub version: u32,
    pub dcs: PathBuf,
    pub saved_games: PathBuf,
    pub source_profile: PathBuf,
    pub display: String,
    pub port: u16,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("Unsupported settings schema".into());
        }
        if self.port < 1024 {
            return Err("Port must be 1024..65535".into());
        }
        if !self.dcs.is_absolute()
            || !self.saved_games.is_absolute()
            || !self.source_profile.is_absolute()
        {
            return Err("Settings paths must be absolute".into());
        }
        if !self
            .dcs
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("DCS.exe"))
            || self.display.is_empty()
        {
            return Err("Invalid DCS executable or display in settings".into());
        }
        Ok(())
    }
    pub fn from_value(value: serde_json::Value) -> Result<Self, String> {
        let settings: Self =
            serde_json::from_value(value).map_err(|e| format!("Invalid settings: {e}"))?;
        settings.validate()?;
        Ok(settings)
    }
    pub fn value(self) -> Result<serde_json::Value, String> {
        serde_json::to_value(self).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
pub struct DisplayRecovery {
    #[serde(default = "schema_one")]
    version: u32,
    pub before: Display,
    pub after: Display,
}
impl DisplayRecovery {
    pub fn from_value(value: serde_json::Value) -> Result<Self, String> {
        let recovery: Self =
            serde_json::from_value(value).map_err(|e| format!("Invalid recovery record: {e}"))?;
        if recovery.version != 1 || recovery.before.device != recovery.after.device {
            return Err("Unsupported or inconsistent recovery record".into());
        }
        recovery.before.validate()?;
        recovery.after.validate()?;
        Ok(recovery)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn settings_reject_unsupported_schemas_and_narrowing_overflow() {
        let valid = json!({"dcs":"C:/DCS/bin/DCS.exe","saved_games":"C:/Saved Games","source_profile":"C:/Saved Games/DCS","display":"virtual","port":8090});
        assert!(Settings::from_value(valid.clone()).is_ok());
        for bad in [json!(65536), json!(-1), json!(80)] {
            let mut value = valid.clone();
            value["port"] = bad;
            assert!(Settings::from_value(value).is_err());
        }
        let mut value = valid.clone();
        value["version"] = json!(2);
        assert!(Settings::from_value(value).is_err());
        let mut value = valid;
        value.as_object_mut().unwrap().remove("source_profile");
        assert!(Settings::from_value(value).is_err());
    }
    #[test]
    fn recovery_rejects_coordinates_outside_the_windows_integer_range() {
        let display = json!({"device":"virtual","description":"Virtual Display","active":true,"primary":false,"x":0,"y":0,"width":1024,"height":512,"frequency":60});
        let mut value = json!({"before":display,"after":display});
        assert!(DisplayRecovery::from_value(value.clone()).is_ok());
        value["before"]["x"] = json!(i64::MAX);
        assert!(DisplayRecovery::from_value(value).is_err());
    }
}
