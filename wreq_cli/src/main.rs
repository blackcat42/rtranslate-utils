//TODO!: cookies
#[derive(PartialEq)]
enum RunMode {
    Help,
    File,
    Pipe
}
use std::io::{self, Read, Write, BufRead};
use std::sync::{OnceLock};
use std::collections::HashMap;
use std::{time::Duration};

use clap::Parser;
use anyhow::{anyhow, Result};
use base64::{prelude::BASE64_STANDARD, Engine};

static TOKIO_RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();


#[derive(Parser, Debug, Clone)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(required = true)]
    url: String,

    #[arg(short = 'H', long = "header", value_name = "HEADER")]
    headers: Vec<String>,

    ///GET, POST
    #[arg(short = 'X', long = "request")]
    method: String,

    #[arg(short = 'd', long = "data", value_name = "POST DATA")]
    data: Option<String>,

    #[arg(long = "data-urlencode", value_name = "GET QUERY")]
    query: Vec<String>,
    
    #[arg(short = 'c', long = "cookie-jar", value_name = "FILE")]
    cookies_path: Option<String>,

    ///e.g. Safari18_5
    #[arg(long = "emulation")]
    emulation: Option<String>,

    #[arg(long = "http-version")]
    http_version: Option<String>,

    #[arg(long = "gzip", default_value = "false")]
    gzip: bool,

    #[arg(long = "connect-timeout")]
    timeout: Option<usize>, //seconds

    #[arg(short = 'x', long = "proxy", value_name = "[protocol://]host[:port]")]
    proxy: Option<String>,

    #[arg(short = 'U', long = "proxy-user", value_name = "user:password")]
    proxy_user_pass: Option<String>,

    #[arg(long = "base64-response", default_value = "false")]
    base64: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{}", e);
        std::process::exit(1);
    }
}



fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let mut handle = io::stdout().lock();

    let res = makeRequestWithWreq(args);
    match res {
        Ok(r) => {
            writeln!(handle, "{}", r)?;
        }
        Err(e) => {
            let r = format!("<WREQ_IS_SUCCESS_BEGIN>0<WREQ_IS_SUCCESS_END><WREQ_ERROR_BEGIN>{}<WREQ_ERROR_END>", e);
            writeln!(handle, "{}", r)?;
        }
    }
    Ok(())
}

fn makeRequestWithWreq(args: Args) -> Result<String> {
    //dbg!(args.clone());
    let rt = TOKIO_RT.get_or_init(|| {
        tokio::runtime::Runtime::new().expect("Tokio Runtime Error")
    });

    let mut proxy: Option<wreq::Proxy> = None;
    if let Some(proxy_url) = args.proxy {
        if let Ok(mut wreq_proxy) = wreq::Proxy::all(&proxy_url) {
            wreq_proxy = if let Some(proxy_user_pass) = args.proxy_user_pass 
                && let Some((u, p)) = proxy_user_pass.split_once(':') {
                    wreq_proxy.basic_auth(u, p)
                } else {
                    wreq_proxy
                };
            proxy = Some(wreq_proxy);
        }
        
    }

    let result = rt.block_on(async {
        //let deserialized_e: wreq_util::Emulation = serde_json::from_str(arg).unwrap_or(None); //not working
        //TODO
        let deserialized_e = match args.emulation.as_deref() {
            Some("Chrome137") => {
                Some(wreq_util::Emulation::Chrome137)
            }
            Some("Firefox139") => {
                Some(wreq_util::Emulation::Firefox139)
            }
            Some("Safari18_5") => {
                Some(wreq_util::Emulation::Safari18_5)
            }
            //TODO
            Some(&_) => {
                Some(wreq_util::Emulation::Chrome137)
            }
            None => None
        };
        let deserialized_h = match args.http_version.as_deref() {
            Some("HTTP_11") => {
                Some(wreq::Version::HTTP_11)
            }
            Some("HTTP_2") => {
                Some(wreq::Version::HTTP_2)
            }
            Some(&_) => {
                None
            }
            None => None
        };

        //dbg!(&args.headers);
        let map_headers: Option<HashMap<String, String>> = if !args.headers.is_empty() {
            Some( 
                args.headers.into_iter()
                    .filter_map(|s| {
                        s.split_once(':')
                            .map(|(key, value)| (key.to_string(), value.to_string()))
                    })
                    .collect()
            )
        } else {
            None
        };

        let mut headers = if let Some(ref r_headers) = map_headers {
            let mut h = wreq::header::HeaderMap::new();
            for (k, v) in r_headers {
                if let (Ok(name), Ok(value)) = (k.parse::<wreq::header::HeaderName>(), wreq::header::HeaderValue::from_str(v)) {
                    h.insert(name, value);
                }
            }
            Some(h)
        } else {
            None
        };

        let map_query: Option<Vec<(String, String)>> = if !args.query.is_empty() {
            Some( 
                args.query.into_iter()
                .filter_map(|s| {
                    s.split_once('=')
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                })
                .collect()
            )
        } else {
            None
        };



        let mut client = wreq::Client::builder().gzip(args.gzip);
        if let Some(proxy) = proxy {
            client = client.proxy(proxy)
        }
        /*if let Some(user_agent) = args.user_agent {
            client = client.user_agent(user_agent)
        }*/
        if let Some(t) = args.timeout {
            client = client.timeout(Duration::from_secs(t as u64))
        }
        if let Some(e) = deserialized_e {
            client = client.emulation(e)
        }
        
        if let Some(mut headers) = headers {
            client = client.default_headers(headers);
        }

        let client = client.build()?;
        //let dh = client.headers();

        let mut resp;
        if args.method == "POST" {
            //println!("POST");
            resp = client.post(args.url);
            if let Some(b) = args.data {
                resp = resp.body(b);
            }
        } else if args.data.is_none() && args.method == "GET" {
            //println!("GET");
            resp = client.get(args.url);
        } else {
            return Err(anyhow!("error: trying to send get request with data"));
        }
        if let Some(h) = deserialized_h {
            resp = resp.version(h)
        }
        if let Some(ref q) = map_query {
            resp = resp.query(q);
        }



        let resp = resp.send().await?;
        let status = resp.status();
        if !args.base64 {
            let resp_text = resp.text().await?;
            //println!("{}", &resp_text);
            return Ok((resp_text, status));
        } else {
            //TODO!
            let resp_bytes = resp.bytes().await?;
            let b64_string = BASE64_STANDARD.encode(&resp_bytes);
            //println!("{}", &resp_text);
            return Ok((b64_string, status));
        }
        
    });

    let mut response = "".to_string();
    match result {
        Ok((json_data, status)) => {
            let status_u16 = status.as_u16();
            let is_success = if status.is_success() {"1"} else {"0"};
            let status_string = status.to_string();

            response = format!("
                <WREQ_IS_SUCCESS_BEGIN>{}<WREQ_IS_SUCCESS_END>
                <WREQ_STATUS_BEGIN>{}<WREQ_STATUS_END>
                <WREQ_U16_STATUS_BEGIN>{}<WREQ_U16_STATUS_END>
                <WREQ_PAYLOAD_BEGIN>{}<WREQ_PAYLOAD_END>
            ", is_success, status_string, status_u16, json_data);
            
        }
        Err(err) => {
            response = format!("
                <WREQ_IS_SUCCESS_BEGIN>0<WREQ_IS_SUCCESS_END>
                <WREQ_ERROR_BEGIN>{}<WREQ_ERROR_END>
            ", err);
        }
    };

    Ok(response)
}
