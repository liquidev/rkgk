port := "8080"

serve wasm_profile="wasm-dev": (wasm wasm_profile)
    RKGK_PORT={{port}} cargo run -p rkgk

wasm profile="wasm-dev":
    cargo build -p haku-wasm --target wasm32-unknown-unknown --profile {{profile}}

deploy:
    bash admin/deploy.bash
