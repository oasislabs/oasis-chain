fn main() {
    let envs: std::collections::HashMap<String, String> = std::env::vars().collect();
    let mut rustflags = envs.entry("RUSTFLAGS".to_string()).or_default();
    *rustflags = format!("{} -Ctarget-feature=+aes,+ssse3", rustflags);
    std::process::Command::new("cargo")
        .args(&["install", "--force", "--git", "https://github.com/oasislabs/oasis-chain"])
        .envs(envs)
        .output()
        .unwrap();
}
