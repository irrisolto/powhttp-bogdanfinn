//! powhttp extension: copy a captured request as a bogdanfinn/tls-client
//! (`http.NewRequest` + `http.Header` + `http.HeaderOrderKey`) Go snippet.
//!
//! Two context-menu entries are registered:
//!   * "Copy as bogdanfinn request"              -> keeps the real cookies
//!   * "Copy as bogdanfinn request (no cookies)" -> drops the cookie values
//!
//! Rules applied while converting:
//!   * `cookie` values are kept or dropped depending on the chosen entry, but
//!     either way a single `cookie` entry is kept inside `http.HeaderOrderKey`
//!     (at the position of its first occurrence) so the tls-client cookie jar
//!     injects cookies in the right slot.
//!   * HTTP/2 pseudo-headers (`:method`, `:authority`, `:scheme`, `:path`, ...)
//!     are excluded from both the header map and the header order.
//!   * A non-empty request body is emitted via `strings.NewReader(...)`.

use powhttp_sdk::sessions::SessionEntry;
use powhttp_sdk::{run, ContextMenuItemSingle, Error, ExtensionHandle, SingleEntryContext};

#[tokio::main]
async fn main() -> Result<(), Error> {
    run(async |handle: ExtensionHandle| {
        handle
            .extend_context_menu_single(ContextMenuItemSingle::new(
                "copy-as-bogdanfinn",
                "Copy as bogdanfinn request",
                async |ctx: SingleEntryContext, handle: ExtensionHandle| {
                    copy_request(handle, ctx, true).await
                },
            ))
            .await?;

        handle
            .extend_context_menu_single(ContextMenuItemSingle::new(
                "copy-as-bogdanfinn-no-cookies",
                "Copy as bogdanfinn request (no cookies)",
                async |ctx: SingleEntryContext, handle: ExtensionHandle| {
                    copy_request(handle, ctx, false).await
                },
            ))
            .await?;

        Ok(())
    })
    .await
}

/// Fetch the selected entry (and its body) and put the generated Go on the clipboard.
async fn copy_request(
    handle: ExtensionHandle,
    ctx: SingleEntryContext,
    include_cookies: bool,
) -> Result<(), Error> {
    let entry = match handle.get_session_entry(ctx.session_id, ctx.entry_id).await? {
        Some(entry) => entry,
        None => return Ok(()),
    };

    let body = handle
        .get_request_body_as_text(ctx.session_id, ctx.entry_id)
        .await?;

    let code = build_go(&entry, body.as_deref(), include_cookies);
    handle.write_text_to_clipboard(&code).await?;
    Ok(())
}

/// Build the Go source snippet for a captured request.
fn build_go(entry: &SessionEntry, body: Option<&str>, include_cookies: bool) -> String {
    let method_const = to_go_method(entry.request.method.as_deref().unwrap_or("GET"));
    let url = entry.url.to_string();

    // Ordered, de-duplicated header map (preserves original name casing and the
    // first-seen order). Multiple values for the same name are grouped.
    let mut map: Vec<(String, Vec<String>)> = Vec::new();
    // Flat, ordered list of header names for http.HeaderOrderKey.
    let mut order: Vec<String> = Vec::new();
    let mut cookie_in_order = false;

    for (name, value) in entry.request.headers.iter() {
        let lower = name.to_ascii_lowercase();

        // Drop HTTP/2 pseudo-headers from both the map and the order.
        if lower.starts_with(':') {
            continue;
        }

        // Cookies always collapse to a single "cookie" entry in the order; the
        // value is only added to the map for the cookie-keeping variant.
        if lower == "cookie" {
            if !cookie_in_order {
                order.push("cookie".to_string());
                cookie_in_order = true;
            }
            if include_cookies {
                match map.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case("cookie")) {
                    Some(slot) => slot.1.push(value.clone()),
                    None => map.push(("cookie".to_string(), vec![value.clone()])),
                }
            }
            continue;
        }

        match map.iter_mut().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            Some(slot) => slot.1.push(value.clone()),
            None => {
                map.push((name.clone(), vec![value.clone()]));
                order.push(name.clone());
            }
        }
    }

    // Align the map values the way gofmt would, relative to the longest key.
    let pad = map
        .iter()
        .map(|(n, _)| go_quote(n).len() + 1) // +1 for the trailing ':'
        .max()
        .unwrap_or(0);

    let has_body = body.map(|b| !b.is_empty()).unwrap_or(false);

    let mut out = String::new();
    if has_body {
        out.push_str(&format!(
            "body := strings.NewReader({})\n",
            go_quote(body.unwrap())
        ));
        out.push_str(&format!(
            "req, err := http.NewRequest({}, {}, body)\n",
            method_const,
            go_quote(&url)
        ));
    } else {
        out.push_str(&format!(
            "req, err := http.NewRequest({}, {}, nil)\n",
            method_const,
            go_quote(&url)
        ));
    }
    out.push_str("if err != nil {\n\tlog.Println(err)\n\treturn\n}\n\n");

    out.push_str("req.Header = http.Header{\n");
    for (name, values) in &map {
        let key = format!("{}:", go_quote(name));
        let vals: Vec<String> = values.iter().map(|v| go_quote(v)).collect();
        out.push_str(&format!(
            "\t{:<width$} {{{}}},\n",
            key,
            vals.join(", "),
            width = pad
        ));
    }

    out.push_str("\thttp.HeaderOrderKey: {\n");
    for name in &order {
        out.push_str(&format!("\t\t{},\n", go_quote(name)));
    }
    out.push_str("\t},\n");
    out.push_str("}\n\n");

    out.push_str("resp, err := client.Do(req)\n");
    out.push_str("if err != nil {\n\tlog.Println(err)\n\treturn\n}\n");
    out.push_str("defer resp.Body.Close()\n\n");

    out.push_str("log.Println(fmt.Sprintf(\"status code: %d\", resp.StatusCode))\n\n");

    out.push_str("readBytes, err := io.ReadAll(resp.Body)\n");
    out.push_str("if err != nil {\n\tlog.Println(err)\n\treturn\n}\n");

    out
}

/// Map an HTTP method to its `net/http` constant, falling back to a quoted
/// string literal for anything non-standard.
fn to_go_method(method: &str) -> String {
    match method.to_ascii_uppercase().as_str() {
        "GET" => "http.MethodGet",
        "HEAD" => "http.MethodHead",
        "POST" => "http.MethodPost",
        "PUT" => "http.MethodPut",
        "PATCH" => "http.MethodPatch",
        "DELETE" => "http.MethodDelete",
        "CONNECT" => "http.MethodConnect",
        "OPTIONS" => "http.MethodOptions",
        "TRACE" => "http.MethodTrace",
        other => return go_quote(other),
    }
    .to_string()
}

/// Escape a string into a Go double-quoted string literal.
fn go_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
