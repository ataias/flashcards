fn main() {
    println!("cargo:rerun-if-env-changed=FLASHCARDS_GIT_SHA");
    println!("cargo:rerun-if-env-changed=GITHUB_SHA");
    println!("cargo:rerun-if-env-changed=FLASHCARDS_GIT_TAG");

    let sha = first_nonempty(&["FLASHCARDS_GIT_SHA", "GITHUB_SHA"])
        .filter(|sha| valid_git_sha(sha))
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=FLASHCARDS_GIT_SHA={sha}");

    if let Some(tag) = std::env::var("FLASHCARDS_GIT_TAG")
        .ok()
        .filter(|tag| valid_git_tag(tag))
    {
        println!("cargo:rustc-env=FLASHCARDS_GIT_TAG={tag}");
    }
}

fn first_nonempty(keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
}

fn valid_git_sha(sha: &str) -> bool {
    let len = sha.len();
    (7..=64).contains(&len) && sha.bytes().all(|b| b.is_ascii_hexdigit())
}

fn valid_git_tag(tag: &str) -> bool {
    let bytes = tag.as_bytes();
    !bytes.is_empty()
        && bytes[0] != b'/'
        && bytes.last() != Some(&b'/')
        && !tag.contains("..")
        && bytes
            .iter()
            .all(|&b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
}
