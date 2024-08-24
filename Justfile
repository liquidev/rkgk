port := "8080"
profile := "dev"
wasm_profile := "wasm-" + profile

serve: wasm
    RKGK_PORT={{port}} RKGK_WASM_PATH=target/wasm32-unknown-unknown/{{wasm_profile}} cargo run -p rkgk --profile {{profile}}

wasm:
    cargo build -p haku-wasm --target wasm32-unknown-unknown --profile {{wasm_profile}}

deploy:
    bash admin/deploy.bash
