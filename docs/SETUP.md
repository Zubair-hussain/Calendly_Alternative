# Booking service

Rust API behind the booking calendar on zubairhussain.xyz. It reads free times from Google Calendar, re-checks the slot, then creates the event with a Google Meet link and emails the guest.

| Route | Purpose |
|---|---|
| `GET /?date=YYYY-MM-DD` | `{ "slots": [...] }` |
| `POST /` | `{ "ok": true, "meetLink": "..." }` |
| `GET /healthz` | Uptime check |

## Safety rules

- The server decides the duration and hours. A tampered time is rejected.
- A lock plus a fresh freeBusy check blocks double booking. This needs **one instance**, so keep `--max-instances=1`.
- 5 booking attempts per IP per hour. Guest details are never logged.

## 1. Google setup

Install the [gcloud CLI](https://cloud.google.com/sdk/docs/install), then:

```bash
gcloud auth login
gcloud projects create zh-booking-2026 --name="ZH Booking"
gcloud config set project zh-booking-2026
gcloud services enable calendar-json.googleapis.com run.googleapis.com artifactregistry.googleapis.com cloudbuild.googleapis.com secretmanager.googleapis.com
```

Link billing (Cloud Run requires it, even within the free tier):

```bash
gcloud billing accounts list
gcloud billing projects link zh-booking-2026 --billing-account=YOUR_ACCOUNT_ID
```

The OAuth client has no gcloud command, so use the console:

1. **Google Auth Platform > Branding**: app name, your email. Audience: **External**.
2. **Audience**: click **Publish app**. In Testing mode refresh tokens expire after 7 days and bookings would stop.
3. **Data Access**: add scope `https://www.googleapis.com/auth/calendar`.
4. **Clients > Create client > Web application**. Add redirect URI `https://developers.google.com/oauthplayground`. Copy the ID and secret.

Get the refresh token:

1. Open https://developers.google.com/oauthplayground
2. Gear icon: tick **Use your own OAuth credentials** and paste them.
3. Enter scope `https://www.googleapis.com/auth/calendar`, then **Authorize APIs**, sign in with the calendar's account, and accept the unverified-app warning for your own account.
4. **Exchange authorization code for tokens**. Copy the refresh token.

## 2. Run locally

```bash
cp .env.example .env
cargo test
cargo run
curl "http://localhost:8080/?date=2026-11-02"
```

## 3. Deploy to Cloud Run

```bash
printf '%s' 'CLIENT_SECRET' | gcloud secrets create google-client-secret --data-file=-
printf '%s' 'REFRESH_TOKEN' | gcloud secrets create google-refresh-token --data-file=-
PROJECT_NUMBER=$(gcloud projects describe zh-booking-2026 --format='value(projectNumber)')
gcloud projects add-iam-policy-binding zh-booking-2026 --member="serviceAccount:${PROJECT_NUMBER}-compute@developer.gserviceaccount.com" --role=roles/secretmanager.secretAccessor
gcloud run deploy zh-booking --source . --region us-central1 --allow-unauthenticated --max-instances=1 --memory=256Mi --set-env-vars="GOOGLE_CLIENT_ID=CLIENT_ID,ALLOWED_ORIGIN=https://zubairhussain.xyz" --set-secrets="GOOGLE_CLIENT_SECRET=google-client-secret:latest,GOOGLE_REFRESH_TOKEN=google-refresh-token:latest"
```

Put the printed URL in Cloudflare as `NEXT_PUBLIC_BOOKING_API_URL`, add that URL to `connect-src` in the portfolio's `next.config.mjs`, and redeploy. The variable is read at build time.

## 4. GitHub

Add the `ANTHROPIC_API_KEY` and `HEALTHCHECK_URL` repository secrets. Then CI, dependency audit, secret scanning, Dependabot and the Claude audit run automatically.

## Keep it alive

- Add a Cloud Monitoring uptime check on `/healthz`, plus an alert for error logs containing `failed`.
- If `freeBusy failed` appears with status 400, the refresh token was revoked. Repeat the token step and update the secret.
- Do one real test booking after every deploy.
