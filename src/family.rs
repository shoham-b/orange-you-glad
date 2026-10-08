//! Family members' Hebrew birthdays, from `Config/family.json`.

use std::collections::BTreeMap;
use std::path::Path;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::hebrew::{self, ADAR, ADAR_II, HebrewDate};

/// Family file inside the library root, next to `subjects.json`.
pub const FAMILY_FILE: &str = "Config/family.json";

/// Wire format: `{"Dana": "15/5", ...}`, name to birthday. A name is also the subject in
/// `subjects.json` that holds the person's pictures.
#[derive(Debug, Deserialize)]
struct FamilyFile(BTreeMap<String, Birthday>);

/// Month of a birthday, numbered from Tishrei = 1 as in an ordinary year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Month {
    /// 1 to 5 (Tishrei to Shevat) or 7 to 12 (Nisan to Elul).
    Number(u8),
    /// `6`: Adar II in a leap year, the only Adar otherwise.
    Adar,
    /// `6a`: Adar I in a leap year, the only Adar otherwise.
    AdarI,
    /// `6b`: Adar II in a leap year, the only Adar otherwise.
    AdarII,
}

/// A birthday as day and month of the Hebrew calendar, written `"day/month"` with the months
/// counted from Tishrei = 1: Cheshvan 2, Kislev 3, Tevet 4, Shevat 5, Adar 6 (`6a` Adar I, `6b`
/// Adar II), Nisan 7, Iyar 8, Sivan 9, Tamuz 10, Av 11, Elul 12. So `"15/5"` is 15 Shevat.
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
            .split_once('/')
            .with_context(|| format!("birthday '{text}' should look like '15/5' (day/month)"))?;
        let day: u8 = day
            .trim()
            .parse()
            .with_context(|| format!("'{day}' is not a day number in '{text}'"))?;
        if !(1..=30).contains(&day) {
            bail!("day {day} in '{text}' must be between 1 and 30");
        }
        let month = match month.trim().to_ascii_lowercase().as_str() {
            "6" => Month::Adar,
            "6a" => Month::AdarI,
            "6b" => Month::AdarII,
            other => match other.parse::<u8>() {
                Ok(n @ (1..=5 | 7..=12)) => Month::Number(n),
                _ => bail!("month '{other}' in '{text}' must be 1 to 12, or 6a or 6b for Adar"),
            },
        };
        Ok(Self { day, month })
    }
}

impl Birthday {
    /// Whether `today` is this birthday.
    ///
    /// A 30th that the year's month lacks (Cheshvan or Kislev) is kept on the month's last day.
    /// Adar I and II birthdays move to the only Adar in an ordinary year.
    pub fn falls_on(self, today: HebrewDate) -> bool {
        let leap = hebrew::is_leap_year(today.year);
        let month = match self.month {
            Month::Number(n @ 1..=5) => n + 6,
            Month::Number(n) => n - 6,
            Month::Adar | Month::AdarII if leap => ADAR_II,
            Month::AdarI if leap => ADAR,
            Month::Adar | Month::AdarI | Month::AdarII => ADAR,
        };
        month == today.month && self.day.min(hebrew::days_in_month(today.year, month)) == today.day
    }
}

/// Names of the people whose birthday is `today`. Each name is also a subject in
/// `subjects.json`, which holds that person's pictures.
///
/// A missing family file means nobody is celebrated.
///
/// # Errors
/// Fails when the family file cannot be read or is invalid.
pub fn birthday_people(root: &Path, today: HebrewDate) -> Result<Vec<String>> {
    let path = root.join(FAMILY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
    };
    let FamilyFile(people) =
        serde_json::from_str(&text).with_context(|| format!("invalid {}", path.display()))?;
    Ok(people
        .into_iter()
        .filter(|(_, birthday)| birthday.falls_on(today))
        .map(|(n, _)| n)
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
        assert_eq!(bday("1/1").month, Month::Number(1));
        assert_eq!(bday("1/6B").month, Month::AdarII);
        for bad in ["5", "0/5", "31/5", "15/0", "15/13", "15/6c", "x/5", "15 Shevat", ""] {
            assert!(bad.parse::<Birthday>().is_err(), "{bad}");
        }
    }

    #[test]
    fn month_numbers_count_from_tishrei() {
        assert!(bday("27/1").falls_on(on(5787, TISHREI, 27)));
        assert!(bday("15/5").falls_on(on(5786, 11, 15))); // Shevat
        assert!(bday("1/7").falls_on(on(5786, 1, 1))); // Nisan
        assert!(bday("29/12").falls_on(on(5786, 6, 29))); // Elul
    }

    #[test]
    fn adar_birthdays_follow_leap_years() {
        let (leap, plain) = (5784, 5785);
        assert!(bday("14/6").falls_on(on(leap, ADAR_II, 14)));
        assert!(!bday("14/6").falls_on(on(leap, ADAR, 14)));
        assert!(bday("14/6a").falls_on(on(leap, ADAR, 14)));
        assert!(!bday("14/6a").falls_on(on(leap, ADAR_II, 14)));
        assert!(bday("14/6b").falls_on(on(leap, ADAR_II, 14)));
        for text in ["14/6", "14/6a", "14/6b"] {
            assert!(bday(text).falls_on(on(plain, ADAR, 14)), "{text}");
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

    fn root_with(family: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("Config")).unwrap();
        if let Some(text) = family {
            fs::write(dir.path().join(FAMILY_FILE), text).unwrap();
        }
        dir
    }

    #[test]
    fn lists_the_people_whose_birthday_is_today() {
        let dir = root_with(Some(r#"{"Dana": "15/5", "Omer": "15/5", "Noa": "1/7"}"#));

        assert_eq!(birthday_people(dir.path(), on(5786, 11, 15)).unwrap(), ["Dana", "Omer"]);
        assert_eq!(birthday_people(dir.path(), on(5786, 11, 16)).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn missing_family_file_means_no_birthdays_and_bad_one_is_an_error() {
        let dir = root_with(None);
        assert_eq!(birthday_people(dir.path(), on(5786, 11, 15)).unwrap(), Vec::<String>::new());
        for bad in ["{ nope", r#"{"A": "soon"}"#, r#"{"A": {"birthday": "15/5"}}"#] {
            fs::write(dir.path().join(FAMILY_FILE), bad).unwrap();
            assert!(birthday_people(dir.path(), on(5786, 11, 15)).is_err(), "{bad}");
        }
    }
}
