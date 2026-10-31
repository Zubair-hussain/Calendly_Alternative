# When something breaks

| Symptom | Likely cause | Fix |
|---|---|---|
| `503 Booking is temporarily unavailable` | Circuit open after 5 Google failures | Wait 60 seconds. `/healthz` shows `"google":"down"` until a trial call succeeds. If it stays down, check the logs |
| Logs show status 400 from Google | Refresh token revoked or expired | Recreate the token (see SETUP.md), update the secret, and confirm the OAuth app is published |
| Logs show status 403 | Calendar API off or scope missing | Enable the API and re-authorize with the calendar scope |
| `409 no longer available` | Slot taken, or time tampered with | Expected. The guest picks another time |
| `429` | 5 attempts per IP per hour reached | Expected. Resets within the hour |
| Browser CORS error | `ALLOWED_ORIGIN` mismatch | Set it to the exact site origin |
| Host sleeps | Free-tier idle stop | Set the `HEALTHCHECK_URL` secret so Keep Alive pings about every 20 minutes |
