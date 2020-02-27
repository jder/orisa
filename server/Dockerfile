FROM rust:1.41 as builder
WORKDIR /usr/src/orisa
COPY . .
RUN cargo install --path .

FROM debian:buster
COPY --from=builder /usr/local/cargo/bin/orisa /usr/local/bin/orisa
CMD ["orisa"]
