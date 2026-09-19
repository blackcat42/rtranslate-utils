#![allow(clippy::collapsible_if)]
#![allow(clippy::needless_return)]
#![allow(clippy::needless_late_init)]
use std::io::{self, Write, BufRead, BufReader};
use std::sync::{OnceLock};
use std::collections::HashMap;
use std::{time::Duration};

use std::fs::File;
use std::path::Path;

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
    cookies_jar_path: Option<String>,

    #[arg(short = 'b', long = "cookie", value_name = "FILE")]
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

struct NetscapeCookie {
    domain: String,
    include_subdomains: String, 
    path: String, 
    secure: String, 
    exp: String,
    name: String,
    value: String
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

    let res = make_request_with_wreq(args);
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

fn make_request_with_wreq(args: Args) -> Result<String> {
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

    //Netscape cookies
    let mut cookie_pairs: Vec<String> = Vec::new();
    let mut old_cookies: Vec<NetscapeCookie> = Vec::new();
    if let Some(ref f) = args.cookies_path {
        let pairs = read_cookies(f, &args.url).unwrap_or((Vec::new(), Vec::new()));
        cookie_pairs = pairs.1;
        old_cookies = pairs.0;
    }
    //dbg!(&cookie_pairs);

    let result = rt.block_on(async {
        //let deserialized_e: wreq_util::Emulation = serde_json::from_str(arg).unwrap_or(None); //not working
        //TODO
        let deserialized_e: Option<wreq_util::Emulation>;
        //let mut ua = "curl/8.7.1".to_string();
        match args.emulation.as_deref() {
            Some("Chrome137") => {
                deserialized_e = Some(wreq_util::Emulation::Chrome137);
                //ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36".to_string();
            }
            Some("Firefox139") => {
                deserialized_e = Some(wreq_util::Emulation::Firefox139);
                //ua = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:139.0) Gecko/20100101 Firefox/139.0".to_string();
            }
            Some("Safari18_5") => {
                deserialized_e = Some(wreq_util::Emulation::Safari18_5);
                //ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.5 Safari/605.1.15".to_string();
            }
            //TODO
            Some(&_) => {
                deserialized_e = Some(wreq_util::Emulation::Chrome137);
            }
            None => {
                deserialized_e = None;
            }
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

        let mut headers = wreq::header::HeaderMap::new();

        if let Some(ref r_headers) = map_headers {
            for (k, v) in r_headers {
                if let (Ok(name), Ok(value)) = (k.parse::<wreq::header::HeaderName>(), wreq::header::HeaderValue::from_str(v)) {
                    headers.insert(name, value);
                }
            }
        }

        /*if let Ok(header_name) = "User-Agent".parse::<wreq::header::HeaderName>() 
        && let Ok(v) = wreq::header::HeaderValue::from_str(&ua) {
            headers.insert(header_name, v);
        }*/

        if !cookie_pairs.is_empty() {
            let cookie_header = cookie_pairs.join("; ");
            if let Ok(v) = wreq::header::HeaderValue::from_str(&cookie_header) {
                headers.insert(wreq::header::COOKIE, v);
            }
        }
        
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

        client = client.default_headers(headers);
        

        let client = client.build()?;
        //let dh = client.headers();

        let mut resp;
        if args.method == "POST" {
            //println!("POST");
            resp = client.post(&args.url);
            if let Some(b) = args.data {
                resp = resp.body(b);
            }
        } else if args.data.is_none() && args.method == "GET" {
            //println!("GET");
            resp = client.get(&args.url);
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
        
        if let Some(f) = args.cookies_jar_path {
            let mut netscape_cookies: Vec<NetscapeCookie> = Vec::new();

            for cookie in resp.cookies() {
                let domain = cookie.domain().unwrap_or(&args.url);
                //let include_subdomains = if domain.starts_with('.') { "TRUE" } else { "FALSE" };
                let include_subdomains = "TRUE";
                let path = cookie.path().unwrap_or("/");
                let secure = if cookie.secure() { "TRUE" } else { "FALSE" };

                let max_age = cookie.max_age()
                    .map(|dt| {
                        timestamp_from_max_age(dt)
                    }).unwrap_or(0);
                  
                let expires = cookie.expires()
                    .map(|dt| {
                        timestamp_from_expires_str(dt)
                    }).unwrap_or(0);

                let exp = if max_age > 1 {max_age} else {expires};
                let name = cookie.name();
                let value = cookie.value();
                //let prefix = if cookie.http_only().unwrap_or(false) { "#HttpOnly_" } else { "" };
                netscape_cookies.push(NetscapeCookie{
                    domain: domain.into(), 
                    include_subdomains: include_subdomains.into(), 
                    path: path.into(), 
                    secure: secure.into(), 
                    exp: exp.to_string(), 
                    name: name.into(), 
                    value: value.into()
                });
            }
            let _ = write_cookies(f, netscape_cookies, old_cookies);
        }
        let status = resp.status();
        if !args.base64 {
            let resp_text = resp.text().await?;
            return Ok((resp_text, status));
        } else {
            let resp_bytes = resp.bytes().await?;
            let b64_string = BASE64_STANDARD.encode(&resp_bytes);
            return Ok((b64_string, status));
        }
    });

    let response: String;
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


fn timestamp_from_max_age(max_age_secs: std::time::Duration) -> i64 {
    let expire_time = std::time::SystemTime::now() + max_age_secs;
    expire_time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn timestamp_from_expires_str(expires: std::time::SystemTime) -> i64 {
    expires
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn get_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}


fn path_match(request_path: &str, cookie_path: &str) -> bool {
    if request_path == cookie_path {
        return true;
    }
    if request_path.starts_with(cookie_path) {
        if cookie_path.ends_with('/') {
            return true;
        }

        //cookie_path = "/api", request_path = "/api/v1"
        if request_path.as_bytes().get(cookie_path.len()) == Some(&b'/') {
            return true;
        }
    }
    false
}

fn read_cookies(f: &str, url: &str) -> Result<(Vec<NetscapeCookie>, Vec<String>)> {
    let mut netscape_cookies: Vec<NetscapeCookie> = Vec::new();
    if !Path::new(f).exists() {
        return Err(anyhow!("path"));
    }
    let parsed_url = url::Url::parse(url).unwrap();
    let request_path = parsed_url.path();

    let mut cookie_pairs = Vec::new();
    let file = File::open(f)?;
    let reader = BufReader::new(file);
    
    for line in reader.lines() {
        let line = line?;
        //println!("{:?}", &line);
        if (line.starts_with('#') && !line.starts_with("#HttpOnly")) || line.trim().is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 7 {
            let name = parts[5];
            let value = parts[6];
            let cookie_path = parts[2];
            //let d = parts[0].replace("#HttpOnly_", "");

            netscape_cookies.push(
                NetscapeCookie {domain: parts[0].to_string(), include_subdomains: parts[1].to_string(), path: cookie_path.to_string(), secure: parts[3].to_string(), exp: parts[4].to_string(), name: name.to_string(), value: value.to_string()}
            );

            if !path_match(request_path, cookie_path) {
                continue;
            }
            if let Ok(expires) = parts[4].parse::<i64>() {
                let now = get_timestamp();
                if expires > 1 && expires < now {
                    continue;
                }
            }

            cookie_pairs.push(format!("{}={}", name, value));
        }
    }
    Ok((netscape_cookies, cookie_pairs))
}

fn write_cookies(f: String, new_cookies: Vec<NetscapeCookie>, old_cookies: Vec<NetscapeCookie>) -> Result<()> {
    let mut file = File::create(f)?;
    writeln!(file, "# Netscape HTTP Cookie File\n")?;
    let cookies = merge_cookies(new_cookies, old_cookies);
    for cookie in cookies {
        let NetscapeCookie {domain, include_subdomains, path, secure, exp, name, value} = cookie;
        writeln!(
            file,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            domain, include_subdomains, path, secure, exp, name, value
        )?;
    }
    Ok(())
}

fn merge_cookies(
    new_cookies: Vec<NetscapeCookie>,
    old_cookies: Vec<NetscapeCookie>
) -> Vec<NetscapeCookie> {
    let mut merged: std::collections::HashMap<(String, String, String), NetscapeCookie> = std::collections::HashMap::new();

    for cookie in old_cookies {
        let key = (cookie.domain.clone(), cookie.path.clone(), cookie.name.clone());
        merged.insert(key, cookie);
    }
    
    for cookie in new_cookies {
        let key = (cookie.domain.clone(), cookie.path.clone(), cookie.name.clone());
        merged.insert(key, cookie);
    }

    merged.into_values().collect()
}
