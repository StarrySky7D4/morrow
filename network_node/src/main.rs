use morrow_network_node::{
    Error, Limits, Result,
    client::{Client, EndpointPolicy},
    plugin::PluginService,
    server::{Handler, Node, Route, TlsIdentity},
};
use std::{net::SocketAddr, path::Path, sync::Arc};

fn bounded_file(path: &str, max: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|_| Error::Invalid)?;
    let mut bytes = Vec::new();
    file.take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Invalid)?;
    if bytes.len() > max {
        return Err(Error::Limit);
    }
    Ok(bytes)
}
fn secret(path: &str) -> Result<String> {
    let token = String::from_utf8(bounded_file(path, 4098)?).map_err(|_| Error::Invalid)?;
    let token = token.trim_end_matches(['\r', '\n']).to_owned();
    morrow_network_node::server::validate_bearer_token(&token)?;
    Ok(token)
}
fn usage() {
    println!("Morrow API node - experimental native HTTP/HTTPS transport");
    println!("plugin <package> <new-data-directory> <byte-handler> <token-file> [127.0.0.1:port]");
    println!(
        "plugin-tls <package> <new-data-directory> <byte-handler> <token-file> <cert-pem> <key-pem> [listen-ip:port]"
    );
    println!("relay <upstream-url> <token-file> [127.0.0.1:port] [--allow-local-upstream]");
    println!("Authenticated endpoint: /v1/invoke. Default listen 127.0.0.1:0. Ctrl+C stops.");
    println!(
        "TLS needs an explicit certificate/key and listen address. No OAuth or general guest IO grants yet."
    );
}
#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() || args == ["--help"] {
        usage();
        return Ok(());
    }
    let limits = Limits::default();
    let (token, address, routes, service, identity) = match args[0].as_str() {
        "plugin" if (5..=6).contains(&args.len()) => {
            let address: SocketAddr = args
                .get(5)
                .map(String::as_str)
                .unwrap_or("127.0.0.1:0")
                .parse()
                .map_err(|_| Error::Invalid)?;
            if !address.ip().is_loopback() {
                return Err(Error::Denied);
            }
            let token = secret(&args[4])?;
            let service = PluginService::open(Path::new(&args[1]), Path::new(&args[2]), &args[3])?;
            let route = Route::new("POST", "/v1/invoke", service.route_handler())?;
            (token, address, vec![route], Some(service), None)
        }
        "plugin-tls" if (7..=8).contains(&args.len()) => {
            let address: SocketAddr = args
                .get(7)
                .map(String::as_str)
                .unwrap_or("127.0.0.1:0")
                .parse()
                .map_err(|_| Error::Invalid)?;
            let token = secret(&args[4])?;
            let certificate = bounded_file(&args[5], 64 * 1024)?;
            let key = bounded_file(&args[6], 64 * 1024)?;
            let identity = TlsIdentity::from_pem(&certificate, &key)?;
            let service = PluginService::open(Path::new(&args[1]), Path::new(&args[2]), &args[3])?;
            let route = Route::new("POST", "/v1/invoke", service.route_handler())?;
            (token, address, vec![route], Some(service), Some(identity))
        }
        "relay" if (3..=5).contains(&args.len()) => {
            let mut remaining = args[3..].iter();
            let next = remaining.next().map(String::as_str);
            let (address, allow_local) = match next {
                None => ("127.0.0.1:0", false),
                Some("--allow-local-upstream") if remaining.next().is_none() => {
                    ("127.0.0.1:0", true)
                }
                Some(value) => (
                    value,
                    match remaining.next().map(String::as_str) {
                        None => false,
                        Some("--allow-local-upstream") => true,
                        _ => return Err(Error::Invalid),
                    },
                ),
            };
            let address: SocketAddr = address.parse().map_err(|_| Error::Invalid)?;
            if !address.ip().is_loopback() {
                return Err(Error::Denied);
            }
            let target = url::Url::parse(&args[1]).map_err(|_| Error::Invalid)?;
            let methods = ["GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"];
            let client = Arc::new(Client::new(
                EndpointPolicy::new(
                    &target.origin().ascii_serialization(),
                    &methods,
                    allow_local,
                )?,
                limits,
            )?);
            let target = target.as_str().to_owned();
            let handler: Handler = Arc::new(move |mut request, cancel| {
                let client = client.clone();
                request.target = target.clone();
                // Node credentials and caller-supplied transport headers never reach upstream.
                request
                    .headers
                    .retain(|(name, _)| matches!(name.as_str(), "content-type" | "accept"));
                Box::pin(async move {
                    let mut response = client.send(request, cancel).await?;
                    response.headers.retain(|(name, _)| {
                        !matches!(
                            name.as_str(),
                            "content-length"
                                | "connection"
                                | "transfer-encoding"
                                | "keep-alive"
                                | "proxy-authenticate"
                                | "proxy-authorization"
                                | "te"
                                | "trailer"
                                | "upgrade"
                        )
                    });
                    Ok(response)
                })
            });
            let routes = methods
                .iter()
                .map(|method| Route::new(method, "/v1/invoke", handler.clone()))
                .collect::<Result<Vec<_>>>()?;
            (secret(&args[2])?, address, routes, None, None)
        }
        _ => return Err(Error::Invalid),
    };
    let scheme = if identity.is_some() { "https" } else { "http" };
    let node = if let Some(identity) = identity {
        Node::bind_tls(address, token, routes, limits, identity).await?
    } else {
        Node::bind(address, token, routes, limits).await?
    };
    println!(
        "Morrow API node listening on {scheme}://{}/v1/invoke",
        node.local_addr()
    );
    // Never print the node token, upstream URL/query or request body.
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| Error::Transport)?;
    let stopped = node.shutdown().await;
    if let Some(service) = service {
        service.shutdown().await?;
    }
    stopped
}
