FROM rust:1.56-slim-bullseye as builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev
WORKDIR /usr/src/orisa
COPY . .
RUN cargo install --path .

FROM debian:bullseye-slim
RUN apt-get update && apt-get install -y openssl ca-certificates 
COPY --from=builder /usr/local/cargo/bin/orisa /usr/local/bin/orisa
CMD ["orisa"]
