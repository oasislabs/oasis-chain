FROM ubuntu:bionic

ENV DEBIAN_FRONTEND=noninteractive
ENV RUSTUP_HOME=/usr/local/lib/rustup
ENV CARGO_HOME=/usr/local/lib/cargo

EXPOSE 8546/tcp

RUN \
 apt-get update -q -q && \
 apt-get install --yes tzdata curl ca-certificates git build-essential && \
 echo 'UTC' > /etc/timezone && \
 rm /etc/localtime && \
 dpkg-reconfigure tzdata && \
 apt-get upgrade --yes && \
 adduser --system --group oasis --home /nonexistent && \
 curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --no-modify-path --default-toolchain nightly && \
 RUSTFLAGS='-C target-feature=+aes,+ssse3' /usr/local/lib/cargo/bin/cargo install --locked --git https://github.com/oasislabs/oasis-chain.git oasis-chain && \
 cp /usr/local/lib/cargo/bin/oasis-chain /usr/local/bin/oasis-chain && \
 /usr/local/lib/cargo/bin/rustup self uninstall -y && \
 apt-get remove --purge --yes curl ca-certificates git build-essential && \
 apt autoremove --yes && \
 apt-get clean && rm -rf /var/lib/apt/lists/* /tmp/* /var/tmp/*

USER oasis

ENTRYPOINT ["/usr/local/bin/oasis-chain", "--interface", "0.0.0.0"]
