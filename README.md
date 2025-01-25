# rquest - `r`ust & quest

[![CI](https://github.com/0x676e67/rquest/actions/workflows/ci.yml/badge.svg)](https://github.com/0x676e67/rquest/actions/workflows/ci.yml)
[![Crates.io License](https://img.shields.io/crates/l/rquest)](./LICENSE)
![Crates.io MSRV](https://img.shields.io/crates/msrv/rquest)
[![crates.io](https://img.shields.io/crates/v/rquest.svg)](https://crates.io/crates/rquest)
[![Crates.io Total Downloads](https://img.shields.io/crates/d/rquest)](https://crates.io/crates/rquest)


> 🚀 Help me work seamlessly with open source sharing by [sponsoring me on GitHub](https://github.com/0x676e67/0x676e67/blob/main/SPONSOR.md)

An ergonomic, all-in-one `TLS`, `JA3`/`JA4`, and `HTTP2` fingerprint HTTP Client for spoof any browser.

## Features

- Plain, JSON, urlencoded, multipart bodies
- Header Order
- Redirect Policy
- Cookie Store
- HTTP Proxies
- WebSocket Upgrade
- HTTPS via BoringSSL
- Perfectly impersonate Chrome, Safari, and Firefox

Additional learning resources include:

- [API Documentation](https://docs.rs/rquest)
- [Repository Examples](https://github.com/0x676e67/rquest/tree/main/examples)

## Usage

This asynchronous example uses [Tokio](https://tokio.rs) and enables some
optional features, so your `Cargo.toml` could look like this:

HTTP

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
rquest = "2.0.0"
```

```rust,no_run
use rquest::{Client, Impersonate};

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to impersonate Firefox133
    let client = Client::builder()
        .impersonate(Impersonate::Firefox133)
        .build()?;

    // Use the API you're already familiar with
    let resp = client.get("https://tls.peet.ws/api/all").send().await?;
    println!("{}", resp.text().await?);

    Ok(())
}
```

WebSocket

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
rquest = { version = "2.0.0", features = ["websocket"] }
futures-util = { version = "0.3.0", default-features = false, features = ["std"] }
```

```rust,no_run
use futures_util::{SinkExt, StreamExt, TryStreamExt};
use rquest::{Impersonate, Client, Message};

#[tokio::main]
async fn main() -> Result<(), rquest::Error> {
    // Build a client to impersonate Firefox133
    let client = Client::builder()
        .impersonate(Impersonate::Firefox133)
        .build()?;

    // Use the API you're already familiar with
    let websocket = client
        .websocket("wss://echo.websocket.org")
        .send()
        .await?
        .into_websocket()
        .await?;

    let (mut tx, mut rx) = websocket.split();

    tokio::spawn(async move {
        for i in 1..11 {
            tx.send(Message::Text(format!("Hello, World! #{i}")))
                .await
                .unwrap();
        }
    });

    while let Some(message) = rx.try_next().await? {
        match message {
            Message::Text(text) => println!("received: {text}"),
            _ => {}
        }
    }

    Ok(())
}

```

## Overview

This project is a fork of [reqwest](https://github.com/seanmonstar/reqwest), and most of the APIs remain the same, similar to how [BoringSSL](https://github.com/cloudflare/boring) is a fork of OpenSSL.

The fork optimizes commonly used APIs and enhances compatibility with connection pools, making it easier to switch proxies, IP addresses, and interfaces. Projects using reqwest can be migrated to rquest directly with minimal changes.

Overall, excluding unstable features, **`rquest`** is a superset of reqwest, offering simpler and more practical APIs while also fixing HTTP version negotiation [issues](https://github.com/seanmonstar/reqwest/issues/2116) in requests.

## Performance

`BoringSSL` is a fork of `OpenSSL` that is designed to be more secure and efficient. It is used by Google Chrome and Android, and is also used by Cloudflare. In addition to that, regarding the TLS parrot echo issue in Firefox, we haven’t encountered any serious problems with `BoringSSL` related to Golang [utls issue](https://github.com/refraction-networking/utls/issues/274).

By default, `HTTP2` tracing is turned off, which will reduce the performance overhead by 15%. For more information, see issue: <https://github.com/hyperium/h2/issues/713>

## Connection Pool

`rquest` and `reqwest` handle connection pools differently. `rquest` manages connections based on the host and `Proxy`/`IP`/`Interface`, allowing flexible switching between them without affecting the connection pool. In contrast, `reqwest` manages connections only by the host.
> `Interface` refers to the network interface of the device, such as `wlan0` or `eth0`.

## Root Certificate

By default, `rquest` uses Mozilla's root certificates through the `webpki-roots` crate. This is a static root certificate bundle that is not automatically updated. It also ignores any root certificates installed on the host running `rquest`, which may be a good thing or a bad thing, depending on your point of view. But you can turn off `default-features` to cancel the default certificate bundle, and the system default certificate path will be used to load the certificate. In addition, `rquest` also provides a certificate store for users to customize the update certificate.

## Fingerprint

- TLS/HTTP2 fingerprint

Supports custom `TLS`/`HTTP2` fingerprint parameters (disabled by default). Unless you’re highly familiar with `TLS` and `HTTP2`, customization is not recommended, as it may cause unexpected issues.

- JA3/JA4/Akamai fingerprint

As `TLS` encryption technology becomes more and more sophisticated and HTTP2 becomes more popular, `JA3`/`JA4`/`Akamai` fingerprints cannot simulate browser fingerprints very well, and the parsed parameters cannot perfectly imitate the browser's `TLS`/`HTTP2` configuration fingerprints. Therefore, `rquest` has not planned to support parsing `JA3`/`JA4`/`Akamai` fingerprint strings for simulation, but encourages users to customize the configuration according to their own situation.

Most of the `Akamai` fingerprint strings obtained by users are not fully calculated. For example, the [website](https://tls.peet.ws/api/all), where the Headers Frame lacks Priority and Stream ID. If I were the server, it would be easy to detect this. For details, please refer to `HTTP2` frame [parser](https://github.com/0x676e67/pingly/blob/main/src/track/inspector/http2.rs)

- Default fingerprint

<details>

  <summary>Basic device emulation types are provided by default</summary>

- **Chrome**

`Chrome100`，`Chrome101`，`Chrome104`，`Chrome105`，`Chrome106`，`Chrome107`，`Chrome108`，`Chrome109`，`Chrome114`，`Chrome116`，`Chrome117`，`Chrome118`，`Chrome119`，`Chrome120`，`Chrome123`，`Chrome124`，`Chrome126`，`Chrome127`，`Chrome128`，`Chrome129`，`Chrome130`，`Chrome131`

- **Edge**

`Edge101`，`Edge122`，`Edge127`，`Edge131`

- **Safari**

`SafariIos17_2`，`SafariIos17_4_1`，`SafariIos16_5`，`Safari15_3`，`Safari15_5`，`Safari15_6_1`，`Safari16`，`Safari16_5`，`Safari17_0`，`Safari17_2_1`，`Safari17_4_1`，`Safari17_5`，`Safari18`，`SafariIPad18`, `Safari18_2`, `Safari18_1_1`

- **OkHttp**

`OkHttp3_9`，`OkHttp3_11`，`OkHttp3_13`，`OkHttp3_14`，`OkHttp4_9`，`OkHttp4_10`，`OkHttp5`

- **Firefox**

`Firefox109`, `Firefox117`, `Firefox128`, `Firefox133`

</details>

## Requirement

Install the dependencies required to build [BoringSSL](https://github.com/google/boringssl/blob/master/BUILDING.md#build-prerequisites)

Do not compile with packages that depend on `openssl-sys`; it links with the same prefix symbol as `boring-sys`, which can cause [link failures](https://github.com/cloudflare/boring/issues/197) and other problems. Even if compilation succeeds, using both `openssl-sys` and `boring-sys` as dependencies can cause memory segmentation faults.

If you prefer compiling for the `musl` target, it is recommended to use the [tikv-jemallocator](https://github.com/tikv/jemallocator) memory allocator; otherwise, multithreaded performance may be suboptimal. Only available in version 0.6.0, details: <https://github.com/tikv/jemallocator/pull/70>

## Building

```shell
sudo apt-get install build-essential cmake perl pkg-config libclang-dev musl-tools -y

cargo build --release
```

You can also use [this GitHub Actions workflow](https://github.com/0x676e67/rquest/blob/main/.github/compilation-guide/build.yml) to compile your project on **Linux**, **Windows**, and **macOS**.

## Contributing

If you would like to submit your contribution, please open a [Pull Request](https://github.com/0x676e67/rquest/pulls).

## Getting help

Your question might already be answered on the [issues](https://github.com/0x676e67/rquest/issues)

## License

Apache-2.0 [LICENSE](LICENSE)

## Accolades

The project is based on a fork of [reqwest](https://github.com/seanmonstar/reqwest).
