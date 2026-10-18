#Build stage
FROM rust:1.99-bookworm AS builder

WORKDIR /app

COPY . .

RUN cargo build --release

# Production stage
FROM debian:bookworm-slim

WORKDIR /usr/local/bin

COPY --from=builder /app/target/release/rust-crud-api . 

RUN useradd --system --no-create-home appuser 

CMD ["./rust-crud-api"]