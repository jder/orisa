FROM rust:1-slim-buster as builder
WORKDIR /usr/src/orisa
COPY . .
RUN cargo install --path .

FROM debian:buster-slim
RUN apt-get update && apt-get install -y openssl ca-certificates extra-runtime-dependencies
COPY --from=builder /usr/local/cargo/bin/orisa /usr/local/bin/orisa
CMD ["orisa"]
