//! Hebrew calendar arithmetic (Gregorian date to Hebrew date), pure and dependency-free.
//!
//! Follows the fixed-day algorithms of "Calendrical Calculations". Days are counted as
//! `NaiveDate::num_days_from_ce`, where 1 January of year 1 is day 1.

use chrono::{Datelike, NaiveDate};

/// Day number of 1 Tishrei of Hebrew year 1.
const EPOCH_DAY: i64 = -1_373_427;

pub const NISAN: u8 = 1;
pub const TISHREI: u8 = 7;
pub const CHESHVAN: u8 = 8;
pub const KISLEV: u8 = 9;
/// Plain Adar in an ordinary year; Adar I in a leap year.
pub const ADAR: u8 = 12;
/// Only exists in leap years.
pub const ADAR_II: u8 = 13;

/// A Hebrew date. Months are numbered from Nisan = 1 (so Tishrei, the new year, is 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HebrewDate {
    pub year: i64,
    pub month: u8,
    pub day: u8,
}

pub fn is_leap_year(year: i64) -> bool {
    (7 * year + 1).rem_euclid(19) < 7
}

fn elapsed_days(year: i64) -> i64 {
    let months = (235 * year - 234).div_euclid(19);
    let parts = 12_084 + 13_753 * months;
    let days = 29 * months + parts.div_euclid(25_920);
    if (3 * (days + 1)).rem_euclid(7) < 3 { days + 1 } else { days }
}

fn new_year_day(year: i64) -> i64 {
    let (before, this, after) =
        (elapsed_days(year - 1), elapsed_days(year), elapsed_days(year + 1));
    let delay = if after - this == 356 { 2 } else { i64::from(this - before == 382) };
    EPOCH_DAY + this + delay
}

fn days_in_year(year: i64) -> i64 {
    new_year_day(year + 1) - new_year_day(year)
}

fn last_month(year: i64) -> u8 {
    if is_leap_year(year) { ADAR_II } else { ADAR }
}

/// Number of days (29 or 30) in `month` of `year`.
pub fn days_in_month(year: i64, month: u8) -> u8 {
    let short = match month {
        2 | 4 | 6 | 10 | ADAR_II => true,
        ADAR => !is_leap_year(year),
        CHESHVAN => days_in_year(year) % 10 != 5,
        KISLEV => days_in_year(year) % 10 == 3,
        _ => false,
    };
    if short { 29 } else { 30 }
}

/// Day number of the first day of `month` in `year`.
fn month_start(year: i64, month: u8) -> i64 {
    let (from, to) =
        if month < TISHREI { (TISHREI, last_month(year) + 1) } else { (TISHREI, month) };
    let mut day = new_year_day(year);
    for m in from..to {
        day += i64::from(days_in_month(year, m));
    }
    if month < TISHREI {
        for m in NISAN..month {
            day += i64::from(days_in_month(year, m));
        }
    }
    day
}

pub fn from_gregorian(date: NaiveDate) -> HebrewDate {
    let day = i64::from(date.num_days_from_ce());
    let mut year = (day - EPOCH_DAY) * 98_496 / 35_975_351;
    while new_year_day(year + 1) <= day {
        year += 1;
    }
    while new_year_day(year) > day {
        year -= 1;
    }
    let mut month = if day < month_start(year, NISAN) { TISHREI } else { NISAN };
    while day >= month_start(year, month) + i64::from(days_in_month(year, month)) {
        month = if month == last_month(year) { NISAN } else { month + 1 };
    }
    HebrewDate { year, month, day: (day - month_start(year, month) + 1) as u8 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hebrew(y: i32, m: u32, d: u32) -> (i64, u8, u8) {
        let h = from_gregorian(NaiveDate::from_ymd_opt(y, m, d).unwrap());
        (h.year, h.month, h.day)
    }

    #[test]
    fn rosh_hashana_dates() {
        assert_eq!(hebrew(2024, 10, 3), (5785, TISHREI, 1));
        assert_eq!(hebrew(2025, 9, 23), (5786, TISHREI, 1));
        assert_eq!(hebrew(2026, 9, 12), (5787, TISHREI, 1));
        assert_eq!(hebrew(2026, 9, 11), (5786, 6, 29));
    }

    #[test]
    fn leap_year_adar_ii_and_passover() {
        assert!(is_leap_year(5784));
        assert_eq!(hebrew(2024, 3, 24), (5784, ADAR_II, 14));
        assert_eq!(hebrew(2024, 2, 10), (5784, ADAR, 1));
        assert_eq!(hebrew(2024, 4, 23), (5784, NISAN, 15));
        assert_eq!(hebrew(2025, 3, 14), (5785, ADAR, 14));
    }

    #[test]
    fn year_boundary_and_modern_dates() {
        assert_eq!(hebrew(1948, 5, 14), (5708, 2, 5)); // 5 Iyar, Israeli independence
        assert_eq!(hebrew(2000, 1, 1), (5760, 10, 23));
    }

    #[test]
    fn every_day_round_trips_through_consecutive_hebrew_dates() {
        let mut date = NaiveDate::from_ymd_opt(2020, 1, 1).unwrap();
        let mut prev = from_gregorian(date);
        for _ in 0..365 * 12 {
            date = date.succ_opt().unwrap();
            let next = from_gregorian(date);
            let rolled = next.day == 1 && prev.day == days_in_month(prev.year, prev.month);
            assert!(next.day == prev.day + 1 && next.month == prev.month || rolled, "{date}");
            prev = next;
        }
    }
}
