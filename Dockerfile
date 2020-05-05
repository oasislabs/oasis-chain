FROM rust:1.41.0 as oasis-chain-builder

COPY . oasis-chain/
WORKDIR oasis-chain

RUN cargo build --release --locked

FROM gcr.io/distroless/cc

COPY --from=oasis-chain-builder /oasis-chain/target/release/oasis-chain /

EXPOSE 8546/tcp

ENTRYPOINT ["/oasis-chain", "--interface", "0.0.0.0"]
