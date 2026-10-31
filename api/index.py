"""Vercel entry point: GET /api?date=, POST /api, GET /api?health=1."""

import json
import os
import sys
from datetime import datetime, timezone
from http.server import BaseHTTPRequestHandler
from urllib.parse import parse_qs, urlparse

sys.path.insert(0, os.path.dirname(__file__))
import _booking  # noqa: E402

MAX_BODY = 8 * 1024


class handler(BaseHTTPRequestHandler):
    def _send(self, status, payload):
        body = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Cache-Control", "no-store")
        origin = os.environ.get("ALLOWED_ORIGIN")
        if origin:
            self.send_header("Access-Control-Allow-Origin", origin)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        query = parse_qs(urlparse(self.path).query)
        if "health" in query:
            return self._send(200, {"status": "ok"})
        now = datetime.now(timezone.utc)
        status, payload = _booking.list_slots(
            _booking.Google(), _booking.Rules(), query.get("date", [None])[0], now
        )
        self._send(status, payload)

    def do_POST(self):
        try:
            length = int(self.headers.get("Content-Length", 0))
        except ValueError:
            length = -1
        if length < 0 or length > MAX_BODY:
            return self._send(413, {"ok": False, "error": "Request too large"})
        body = self.rfile.read(length)
        now = datetime.now(timezone.utc)
        status, payload = _booking.create_booking(_booking.Google(), _booking.Rules(), body, now)
        self._send(status, payload)

    def log_message(self, *args):
        pass  # guest details must never reach the logs
