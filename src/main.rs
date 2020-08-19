use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use rustls;
use webpki;
use webpki_roots;

use structopt::StructOpt;
use url::Url;

mod gemini;

const DEFAULT_PORT: u16 = 1965;

#[derive(StructOpt)]
struct Cli {
    /// The URL to be fetched.
    url: String,
}

fn main() -> std::io::Result<()> {
    let args = Cli::from_args();
    let url = Url::parse(&args.url).expect("Invalid URL.");

    let host_str = url.host_str().expect("Invalid host.");
    let port = url.port().unwrap_or(DEFAULT_PORT);

    let mut config = rustls::ClientConfig::new();
    config
        .root_store
        .add_server_trust_anchors(&webpki_roots::TLS_SERVER_ROOTS);

    let dns_name = webpki::DNSNameRef::try_from_ascii_str(host_str).unwrap();
    let mut sess = rustls::ClientSession::new(&Arc::new(config), dns_name);
    let mut stream = TcpStream::connect((host_str, port)).expect("Failed to connect...");
    let mut tls = rustls::Stream::new(&mut sess, &mut stream);

    let req = gemini::request(url.as_str());
    tls.write(&req).unwrap();

    let mut plaintext = Vec::new();
    match tls.read_to_end(&mut plaintext) {
        Ok(_) => (),
        // Ignore ConnectionAborted -- this means that the server closed the
        // connection after responding.
        Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionAborted => (),
        Err(e) => panic!("TLS read error: {:?}", e),
    }

    let response = gemini::parse_response(&plaintext).expect("Failed to parse response.");

    match gemini::status_category(&response.header.status).unwrap() {
        gemini::StatusCategory::Success => println!("{}", response.body),
        _ => panic!(
            "Error: {} - {}",
            response.header.status, response.header.meta
        ),
    }

    Ok(())
}
