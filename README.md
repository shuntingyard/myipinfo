# myipinfo

Emulate some of the behaviour of `curl ipinfo.io/<ipaddr>` using Rust's [geoip2](https://crates.io/crates/geoip2) crate.

## Build

Clone this repo, make sure you have a working Rust installation and either run
```bash
cargo build --release

```
or make sure Docker is working on your machine, add [cross](https://github.com/cross-rs/cross) to your Rust installation and possibly use the `cross.sh` script.

## TODO

- [ ] Calculate zoom optimum for OpenStreetMap URL using MaxMind’s accuracy radius.
- [ ] Investigate the interesting bug on `armv7-unknown-linux-gnueabihf`.
- [ ] Treat cases with no ASN entry, e.g. `1.2.3.4`
