FROM rust:1.41 as builder
WORKDIR /usr/src/orisa
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y openssl
COPY --from=builder /usr/local/cargo/bin/orisa /usr/local/bin/orisa
CMD ["orisa"]
