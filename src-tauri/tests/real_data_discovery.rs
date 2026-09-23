//! Read-only check against this machine's real Claude data. Run manually:
//! `cargo test --test real_data_discovery -- --ignored --nocapture`

use session_relay_lib::engine::links::{DirRole, Links};

#[test]
#[ignore = "reads the developer's real ~/.claude/projects"]
fn classify_real_project_dirs() {
    let home = std::env::var("CLAUDE_CONFIG_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| {
        std::path::PathBuf::from(std::env::var("USERPROFILE").expect("USERPROFILE")).join(".claude")
    });
    let projects = home.join("projects");
    let mut links = Links::default();
    let (mut primary, mut secondary, mut none) = (0, 0, 0);
    let started = std::time::Instant::now();
    for entry in std::fs::read_dir(&projects).unwrap().flatten().filter(|e| e.path().is_dir()) {
        let enc = entry.file_name().to_string_lossy().into_owned();
        match links.classify_dir(&projects, &enc) {
            DirRole::Primary(key) => {
                primary += 1;
                println!("PRIMARY   {enc} -> {} [{}]", key.remote, key.subpath);
            }
            DirRole::Secondary(key) => {
                secondary += 1;
                println!("SECONDARY {enc} -> {}", key.remote);
            }
            DirRole::NoRemote => {
                none += 1;
                println!("NO_REMOTE {enc}");
            }
        }
    }
    println!("primary={primary} secondary={secondary} no_remote={none} in {:?}", started.elapsed());
}
