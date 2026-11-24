//! The Mac app's update notice: at start (as an installed app), ask GitHub
//! for the latest release; when it's newer than this one, show its changes
//! in a notice (once per version). ▶ on the notice opens the download page.

use std::path::Path;
use std::sync::{Arc, Mutex};

use reader_core::app::Notice;
use reader_core::App;

pub const RELEASES: &str = "https://github.com/aallevi/fastreader/releases/latest";
const API: &str = "https://api.github.com/repos/aallevi/fastreader/releases/latest";
/// The last version announced, so each is shown once.
const SEEN: &str = "target/update-announced";

/// "v0.2.0" newer than "0.1.0"? Compares the numbers, part by part.
pub fn newer(current: &str, tag: &str) -> bool {
    let parts = |v: &str| v.trim_start_matches('v').split('.').map(|p| p.parse::<u32>().unwrap_or(0)).collect::<Vec<_>>();
    parts(tag) > parts(current)
}

/// A JSON string field ("tag_name", "body") from the release answer.
pub fn field(json: &str, name: &str) -> Option<String> {
    let key = format!("\"{name}\"");
    let at = json.find(&key)? + key.len();
    let rest = json[at..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                'r' => {}
                't' => out.push('\t'),
                'u' => {
                    let hex: String = chars.by_ref().take(4).collect();
                    out.push(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32).unwrap_or('?'));
                }
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// The changes to list: the notes' bullet points (markdown removed), or
/// their first lines when there are none; at most `max`.
pub fn change_lines(body: &str, max: usize) -> Vec<String> {
    let plain = |s: &str| {
        let mut t = s.replace("**", "").replace('`', "");
        // [text](url) -> text
        while let (Some(a), Some(b)) = (t.find("]("), t.find('[')) {
            let Some(c) = t[a..].find(')') else { break };
            if b > a {
                break;
            }
            t = format!("{}{}{}", &t[..b], &t[b + 1..a], &t[a + c + 1..]);
        }
        t.trim().to_string()
    };
    let bullets: Vec<String> = body.lines().filter_map(|l| l.trim_start().strip_prefix("- ").or_else(|| l.trim_start().strip_prefix("* "))).map(plain).filter(|s| !s.is_empty()).take(max).collect();
    if !bullets.is_empty() {
        return bullets;
    }
    body.lines().map(plain).filter(|s| !s.is_empty()).take(max).collect()
}

/// Asks in the background; a newer release not announced yet becomes the
/// app's notice. No network, no answer: nothing happens.
pub fn check(app: Arc<Mutex<App>>) {
    std::thread::spawn(move || {
        let Ok(out) = std::process::Command::new("curl").args(["-s", "-m", "8", "-H", "Accept: application/vnd.github+json", API]).output() else { return };
        let json = String::from_utf8_lossy(&out.stdout);
        let Some(tag) = field(&json, "tag_name") else { return };
        if !newer(env!("CARGO_PKG_VERSION"), &tag) || std::fs::read_to_string(SEEN).is_ok_and(|s| s.trim() == tag) {
            return;
        }
        let body = field(&json, "body").unwrap_or_default();
        let version = tag.trim_start_matches('v');
        app.lock().unwrap().notice = Some(Notice { title: format!("Fast Reader {version} is out"), lines: change_lines(&body, 6) });
        let _ = std::fs::write(Path::new(SEEN), &tag);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.1.0", "v0.2.0"));
        assert!(newer("0.9.0", "v0.10.0"), "by number, not as text");
        assert!(!newer("0.2.0", "v0.2.0"));
        assert!(!newer("0.3.0", "v0.2.9"));
    }

    #[test]
    fn reads_the_release_answer() {
        let json = r#"{"url":"x","tag_name":"v0.2.0","name":"Fast Reader 0.2.0","body":"New:\n- **Figure references** show the figure\n- Tables below the word, see [README](https://x/y)\n- Sleep timer è qui"}"#;
        assert_eq!(field(json, "tag_name").as_deref(), Some("v0.2.0"));
        let lines = change_lines(&field(json, "body").unwrap(), 6);
        assert_eq!(lines, ["Figure references show the figure", "Tables below the word, see README", "Sleep timer è qui"]);
        assert_eq!(change_lines("Just one line of notes.", 6), ["Just one line of notes."]);
    }
}
