//! The three display languages.
//!
//! The C++ tree kept the strings in `display/languages.h` and chose between them with a
//! three-branch `if` in `initLangStrings()`. Here the choice is a lookup on a two-field enum, so
//! adding a language is a table row and a compile error rather than a forgotten branch.
//!
//! Only the labels a template actually draws are present. The C++ header also declared strings for
//! the calibration flow and the backflush reminder, which live on fullscreen screens this port
//! renders from [`crate::model::ScreenModel`] instead; those are named here too where a template
//! uses them.

/// The display language, matching the config value `display.language`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Language {
    #[default]
    English,
    Spanish,
    German,
}

impl Language {
    /// The value the configuration stores.
    pub const fn as_u8(self) -> u8 {
        match self {
            Language::English => 0,
            Language::Spanish => 1,
            Language::German => 2,
        }
    }

    /// Parses the stored value. An unknown value falls back to English rather than refusing to
    /// boot: a display language is a preference, not a safety parameter.
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Language::Spanish,
            2 => Language::German,
            _ => Language::English,
        }
    }
}

/// Every label the templates draw, for one language.
#[derive(Clone, Copy, Debug)]
pub struct Strings {
    pub set_temp: &'static str,
    pub current_temp: &'static str,
    pub brew: &'static str,
    pub weight: &'static str,
    pub manual_flush: &'static str,
    pub hot_water: &'static str,
    pub pressure: &'static str,
    pub uptime: &'static str,
    pub offline: &'static str,
    pub sensor_error_1: &'static str,
    pub sensor_error_2: &'static str,
    pub scale_fault: &'static str,
    pub backflush_start_1: &'static str,
    pub backflush_start_2: &'static str,
    pub tank_empty: &'static str,
    pub emergency_stop: &'static str,
    pub eeprom_error: &'static str,
    pub standby: &'static str,
    /// The upright template's single large word, which has no translation in the C++ tree either.
    pub upright_ready: &'static str,
    pub upright_waiting: &'static str,
    pub upright_brewing: &'static str,
    pub upright_flushing: &'static str,
    pub upright_clean: &'static str,
}

const EN: Strings = Strings {
    set_temp: "Set:",
    current_temp: "Temp:",
    brew: "Brew:",
    weight: "Weight:",
    manual_flush: "Flush:",
    hot_water: "Water:",
    pressure: "Press:",
    uptime: "Up:",
    offline: "Offline",
    sensor_error_1: "Error, Temp:",
    sensor_error_2: "Check sensor!",
    scale_fault: "Fault",
    backflush_start_1: "Press brew",
    backflush_start_2: "to start",
    tank_empty: "No water",
    emergency_stop: "EMERGENCY",
    eeprom_error: "EEPROM ERR",
    standby: "STANDBY",
    upright_ready: "OK",
    upright_waiting: "WAIT",
    upright_brewing: "BREW",
    upright_flushing: "FLUSH",
    upright_clean: "CLEAN",
};

const ES: Strings = Strings {
    set_temp: "Obj:",
    current_temp: "Temp:",
    brew: "Brew:",
    weight: "Peso:",
    manual_flush: "Fregar:",
    hot_water: "Agua:",
    pressure: "Pres:",
    uptime: "Up:",
    offline: "Offline",
    sensor_error_1: "Error, Temp:",
    sensor_error_2: "Comprueba sensor!",
    scale_fault: "falla",
    backflush_start_1: "Pulsa brew",
    backflush_start_2: "para iniciar",
    tank_empty: "Sin agua",
    emergency_stop: "EMERGENCIA",
    eeprom_error: "EEPROM ERR",
    standby: "STANDBY",
    upright_ready: "OK",
    upright_waiting: "ESPERA",
    upright_brewing: "BREW",
    upright_flushing: "FLUSH",
    upright_clean: "LIMPIEZA",
};

const DE: Strings = Strings {
    set_temp: "Soll:",
    current_temp: "Ist:",
    brew: "Bezug:",
    weight: "Gewicht:",
    manual_flush: "Spuelen:",
    hot_water: "Wasser:",
    pressure: "Druck:",
    uptime: "Up:",
    offline: "Offline",
    sensor_error_1: "Fehler, Temp:",
    sensor_error_2: "Sensor pruefen!",
    scale_fault: "Fehler",
    backflush_start_1: "Bezug druecken",
    backflush_start_2: "um zu starten",
    tank_empty: "Kein Wasser",
    emergency_stop: "NOT-AUS",
    eeprom_error: "EEPROM ERR",
    standby: "STANDBY",
    upright_ready: "OK",
    upright_waiting: "WARTEN",
    upright_brewing: "BREW",
    upright_flushing: "FLUSH",
    upright_clean: "REINIGEN",
};

impl Language {
    /// The label table for this language.
    pub const fn strings(self) -> &'static Strings {
        match self {
            Language::English => &EN,
            Language::Spanish => &ES,
            Language::German => &DE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_value_round_trips() {
        for l in [Language::English, Language::Spanish, Language::German] {
            assert_eq!(Language::from_u8(l.as_u8()), l);
        }
    }

    #[test]
    fn an_unknown_stored_value_falls_back_to_english() {
        assert_eq!(Language::from_u8(200), Language::English);
    }

    #[test]
    fn no_label_is_empty_and_every_language_defines_all_of_them() {
        // A missing label would render as a blank space on the panel with no error anywhere, so
        // the check is on the strings themselves rather than on a comparison between languages.
        for l in [Language::English, Language::Spanish, Language::German] {
            let s = l.strings();
            let all = [
                s.set_temp,
                s.current_temp,
                s.brew,
                s.weight,
                s.manual_flush,
                s.hot_water,
                s.pressure,
                s.uptime,
                s.offline,
                s.sensor_error_1,
                s.sensor_error_2,
                s.scale_fault,
                s.backflush_start_1,
                s.backflush_start_2,
                s.tank_empty,
                s.emergency_stop,
                s.eeprom_error,
                s.standby,
            ];
            for label in all {
                assert!(!label.is_empty(), "{l:?} has an empty label");
            }
        }
    }

    #[test]
    fn every_label_fits_the_row_it_is_drawn_in() {
        // The label column is 36 px wide in the shared temperature block, which is six small
        // cells. A seventh character would push into the value's fixed box.
        let long = [Language::English, Language::Spanish, Language::German];
        for l in long {
            let s = l.strings();
            for label in [s.set_temp, s.current_temp, s.brew, s.pressure] {
                assert!(
                    crate::font::str_width(label, crate::font::Font::Small) <= 36,
                    "{l:?} label {label:?} does not fit the 36 px label column"
                );
            }
        }
    }
}
