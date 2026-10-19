//! Platform-independent half of web applet views: page path, virtual host,
//! URL policy and the messages exchanged with the page. The daemon hosts the
//! WebView2 itself.
use illium_theme::Theme;
use serde_json::{Value, json};

/// Runs before any page script and defines `window.illium`.
pub const SCRIPT: &str = include_str!("web.js");
/// Runs before a remote site's scripts when its manifest declares `style`.
const STYLE_SCRIPT: &str = include_str!("style.js");
/// `STYLE_SCRIPT` carrying the applet's style sheet.
pub fn style_script(css: &str) -> String {
    let literal = serde_json::to_string(css).unwrap_or_default();
    STYLE_SCRIPT.replace("'__ILLIUM_CSS__'", &literal)
}
/// Longest action argument a page may send.
pub const MAX_ACTION_BYTES: usize = 4096;

/// A relative `.html`/`.htm` path whose components are URL-safe names, so it
/// can be appended to the virtual host without escaping.
pub fn page(path: &str) -> bool {
    inside(path, &["html", "htm"])
}
/// A relative `.css` path inside the applet folder.
pub fn stylesheet(path: &str) -> bool {
    inside(path, &["css"])
}
fn inside(path: &str, extensions: &[&str]) -> bool {
    let parts: Vec<&str> = path.split(['/', '\\']).collect();
    parts.len() <= 16
        && parts.iter().all(|part| {
            !part.is_empty()
                && *part != "."
                && *part != ".."
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        })
        && parts
            .last()
            .and_then(|file| file.rsplit_once('.'))
            .is_some_and(|(stem, extension)| {
                !stem.is_empty() && extensions.iter().any(|e| extension.eq_ignore_ascii_case(e))
            })
}
/// One origin per applet, so storage is not shared between applets. `.example`
/// is reserved and never resolves, as Microsoft recommends for virtual hosts.
pub fn host(name: &str) -> String {
    format!(
        "applet-{}.illium.example",
        name.to_ascii_lowercase().replace('_', "-")
    )
}
pub fn url(name: &str, page: &str) -> String {
    format!("https://{}/{}", host(name), page.replace('\\', "/"))
}
/// The host (with its port) of a `site` URL: `https://`, a host without user
/// information, then nothing or a path. The host is what `allowed` checks.
pub fn site_host(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let host = rest.split(['/', '?', '#']).next()?;
    let plain = !host.is_empty()
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b':'));
    let path = matches!(rest.as_bytes().get(host.len()), None | Some(b'/'));
    (plain && path).then(|| host.to_owned())
}
fn starts_with(text: &str, prefix: &str) -> bool {
    text.get(..prefix.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
}
/// Whether a page may navigate to or load `uri`: only `hosts`, its own and the
/// manifest's extra ones. `data:` and `blob:` resources never leave the page;
/// navigating to them does.
pub fn allowed(hosts: &[String], uri: &str, navigation: bool) -> bool {
    hosts
        .iter()
        .any(|host| starts_with(uri, &format!("https://{host}/")))
        || (!navigation && (starts_with(uri, "data:") || starts_with(uri, "blob:")))
}
#[derive(Debug, PartialEq)]
pub enum Request {
    /// Pending script state, sent once per document by the injected script.
    Ready,
    Action(Option<String>),
    Close,
}
/// Decodes a message posted by the injected script. Several arguments become
/// one JSON array, like a multi-argument Slint `action` callback.
pub fn request(json: &str) -> Result<Request, String> {
    let message: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let action = match message["type"].as_str() {
        Some("ready") => return Ok(Request::Ready),
        Some("close") => return Ok(Request::Close),
        Some("action") => match &message["args"] {
            Value::Null => None,
            Value::String(only) => Some(only.clone()),
            Value::Array(items) => {
                let strings = items
                    .iter()
                    .map(|item| item.as_str().ok_or("action arguments must be strings"))
                    .collect::<Result<Vec<_>, _>>()?;
                match strings.as_slice() {
                    [] => None,
                    [only] => Some((*only).to_owned()),
                    _ => Some(serde_json::to_string(&strings).map_err(|e| e.to_string())?),
                }
            }
            _ => return Err("action arguments must be a string or an array".into()),
        },
        _ => return Err("unknown message".into()),
    };
    if action.as_ref().is_some_and(|a| a.len() > MAX_ACTION_BYTES) {
        return Err(format!("action exceeds {MAX_ACTION_BYTES} bytes"));
    }
    Ok(Request::Action(action))
}
/// An explicit theme mode wins; otherwise the background's luminance decides.
pub fn dark(theme: &Theme) -> bool {
    match theme.mode.as_deref() {
        Some("light") => false,
        Some("dark") => true,
        _ => illium_theme::rgb(&theme.background).is_none_or(|(r, g, b)| {
            0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b) < 128.0
        }),
    }
}
pub fn theme(theme: &Theme) -> Value {
    json!({
        "bg": theme.background,
        "surface": theme.surface,
        "overlay": theme.overlay,
        "fg": theme.text,
        "muted": theme.subtext,
        "accent": theme.accent,
        "green": theme.green,
        "yellow": theme.yellow,
        "red": theme.red,
        "dark": dark(theme),
    })
}
/// Envelope understood by the injected script.
pub fn message(kind: &str, value: Value) -> String {
    json!({ "type": kind, "value": value }).to_string()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pages_stay_inside_the_folder() {
        for good in [
            "index.html",
            "web/index.html",
            "web\\Game.HTM",
            "a-b_c.d.html",
        ] {
            assert!(page(good), "{good}");
        }
        for bad in [
            "",
            "/index.html",
            "C:\\index.html",
            "../index.html",
            "web/../index.html",
            "./index.html",
            "web//index.html",
            "index.js",
            ".html",
            "my page.html",
            "index.html?x",
            "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q.html",
        ] {
            assert!(!page(bad), "{bad}");
        }
        assert!(stylesheet("theme.css"));
        assert!(stylesheet("css\\Theme.CSS"));
        assert!(!stylesheet("index.html"));
        assert!(!stylesheet("../theme.css"));
    }
    #[test]
    fn style_script_embeds_the_sheet() {
        let script = style_script("html { --dl-bg: var(--illium-bg); }\n/* it's </script> */");
        assert!(script.contains(
            r#"const css = "html { --dl-bg: var(--illium-bg); }\n/* it's </script> */";"#
        ));
        assert!(!script.contains("__ILLIUM_CSS__"));
    }
    #[test]
    fn site_hosts() {
        assert_eq!(
            site_host("https://notes.example.org/").as_deref(),
            Some("notes.example.org")
        );
        assert_eq!(
            site_host("https://notes.example.org:8443/app").as_deref(),
            Some("notes.example.org:8443")
        );
        assert_eq!(
            site_host("https://notes.example.org").as_deref(),
            Some("notes.example.org")
        );
        for bad in [
            "http://notes.example.org/",
            "https://user@notes.example.org/",
            "https:///path",
            "https://notes.example.org?x=1",
            "https://",
            "",
        ] {
            assert_eq!(site_host(bad), None, "{bad}");
        }
    }
    #[test]
    fn hosts_are_per_applet() {
        assert_eq!(host("Ankidle"), "applet-ankidle.illium.example");
        assert_eq!(host("_web-template"), "applet--web-template.illium.example");
        assert_eq!(
            url("game", "web\\index.html"),
            "https://applet-game.illium.example/web/index.html"
        );
    }
    #[test]
    fn only_the_applet_host_is_reachable() {
        let host = vec![host("game")];
        for (uri, navigation) in [
            ("https://applet-game.illium.example/index.html", true),
            ("HTTPS://APPLET-GAME.ILLIUM.EXAMPLE/img/a.png", false),
            ("data:image/png;base64,AAAA", false),
            ("blob:https://applet-game.illium.example/123", false),
        ] {
            assert!(allowed(&host, uri, navigation), "{uri}");
        }
        for (uri, navigation) in [
            ("https://example.com/", false),
            ("http://applet-game.illium.example/index.html", false),
            ("https://applet-game.illium.example.evil.com/", false),
            ("https://applet-game.illium.example@evil.com/", false),
            ("https://applet-other.illium.example/index.html", true),
            ("wss://applet-game.illium.example.evil.com/", false),
            ("file:///C:/Windows/win.ini", true),
            ("data:text/html,<p>", true),
            ("about:blank", true),
            ("", false),
        ] {
            assert!(!allowed(&host, uri, navigation), "{uri}");
        }
    }
    #[test]
    fn extra_hosts_are_reachable_too() {
        let hosts = vec![
            "notes.example.org".to_owned(),
            "login.example.com".to_owned(),
        ];
        assert!(allowed(&hosts, "https://notes.example.org/", true));
        assert!(allowed(
            &hosts,
            "https://login.example.com/oauth2/authorize?x=1",
            true
        ));
        assert!(allowed(&hosts, "https://login.example.com/app.js", false));
        assert!(!allowed(
            &hosts,
            "https://login.example.com.evil.com/",
            true
        ));
        assert!(!allowed(&hosts, "https://cdn.example.com/app.js", false));
        assert!(!allowed(&[], "https://notes.example.org/", true));
    }
    #[test]
    fn page_requests() {
        assert_eq!(request(r#"{"type":"ready"}"#), Ok(Request::Ready));
        assert_eq!(request(r#"{"type":"close"}"#), Ok(Request::Close));
        assert_eq!(
            request(r#"{"type":"action","args":null}"#),
            Ok(Request::Action(None))
        );
        assert_eq!(
            request(r#"{"type":"action","args":"buy 2411"}"#),
            Ok(Request::Action(Some("buy 2411".into())))
        );
        assert_eq!(
            request(r#"{"type":"action","args":["one"]}"#),
            Ok(Request::Action(Some("one".into())))
        );
        assert_eq!(
            request(r#"{"type":"action","args":["bar","141,0,0"]}"#),
            Ok(Request::Action(Some(r#"["bar","141,0,0"]"#.into())))
        );
        assert_eq!(
            request(r#"{"type":"action","args":[]}"#),
            Ok(Request::Action(None))
        );
        let long = format!(r#"{{"type":"action","args":"{}"}}"#, "x".repeat(4097));
        for bad in [
            "",
            "not json",
            r#""action""#,
            r#"{"type":"other"}"#,
            r#"{"type":"action","args":3}"#,
            r#"{"type":"action","args":["a",1]}"#,
            long.as_str(),
        ] {
            assert!(request(bad).is_err(), "{bad}");
        }
    }
    #[test]
    fn theme_messages() {
        let mut theme = Theme::parse(
            "name = 't'\nbackground = '#1e1e2e'\nsurface = '#313244'\noverlay = '#45475a'\n\
             text = '#cdd6f4'\nsubtext = '#a6adc8'\naccent = '#cba6f7'\ngreen = '#a6e3a1'\n\
             yellow = '#f9e2af'\nred = '#f38ba8'\n",
        )
        .unwrap();
        let value = super::theme(&theme);
        assert_eq!(value["bg"], "#1e1e2e");
        assert_eq!(value["muted"], "#a6adc8");
        assert_eq!(value["dark"], true);
        theme.background = "#eff1f5".into();
        assert!(!dark(&theme));
        theme.mode = Some("dark".into());
        assert!(dark(&theme));
        assert_eq!(
            message("busy", json!(true)),
            r#"{"type":"busy","value":true}"#
        );
    }
}
