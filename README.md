# powhttp-bogdanfinn

A [powhttp](https://hypersolutions.co) extension that copies a captured request
as a [bogdanfinn/tls-client](https://github.com/bogdan-finn/tls-client) Go
snippet, preserving the original header order.

The generated code uses `http.NewRequest` together with an `http.Header` map and
`http.HeaderOrderKey`, so the request can be replayed through tls-client with the
same headers, in the same order, as the capture.

## Context-menu entries

The extension registers two single-entry context-menu items:

| Entry | Behaviour |
| --- | --- |
| **Copy as bogdanfinn request** | Keeps the real `cookie` values. |
| **Copy as bogdanfinn request (no cookies)** | Drops the `cookie` values. |

Either way, a single `cookie` entry is kept inside `http.HeaderOrderKey` (at the
position of its first occurrence) so the tls-client cookie jar injects cookies in
the right slot.

## Conversion rules

- **Header order and casing** are preserved. Repeated headers are grouped under
  one map key; the name appears once in `HeaderOrderKey`.
- **HTTP/2 pseudo-headers** (`:method`, `:authority`, `:scheme`, `:path`, …) are
  excluded from both the header map and the header order.
- **Cookies** collapse to a single `cookie` entry in the order; the value is only
  emitted for the cookie-keeping variant.
- A non-empty **request body** is emitted via `strings.NewReader(...)`.
- The map is **aligned** the way `gofmt` would format it.

## Example output

```go
req, err := http.NewRequest(http.MethodGet, "https://example.com/", nil)
if err != nil {
	log.Println(err)
	return
}

req.Header = http.Header{
	"user-agent": {"Mozilla/5.0 ..."},
	"accept":     {"*/*"},
	http.HeaderOrderKey: {
		"user-agent",
		"accept",
		"cookie",
	},
}

resp, err := client.Do(req)
if err != nil {
	log.Println(err)
	return
}
```

## Building

```bash
cargo build --release
```

This produces `target/release/powhttp-bogdanfinn.exe`, which
[`extension.json`](extension.json) points to as the extension command.

## License

See repository for license details.
