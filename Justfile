serve wasm_profile="wasm-dev": (wasm wasm_profile)
    cargo run -p canvane

wasm profile="wasm-dev":
    cargo build -p haku-wasm --target wasm32-unknown-unknown --profile {{profile}}
