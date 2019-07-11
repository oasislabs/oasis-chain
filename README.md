# oasis-chain

A simulated Oasis blockchain for local testing.

## Build/install
```
$ git clone https://github.com/oasislabs/oasis-chain
$ cd oasis-chain
$ cargo install --path . --debug
```
or
```
$ RUSTFLAGS='-C target-feature=+aes,+ssse3' cargo install --git https://github.com/oasislabs/oasis-chain --debug
```

## Run
```
$ oasis-chain
2019-07-09 12:56:47,578 INFO  [ws] Listening for new connections on 127.0.0.1:8546.
2019-07-09 12:56:47,579 INFO  [oasis_chain] Oasis local chain is running
```
