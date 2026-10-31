import json
import os
import sys
import threading
import unittest
import urllib.error
import urllib.request
from datetime import date, datetime, timezone
from http.server import ThreadingHTTPServer

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "api"))
import _booking as b  # noqa: E402
import index  # noqa: E402

NOW = datetime(2026, 11, 1, tzinfo=timezone.utc)
MONDAY = date(2026, 11, 2)
SLOT = "2026-11-02T04:00:00Z"
RULES = b.Rules({})


def utc(text):
    return b.parse_time(text)


class FakeGoogle:
    """Behaves like Google: a second insert of one event id gets 409."""

    def __init__(self, fail=None):
        self.calendar_id = "primary"
        self.events = {}
        self.fail = fail
        self.lock = threading.Lock()

    def busy(self, start, end):
        if self.fail:
            raise b.GoogleError(self.fail)
        return []

    def book(self, start, minutes, name, email, notes):
        with self.lock:
            key = b.event_id(self.calendar_id, start)
            if key in self.events:
                raise b.GoogleError(409)
            self.events[key] = email
        return "https://meet.google.com/abc"


def post(google, **fields):
    data = {"start": SLOT, "name": "Test", "email": "a@example.com", **fields}
    return b.create_booking(google, RULES, json.dumps(data), NOW)


class Availability(unittest.TestCase):
    def test_full_day_in_host_time(self):
        slots = b.free_slots(RULES, MONDAY, [], NOW)
        self.assertEqual(len(slots), 16)
        self.assertEqual(slots[0], utc(SLOT))

    def test_weekend_closed(self):
        self.assertEqual(b.free_slots(RULES, date(2026, 11, 7), [], NOW), [])

    def test_busy_removes_overlaps(self):
        busy = [(utc("2026-11-02T04:15:00Z"), utc("2026-11-02T05:00:00Z"))]
        slots = b.free_slots(RULES, MONDAY, busy, NOW)
        self.assertEqual(len(slots), 14)
        self.assertEqual(slots[0], utc("2026-11-02T05:00:00Z"))

    def test_minimum_notice(self):
        slots = b.free_slots(RULES, MONDAY, [], utc("2026-11-02T04:10:00Z"))
        self.assertEqual(slots[0], utc("2026-11-02T06:30:00Z"))

    def test_far_future_rejected(self):
        self.assertEqual(b.free_slots(RULES, date(2027, 6, 1), [], NOW), [])


class EventTimes(unittest.TestCase):
    def test_all_day_event_blocks_the_day(self):
        tz = RULES.tz
        busy = [(b.parse_event_time({"date": "2026-11-02"}, tz), b.parse_event_time({"date": "2026-11-03"}, tz))]
        self.assertEqual(b.free_slots(RULES, MONDAY, busy, NOW), [])

    def test_timed_event(self):
        self.assertEqual(b.parse_event_time({"dateTime": SLOT}, RULES.tz), utc(SLOT))


class Breakable(unittest.TestCase):
    def test_books_valid_slot(self):
        status, body = post(FakeGoogle())
        self.assertEqual((status, body["ok"]), (200, True))

    def test_tampered_time_rejected(self):
        self.assertEqual(post(FakeGoogle(), start="2026-11-02T04:07:00Z")[0], 409)

    def test_time_without_zone_rejected(self):
        self.assertEqual(post(FakeGoogle(), start="2026-11-02T09:00:00")[0], 400)

    def test_bad_input_rejected(self):
        for raw in ["", "not json", "[]", "null", '{"start":1}', '{"start":"x","name":"a","email":"a@b.co"}']:
            self.assertEqual(b.create_booking(FakeGoogle(), RULES, raw, NOW)[0], 400, raw)

    def test_bad_details_rejected(self):
        for fields in [{"name": " "}, {"name": "x" * 101}, {"notes": "x" * 2001}]:
            self.assertEqual(post(FakeGoogle(), **fields)[0], 400)
        for email in ["", "a", "a@b", "@b.co", "a b@c.co", "a@b@c.co", "a@.co", "a@b.co\nBcc:x"]:
            self.assertEqual(post(FakeGoogle(), email=email)[0], 400, email)

    def test_google_outage_is_clean_error(self):
        status, body = post(FakeGoogle(fail=503))
        self.assertEqual(status, 502)
        self.assertNotIn("503", body["error"])
        self.assertEqual(b.list_slots(FakeGoogle(fail=403), RULES, "2026-11-02", NOW)[0], 502)

    def test_bad_date_rejected(self):
        self.assertEqual(b.list_slots(FakeGoogle(), RULES, "nope", NOW)[0], 400)
        self.assertEqual(b.list_slots(FakeGoogle(), RULES, None, NOW)[0], 400)

    def test_same_slot_cannot_double_book_under_load(self):
        google = FakeGoogle()
        results = []
        threads = [threading.Thread(target=lambda: results.append(post(google)[0])) for _ in range(30)]
        [t.start() for t in threads]
        [t.join() for t in threads]
        self.assertEqual(results.count(200), 1)
        self.assertEqual(results.count(409), 29)
        self.assertEqual(len(google.events), 1)


class Http(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), index.handler)
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()

    def call(self, path, data=None):
        try:
            with urllib.request.urlopen(urllib.request.Request(self.url + path, data=data)) as r:
                return r.status, json.load(r)
        except urllib.error.HTTPError as err:
            return err.code, json.load(err)

    def test_health(self):
        self.assertEqual(self.call("/?health=1"), (200, {"status": "ok"}))

    def test_rejects_oversized_body(self):
        self.assertEqual(self.call("/", b"x" * 9000)[0], 413)

    def test_rejects_garbage_body(self):
        self.assertEqual(self.call("/", b"garbage")[0], 400)


if __name__ == "__main__":
    unittest.main()
