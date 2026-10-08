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

/// Wire format: `{"Dana": {"birthday": "15/5", "pictures": ["uuid", ...]}}`. `pictures` are
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

/// A birthday as day and month of the Hebrew calendar, written `"day/month"` with the months
/// counted from Tishrei = 1: Cheshvan 2, Kislev 3, Tevet 4, Shevat 5, Adar 6 (7 also means Adar,
/// as Adar II), Nisan 8, Iyar 9, Sivan 10, Tamuz 11, Av 12, Elul 13. So `"15/5"` is 15 Shevat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(try_from = "String")]
pub struct Birthday {
    day: u8,
    month: u8,
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
            .split_once('/')
            .with_context(|| format!("birthday '{text}' should look like '15/5' (day/month)"))?;
        let number = |part: &str, what: &str| -> Result<u8> {
            part.trim().parse().with_context(|| format!("'{part}' is not a {what} in '{text}'"))
        };
        let (day, month) = (number(day, "day")?, number(month, "month")?);
        if !(1..=30).contains(&day) {
            bail!("day {day} in '{text}' must be between 1 and 30");
        }
        if !(1..=13).contains(&month) {
            bail!("month {month} in '{text}' must be between 1 (Tishrei) and 13 (Elul)");
        }
        Ok(Self { day, month })
    }
}

impl Birthday {
    /// Whether `today` is this birthday.
    ///
    /// A 30th that the year's month lacks (Cheshvan or Kislev) is kept on the month's last day.
    /// An Adar birthday is in Adar II in a leap year.
    pub fn falls_on(self, today: HebrewDate) -> bool {
        let month = match self.month {
            m @ 1..=5 => m + 6,
            6 | 7 if hebrew::is_leap_year(today.year) => ADAR_II,
            6 | 7 => ADAR,
            m => m - 7,
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
    fn parses_day_slash_month_and_rejects_nonsense() {
        assert_eq!(bday("15/5"), bday(" 15 / 5 "));
        assert_eq!(bday("1/1").month, 1);
        for bad in ["5", "0/5", "31/5", "15/0", "15/14", "x/5", "15 Shevat", ""] {
            assert!(bad.parse::<Birthday>().is_err(), "{bad}");
        }
    }

    #[test]
    fn month_numbers_count_from_tishrei() {
        assert!(bday("27/1").falls_on(on(5787, TISHREI, 27)));
        assert!(bday("15/5").falls_on(on(5786, 11, 15))); // Shevat
        assert!(bday("1/8").falls_on(on(5786, 1, 1))); // Nisan
        assert!(bday("29/13").falls_on(on(5786, 6, 29))); // Elul
    }

    #[test]
    fn adar_birthdays_follow_leap_years() {
        let (leap, plain) = (5784, 5785);
        for text in ["14/6", "14/7"] {
            assert!(bday(text).falls_on(on(leap, ADAR_II, 14)));
            assert!(!bday(text).falls_on(on(leap, ADAR, 14)));
            assert!(bday(text).falls_on(on(plain, ADAR, 14)));
        }
    }

    #[test]
    fn thirtieth_of_a_short_month_moves_to_its_last_day() {
        let short = (5780..5800).find(|&y| hebrew::days_in_month(y, CHESHVAN) == 29).unwrap();
        let long = (5780..5800).find(|&y| hebrew::days_in_month(y, CHESHVAN) == 30).unwrap();
        assert!(bday("30/2").falls_on(on(short, CHESHVAN, 29)));
        assert!(!bday("30/2").falls_on(on(short, CHESHVAN, 28)));
        assert!(bday("30/2").falls_on(on(long, CHESHVAN, 30)));
        assert!(!bday("30/2").falls_on(on(long, CHESHVAN, 29)));
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
        "Dana": {"birthday": "15/5", "pictures": ["d1", "d2", "later"]},
        "Omer": {"birthday": "15/5"},
        "Noa": {"birthday": "1/8", "pictures": ["n1"]}
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
