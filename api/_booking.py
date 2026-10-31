"""Booking logic. Availability rules are pure; Google calls are injected."""

import hashlib
import json
import os
import re
import urllib.error
import urllib.parse
import urllib.request
from datetime import date, datetime, time, timedelta, timezone
from zoneinfo import ZoneInfo

CALENDAR_API = "https://www.googleapis.com/calendar/v3"
EMAIL = re.compile(r"^[^@\s]+@[^@\s.]+(\.[^@\s.]+)+$")


class Rules:
    def __init__(self, env=os.environ):
        self.tz = ZoneInfo(env.get("HOST_TIMEZONE", "Asia/Karachi"))
        self.start_hour = int(env.get("WORK_START_HOUR", 9))
        self.end_hour = int(env.get("WORK_END_HOUR", 17))
        self.minutes = int(env.get("MEETING_MINUTES", 30))
        self.notice_hours = int(env.get("MIN_NOTICE_HOURS", 2))
        self.max_days = int(env.get("MAX_DAYS_AHEAD", 60))


def day_bounds(rules, day):
    if day.weekday() >= 5:
        return None
    at = lambda hour: datetime.combine(day, time(hour), rules.tz).astimezone(timezone.utc)
    return at(rules.start_hour), at(rules.end_hour)


def free_slots(rules, day, busy, now):
    bounds = day_bounds(rules, day)
    if not bounds:
        return []
    step = timedelta(minutes=rules.minutes)
    earliest = now + timedelta(hours=rules.notice_hours)
    latest = now + timedelta(days=rules.max_days)
    slots, start = [], bounds[0]
    while start + step <= bounds[1]:
        end = start + step
        clash = any(start < b_end and b_start < end for b_start, b_end in busy)
        if earliest <= start <= latest and not clash:
            slots.append(start)
        start = end
    return slots


def is_offered(rules, start, busy, now):
    return start in free_slots(rules, start.astimezone(rules.tz).date(), busy, now)


def event_id(calendar_id, start):
    """Same slot, same id: Google rejects a second insert with 409."""
    return hashlib.sha256(f"{calendar_id}|{start.isoformat()}".encode()).hexdigest()[:40]


def iso(value):
    return value.astimezone(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def parse_time(value):
    parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("time zone required")
    return parsed.astimezone(timezone.utc)


class GoogleError(Exception):
    def __init__(self, status):
        super().__init__(f"Google returned {status}")
        self.status = status


class Google:
    def __init__(self, env=os.environ):
        self.env = env
        self.calendar_id = env.get("GOOGLE_CALENDAR_ID", "primary")

    def _request(self, url, data=None, headers=None):
        request = urllib.request.Request(url, data=data, headers=headers or {})
        try:
            with urllib.request.urlopen(request, timeout=8) as response:
                return json.load(response)
        except urllib.error.HTTPError as err:
            raise GoogleError(err.code) from None
        except (urllib.error.URLError, TimeoutError):
            raise GoogleError(503) from None

    def _headers(self):
        form = urllib.parse.urlencode({
            "client_id": self.env["GOOGLE_CLIENT_ID"],
            "client_secret": self.env["GOOGLE_CLIENT_SECRET"],
            "refresh_token": self.env["GOOGLE_REFRESH_TOKEN"],
            "grant_type": "refresh_token",
        }).encode()
        token = self._request("https://oauth2.googleapis.com/token", form)["access_token"]
        return {"Authorization": f"Bearer {token}", "Content-Type": "application/json"}

    def busy(self, start, end):
        body = {"timeMin": iso(start), "timeMax": iso(end), "items": [{"id": self.calendar_id}]}
        result = self._request(f"{CALENDAR_API}/freeBusy", json.dumps(body).encode(), self._headers())
        calendar = result["calendars"][self.calendar_id]
        if calendar.get("errors"):
            raise GoogleError(502)
        return [(parse_time(b["start"]), parse_time(b["end"])) for b in calendar.get("busy", [])]

    def book(self, start, minutes, name, email, notes):
        body = {
            "id": event_id(self.calendar_id, start),
            "summary": f"Meeting with {name}",
            "description": notes,
            "start": {"dateTime": iso(start)},
            "end": {"dateTime": iso(start + timedelta(minutes=minutes))},
            "attendees": [{"email": email, "displayName": name}],
            "conferenceData": {"createRequest": {
                "requestId": body_request_id(start),
                "conferenceSolutionKey": {"type": "hangoutsMeet"},
            }},
        }
        calendar = urllib.parse.quote(self.calendar_id, safe="")
        url = f"{CALENDAR_API}/calendars/{calendar}/events?conferenceDataVersion=1&sendUpdates=all"
        return self._request(url, json.dumps(body).encode(), self._headers()).get("hangoutLink", "")


def body_request_id(start):
    return f"booking-{int(start.timestamp())}"


def list_slots(google, rules, raw_date, now):
    try:
        day = date.fromisoformat(raw_date)
    except (TypeError, ValueError):
        return 400, {"error": "Invalid date"}
    bounds = day_bounds(rules, day)
    if not bounds:
        return 200, {"slots": []}
    try:
        busy = google.busy(*bounds)
    except GoogleError:
        return 502, {"error": "Could not load times"}
    return 200, {"slots": [iso(s) for s in free_slots(rules, day, busy, now)]}


def create_booking(google, rules, raw_body, now):
    try:
        data = json.loads(raw_body)
        start = parse_time(data["start"])
        name = str(data["name"]).strip()
        email = str(data["email"]).strip()
        notes = str(data.get("notes", ""))
    except (KeyError, TypeError, ValueError, AttributeError):
        return 400, {"ok": False, "error": "Invalid request"}
    if not name or len(name) > 100 or len(notes) > 2000 or len(email) > 254 or not EMAIL.match(email):
        return 400, {"ok": False, "error": "Please check your details"}
    end = start + timedelta(minutes=rules.minutes)
    try:
        if not is_offered(rules, start, google.busy(start, end), now):
            return 409, {"ok": False, "error": "That time is no longer available"}
        link = google.book(start, rules.minutes, name, email, notes)
    except GoogleError as err:
        if err.status == 409:
            return 409, {"ok": False, "error": "That time is no longer available"}
        return 502, {"ok": False, "error": "Please try again"}
    return 200, {"ok": True, "meetLink": link}
