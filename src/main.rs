use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use structopt::StructOpt;
use url::Url;

mod gemini;
mod verifier;

const DEFAULT_PORT: u16 = 1965;

#[derive(StructOpt)]
struct Cli {
    /// The URL to be fetched.
    url: String,

    #[structopt(long, default_value = "10")]
    max_redirects: u32,

    #[structopt(long)]
    validate_certificate: bool,
}

fn rustls_config(validate_certificate: bool) -> rustls::ClientConfig {
    let mut config = rustls::ClientConfig::new();
    match validate_certificate {
        true => config
            .root_store
            .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS),
        false => config
            .dangerous()
            .set_certificate_verifier(Arc::new(verifier::NullVerifier {})),
    }
    config
}

fn fetch(config: &Arc<rustls::ClientConfig>, url: &str) -> Result<gemini::Response> {
    let url = Url::parse(url).with_context(|| "invalid URL")?;

    match url.scheme() {
        "gemini" | "" => (),
        s => return Err(anyhow!("unknown scheme \"{}\"", s)),
    }

    let host_str = url.host_str().with_context(|| "invalid host")?;
    let port = url.port().unwrap_or(DEFAULT_PORT);

    let dns_name = webpki::DNSNameRef::try_from_ascii_str(host_str)?;
    let mut sess = rustls::ClientSession::new(config, dns_name);
    let mut stream = TcpStream::connect((host_str, port)).with_context(|| "connection failed")?;
    let mut tls = rustls::Stream::new(&mut sess, &mut stream);

    let req = gemini::request(url.as_str());
    tls.write(&req)
        .with_context(|| "failed sending gemini request")?;

    let mut plaintext = Vec::new();
    match tls.read_to_end(&mut plaintext) {
        Ok(_) => (),
        // Ignore ConnectionAborted -- this means that the server closed the
        // connection after responding.
        Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionAborted => (),
        Err(e) => Err(e).with_context(|| "TLS read error")?,
    }

    Ok(gemini::parse_response(&plaintext).with_context(|| "failed to parse response")?)
}

fn recursive_fetch(
    config: &Arc<rustls::ClientConfig>,
    url: &str,
    max_redirects: u32,
) -> Result<gemini::Response> {
    let mut redirects = 0;
    let mut current_url = url.to_string();
    while redirects <= max_redirects {
        let response = fetch(config, &current_url)?;
        match gemini::status_category(&response.header.status)? {
            gemini::StatusCategory::Redirect => {
                redirects += 1;
                current_url = response.header.meta;
            }
            _ => return Ok(response),
        }
    }
    Err(anyhow!("maximum redirects ({}) exceeded", max_redirects))
}

fn main() -> Result<()> {
    let args = Cli::from_args();
    let config = Arc::new(rustls_config(args.validate_certificate));
    let response = recursive_fetch(&config, &args.url, args.max_redirects)?;

    match gemini::status_category(&response.header.status)? {
        gemini::StatusCategory::Success => println!("{}", response.body),
        _ => {
            return Err(anyhow!(
                "{} - {}",
                response.header.status,
                response.header.meta
            ))
        }
    }

    Ok(())
}
