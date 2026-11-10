//! Build script: re-run the build when a migration file changes, because
//! `sqlx::migrate!` embeds the SQL files at compile time.

fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
