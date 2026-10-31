//! Pure availability logic. No I/O, so every rule here is unit tested.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};
use chrono_tz::Tz;

#[derive(Clone, Debug)]
pub struct Rules {
    pub tz: Tz,
    pub start_hour: u32,
    pub end_hour: u32,
    pub duration_min: i64,
    pub min_notice_hours: i64,
    pub max_days_ahead: i64,
}

#[derive(Clone, Copy, Debug)]
pub struct Busy {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// UTC bounds of a host-local working day, or None for weekends and
/// local times that do not exist.
pub fn day_bounds(rules: &Rules, date: NaiveDate) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    if matches!(date.weekday(), Weekday::Sat | Weekday::Sun) {
        return None;
    }
    let at = |hour| {
        let time = NaiveTime::from_hms_opt(hour, 0, 0)?;
        rules
            .tz
            .from_local_datetime(&date.and_time(time))
            .earliest()
            .map(|d| d.with_timezone(&Utc))
    };
    Some((at(rules.start_hour)?, at(rules.end_hour)?))
}

pub fn free_slots(
    rules: &Rules,
    date: NaiveDate,
    busy: &[Busy],
    now: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let Some((open, close)) = day_bounds(rules, date) else {
        return Vec::new();
    };
    let step = Duration::minutes(rules.duration_min);
    let earliest = now + Duration::hours(rules.min_notice_hours);
    let latest = now + Duration::days(rules.max_days_ahead);
    let mut out = Vec::new();
    let mut start = open;
    while start + step <= close {
        let end = start + step;
        let clash = busy.iter().any(|b| start < b.end && b.start < end);
        if start >= earliest && start <= latest && !clash {
            out.push(start);
        }
        start = end;
    }
    out
}

/// True when `start` is exactly one of the slots offered for its day.
pub fn is_offered(rules: &Rules, start: DateTime<Utc>, busy: &[Busy], now: DateTime<Utc>) -> bool {
    let date = start.with_timezone(&rules.tz).date_naive();
    free_slots(rules, date, busy, now).contains(&start)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Rules {
        Rules {
            tz: chrono_tz::Asia::Karachi,
            start_hour: 9,
            end_hour: 17,
            duration_min: 30,
            min_notice_hours: 2,
            max_days_ahead: 60,
        }
    }
    fn utc(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }
    const MONDAY: &str = "2026-11-02";
    const NOW: &str = "2026-11-01T00:00:00Z";

    #[test]
    fn full_day_has_sixteen_slots_in_host_time() {
        let d = MONDAY.parse().unwrap();
        let s = free_slots(&rules(), d, &[], utc(NOW));
        assert_eq!(s.len(), 16);
        assert_eq!(s[0], utc("2026-11-02T04:00:00Z")); // 09:00 PKT
    }

    #[test]
    fn weekends_are_closed() {
        let d = "2026-11-07".parse().unwrap();
        assert!(free_slots(&rules(), d, &[], utc(NOW)).is_empty());
    }

    #[test]
    fn busy_removes_every_overlapping_slot_only() {
        let d = MONDAY.parse().unwrap();
        let busy = [Busy {
            start: utc("2026-11-02T04:15:00Z"),
            end: utc("2026-11-02T05:00:00Z"),
        }];
        let s = free_slots(&rules(), d, &busy, utc(NOW));
        assert_eq!(s.len(), 14);
        assert_eq!(s[0], utc("2026-11-02T05:00:00Z"));
    }

    #[test]
    fn minimum_notice_is_enforced() {
        let d = MONDAY.parse().unwrap();
        let s = free_slots(&rules(), d, &[], utc("2026-11-02T04:10:00Z"));
        assert_eq!(s[0], utc("2026-11-02T06:30:00Z"));
    }

    #[test]
    fn far_future_is_rejected() {
        let d = "2027-06-01".parse().unwrap();
        assert!(free_slots(&rules(), d, &[], utc(NOW)).is_empty());
    }

    #[test]
    fn tampered_start_time_is_not_offered() {
        assert!(is_offered(
            &rules(),
            utc("2026-11-02T04:30:00Z"),
            &[],
            utc(NOW)
        ));
        assert!(!is_offered(
            &rules(),
            utc("2026-11-02T04:07:00Z"),
            &[],
            utc(NOW)
        ));
    }
}
