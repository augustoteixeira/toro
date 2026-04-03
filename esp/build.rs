use std::collections::HashMap;

fn main() {
    embuild::espidf::sysenv::output();

    // Read cfg.toml and emit its [toro] values as CFG_TORO_* compile-time env vars.
    let cfg_toml = std::fs::read_to_string("cfg.toml").expect(
        "cfg.toml not found — copy cfg.toml.example and fill in your values",
    );

    let parsed: HashMap<String, HashMap<String, String>> =
        toml::from_str(&cfg_toml).expect("failed to parse cfg.toml");

    if let Some(toro) = parsed.get("toro") {
        for (key, value) in toro {
            let var = format!("CFG_TORO_{}", key.to_uppercase());
            println!("cargo:rustc-env={var}={value}");
        }
    }

    println!("cargo:rerun-if-changed=cfg.toml");
}
