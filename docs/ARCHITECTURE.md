# Architecture

- `web/book.html`: dependency-free page with the calendar and clock picker.
- `react/BookingWidget.tsx`: optional React version of the booking flow.
- `src/main.rs`: routes, validation, rate limit, booking lock.
- `src/slots.rs`: pure availability rules, unit tested.
- `src/google.rs`: OAuth refresh, freeBusy, events.insert.
- `src/circuit.rs`: circuit breaker around Google.

The server owns duration, working hours and notice. Times from the browser are treated as requests and checked again before booking. The booking lock is in memory, so run one instance only.
