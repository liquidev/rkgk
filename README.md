# rakugaki - digital multiplayer graffiti

rakugaki is a multiplayer paint canvas with programmable brushes!

At the heart of rakugaki is the _brush_ - a little program for manipulating pixels on a _wall_.
Brushes are written in a tiny programming language called _haku_.

## I wanna try it out!

Since the app is currently in very early alpha stages, there's no public instance at the moment.

You're free to spin up a server for your friends though!
Here's the setup procedure for production instances.

```sh
# As of writing this, 1.81 is not yet released, so we have to use Rust nightly.
rustup toolchain install nightly-2024-08-11
rustup default nightly-2024-08-11
rustup target add wasm32-unknown-unknown

# We use `just` to wrangle the process of building the client-side WebAssembly and the server.
cargo install just

# Now it's time to run
just port=8080 profile=release
```

For development, I recommend using `cargo watch` for live reloading.
`just` defaults to a development configuration.

```sh
cargo install cargo-watch
cargo watch -- just
```

