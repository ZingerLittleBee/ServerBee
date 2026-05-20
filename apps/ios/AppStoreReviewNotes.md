# ServerBee iOS — App Store Review Notes

## What ServerBee does

ServerBee is a companion client for the **self-hosted** ServerBee VPS monitoring server. Users
deploy the open-source ServerBee server on their own hardware (a VPS, home server, or LAN
machine), and this iOS app connects to that user-provided server over HTTP/HTTPS and a
WebSocket to view live metrics, alerts, and run a remote terminal.

The iOS app **does not connect to any first-party backend**. There is no "ServerBee cloud."
Every connection target is entered by the end user.

## Why NSAllowsArbitraryLoads is `true`

`Info.plist` sets:

```xml
<key>NSAppTransportSecurity</key>
<dict>
    <key>NSAllowsArbitraryLoads</key>
    <true/>
</dict>
```

This is required because:

1. **User-provided endpoints.** The server URL is typed by the user at login. It can be a
   bare IPv4 address (`http://192.168.1.10:9527`), an IPv6 literal, a `.local` mDNS host,
   or a public domain. We cannot enumerate `NSExceptionDomains` ahead of time.
2. **Self-hosted LAN deployments rarely have a valid TLS certificate.** A typical home or
   small-office user accesses ServerBee over their local network using a private IP — they
   cannot obtain a publicly-trusted certificate for `192.168.x.x`.
3. **HTTPS is encouraged but cannot be required.** Forcing HTTPS would lock out the
   majority of self-hosted users on day one.

## Mitigations in the app

- A yellow **"unencrypted HTTP" warning banner** is shown both on the Login screen and on
  the Settings screen whenever the configured server URL begins with `http://`. The user
  is informed in clear language that credentials and metrics travel in plain text.
- HTTPS is always preferred: the URL normalizer in `AuthViewModel.login` auto-prepends
  `https://` if the user omits the scheme.
- No analytics, telemetry, or third-party network calls are made.

## How to test

A public demo server is available at:

```
URL:      https://demo.serverbee.app
Username: reviewer
Password: <provided in App Store Connect "App Review Information" → Notes>
```

The demo server is hosted with a valid Let's Encrypt certificate, so the review can be
completed entirely over HTTPS. The `http://` banner can be observed by typing
`http://demo.serverbee.app` into the server URL field before logging in.
