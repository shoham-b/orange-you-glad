//! Family members' Hebrew birthdays and their pictures, from `Config/family.json`.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::hebrew::{self, ADAR, ADAR_II, HebrewDate};
use crate::library::{self, PROCESSED_DIR, Subject};

/// Family file inside the library root, next to `subjects.json`.
pub const FAMILY_FILE: &str = "Config/family.json";

/// Wire format: `{"Dana": {"birthday": "15 Shevat", "pictures": ["uuid", ...]}}`. `pictures` are
/// uuids of files in `Processed`, like in `subjects.json`.
#[derive(Debug, Deserialize)]
struct FamilyFile(BTreeMap<String, Person>);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Person {
    birthday: Birthday,
    #[serde(default)]
    pictures: Vec<String>,
}

/// Which month of the Hebrew year a birthday falls in. Adar needs care in leap years.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Month {
    Fixed(u8),
    /// Plain "Adar": Adar II in a leap year, the only Adar otherwise.
    Adar,
    AdarI,
    AdarII,
}

/// A birthday as day and month of the Hebrew calendar, written like `"15 Shevat"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Birthday {
    day: u8,
    month: Month,
}

impl TryFrom<String> for Birthday {
    type Error = anyhow::Error;
    fn try_from(text: String) -> Result<Self> {
        text.parse()
    }
}

impl FromStr for Birthday {
    type Err = anyhow::Error;

    fn from_str(text: &str) -> Result<Self> {
        let (day, month) = text
            .trim()
            .split_once(char::is_whitespace)
            .with_context(|| format!("birthday '{text}' should look like '15 Shevat'"))?;
        let day: u8 =
            day.parse().with_context(|| format!("'{day}' is not a day number in '{text}'"))?;
        if !(1..=30).contains(&day) {
            bail!("day {day} in '{text}' must be between 1 and 30");
        }
        let month = parse_month(month)
            .with_context(|| format!("unknown Hebrew month '{}' in '{text}'", month.trim()))?;
        Ok(Self { day, month })
    }
}

fn parse_month(name: &str) -> Option<Month> {
    let key: String =
        name.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase();
    let fixed = match key.as_str() {
        "nisan" | "nissan" => 1,
        "iyar" | "iyyar" => 2,
        "sivan" => 3,
        "tamuz" | "tammuz" => 4,
        "av" | "menachemav" => 5,
        "elul" => 6,
        "tishrei" | "tishri" | "tishrey" => 7,
        "cheshvan" | "heshvan" | "marcheshvan" => 8,
        "kislev" => 9,
        "tevet" | "teves" => 10,
        "shevat" | "shvat" => 11,
        "adar" => return Some(Month::Adar),
        "adar1" | "adari" | "adaralef" => return Some(Month::AdarI),
        "adar2" | "adarii" | "adarbet" => return Some(Month::AdarII),
        _ => return None,
    };
    Some(Month::Fixed(fixed))
}

impl Birthday {
    /// Whether `today` is this birthday.
    ///
    /// A 30th that the year's month lacks (Cheshvan or Kislev) is kept on the month's last day.
    /// Adar I and II birthdays move to the only Adar in an ordinary year.
    pub fn falls_on(self, today: HebrewDate) -> bool {
        let leap = hebrew::is_leap_year(today.year);
        let month = match self.month {
            Month::Fixed(m) => m,
            Month::Adar | Month::AdarII if leap => ADAR_II,
            Month::AdarI if leap => ADAR,
            Month::Adar | Month::AdarI | Month::AdarII => ADAR,
        };
        month == today.month && self.day.min(hebrew::days_in_month(today.year, month)) == today.day
    }
}

/// One subject per person whose birthday is `today` and who has pictures that are synced,
/// named after the person.
///
/// A missing family file means nobody is celebrated.
///
/// # Errors
/// Fails when the family file or the pictures folder cannot be read, or the file is invalid.
pub fn birthday_subjects(root: &Path, today: HebrewDate) -> Result<Vec<Subject>> {
    let path = root.join(FAMILY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
    };
    let FamilyFile(people) =
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    let celebrated: Vec<_> =
        people.into_iter().filter(|(_, p)| p.birthday.falls_on(today)).collect();
    if celebrated.is_empty() {
        return Ok(Vec::new());
    }
    let files = library::index_processed(&root.join(PROCESSED_DIR))?;
    Ok(celebrated
        .into_iter()
        .filter_map(|(name, person)| library::resolve(name, &person.pictures, &files))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hebrew::{CHESHVAN, TISHREI};
    use std::fs;

    fn on(year: i64, month: u8, day: u8) -> HebrewDate {
        HebrewDate { year, month, day }
    }

    fn bday(text: &str) -> Birthday {
        text.parse().unwrap()
    }

    #[test]
    fn parses_names_spellings_and_rejects_nonsense() {
        assert_eq!(bday("15 Shevat"), bday(" 15  shvat "));
        assert_eq!(bday("1 Tishrei").month, Month::Fixed(TISHREI));
        assert_eq!(bday("14 Adar II").month, Month::AdarII);
        assert_eq!(bday("14 Adar I").month, Month::AdarI);
        for bad in ["Shevat", "0 Shevat", "31 Shevat", "x Shevat", "15 January", ""] {
            assert!(bad.parse::<Birthday>().is_err(), "{bad}");
        }
    }

    #[test]
    fn adar_birthdays_follow_leap_years() {
        let (leap, plain) = (5784, 5785);
        assert!(bday("14 Adar").falls_on(on(leap, ADAR_II, 14)));
        assert!(!bday("14 Adar").falls_on(on(leap, ADAR, 14)));
        assert!(bday("14 Adar").falls_on(on(plain, ADAR, 14)));
        assert!(bday("14 Adar I").falls_on(on(leap, ADAR, 14)));
        assert!(bday("14 Adar I").falls_on(on(plain, ADAR, 14)));
        assert!(bday("14 Adar II").falls_on(on(leap, ADAR_II, 14)));
        assert!(bday("14 Adar II").falls_on(on(plain, ADAR, 14)));
    }

    #[test]
    fn thirtieth_of_a_short_month_moves_to_its_last_day() {
        let short = (5780..5800).find(|&y| hebrew::days_in_month(y, CHESHVAN) == 29).unwrap();
        let long = (5780..5800).find(|&y| hebrew::days_in_month(y, CHESHVAN) == 30).unwrap();
        assert!(bday("30 Cheshvan").falls_on(on(short, CHESHVAN, 29)));
        assert!(!bday("30 Cheshvan").falls_on(on(short, CHESHVAN, 28)));
        assert!(bday("30 Cheshvan").falls_on(on(long, CHESHVAN, 30)));
        assert!(!bday("30 Cheshvan").falls_on(on(long, CHESHVAN, 29)));
    }

    fn library(family: Option<&str>, files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Config")).unwrap();
        fs::create_dir_all(dir.path().join(PROCESSED_DIR)).unwrap();
        if let Some(text) = family {
            fs::write(dir.path().join(FAMILY_FILE), text).unwrap();
        }
        for file in files {
            fs::write(dir.path().join(PROCESSED_DIR).join(file), b"x").unwrap();
        }
        dir
    }

    const FAMILY: &str = r#"{
        "Dana": {"birthday": "15 Shevat", "pictures": ["d1", "d2", "later"]},
        "Omer": {"birthday": "15 Shevat"},
        "Noa": {"birthday": "1 Nisan", "pictures": ["n1"]}
    }"#;

    #[test]
    fn birthday_person_with_synced_pictures_becomes_a_subject() {
        let dir = library(Some(FAMILY), &["d1.jpg", "d2.png", "n1.jpg"]);

        let subjects = birthday_subjects(dir.path(), on(5786, 11, 15)).unwrap();

        // Omer has the same birthday but no pictures; Noa's is another day.
        assert_eq!(subjects.len(), 1);
        assert_eq!(subjects[0].name, "Dana");
        assert_eq!(subjects[0].images.len(), 2);
        assert_eq!(birthday_subjects(dir.path(), on(5786, 11, 16)).unwrap(), vec![]);
    }

    #[test]
    fn missing_family_file_means_no_birthdays_and_bad_one_is_an_error() {
        let dir = library(None, &[]);
        assert_eq!(birthday_subjects(dir.path(), on(5786, 11, 15)).unwrap(), vec![]);
        for bad in ["{ nope", r#"{"A": {"birthday": "soon"}}"#, r#"{"A": {"pictures": []}}"#] {
            fs::write(dir.path().join(FAMILY_FILE), bad).unwrap();
            assert!(birthday_subjects(dir.path(), on(5786, 11, 15)).is_err(), "{bad}");
        }
    }
}
