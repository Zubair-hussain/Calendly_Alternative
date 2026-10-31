FROM rust:1-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --locked

FROM gcr.io/distroless/cc-debian12:nonroot
COPY /app/target/release/calendly-booking /server
ENV PORT=8080
ENTRYPOINT ["/server"]
