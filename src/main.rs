use std::collections::HashMap;
use std::fmt::{Display, Formatter};
use std::str::FromStr;
use std::{env, fmt, fs};

use nom::AsBytes;
use tokio::io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
enum HTTPStatusCode {
    OK = 200,
    Created = 201,
    Accepted = 202,
    NoContent = 204,
    MovedPermanently = 301,
    Found = 302,
    NotModified = 304,
    BadRequest = 400,
    Unauthorized = 401,
    Forbidden = 403,
    NotFound = 404,
    MethodNotAllowed = 405,
    RequestTimeout = 408,
    Conflict = 409,
    Gone = 410,
    PreconditionFailed = 412,
    PayloadTooLarge = 413,
    URITooLong = 414,
    UnsupportedMediaType = 415,
}

impl Display for HTTPStatusCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self as u16)
    }
}

enum HTTPVersion {
    V1_0,
    V1_1,
    V2_0,
}

impl FromStr for HTTPVersion {
    type Err = ();

    fn from_str(input: &str) -> Result<HTTPVersion, Self::Err> {
        match input {
            "HTTP/1.0" => Ok(HTTPVersion::V1_0),
            "HTTP/1.1" => Ok(HTTPVersion::V1_1),
            "HTTP/2.0" => Ok(HTTPVersion::V2_0),
            _ => Err(()),
        }
    }
}

enum HTTPMethod {
    GET,
    POST,
    PUT,
    DELETE,
    HEAD,
    PATCH,
}

impl FromStr for HTTPMethod {
    type Err = ();

    fn from_str(input: &str) -> Result<HTTPMethod, Self::Err> {
        match input {
            "GET" => Ok(HTTPMethod::GET),
            "POST" => Ok(HTTPMethod::POST),
            "PUT" => Ok(HTTPMethod::PUT),
            "DELETE" => Ok(HTTPMethod::DELETE),
            "HEAD" => Ok(HTTPMethod::HEAD),
            "PATCH" => Ok(HTTPMethod::PATCH),
            _ => Err(()),
        }
    }
}

struct HTTPResponse {
    code: HTTPStatusCode,
    message: String,
    headers: Option<Vec<String>>,
    body: Option<String>,
}

impl HTTPResponse {
    fn format(&self) -> String {
        let mut headers = String::new();
        if let Some(headers_vec) = &self.headers {
            for header in headers_vec {
                headers.push_str(header);
                headers.push_str("\r\n");
            }
        }
        let body = if self.body.is_some() {
            self.body.as_ref().unwrap()
        } else {
            ""
        };
        format!(
            "HTTP/1.1 {} {}\r\n{}\r\n{}",
            self.code, self.message, headers, body
        )
    }
}

async fn handle_connection(
    reader: &mut BufReader<&mut TcpStream>,
    directory: &String,
) -> io::Result<()> {
    let mut line = String::new();
    reader.read_line(&mut line).await.unwrap();

    let path = line.split_whitespace().nth(1).unwrap();
    let request = line.split_whitespace().nth(0).unwrap();
    let mut headers = HashMap::new();
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        if line == "\r\n" {
            break;
        }
        let header = line.split_once(":").unwrap();
        headers.insert(header.0.trim().to_string(), header.1.trim().to_string());
    }

    let response = match path.split("/").nth(1).unwrap() {
        "echo" => {
            let content = path.get(6..).unwrap();
            let mut headers = Vec::new();
            headers.push("Content-Type: text/plain".to_string());
            headers.push(format!("Content-Length: {}", content.len()));
            HTTPResponse {
                code: HTTPStatusCode::OK,
                message: "OK".to_string(),
                headers: Some(headers),
                body: Some(content.to_string()),
            }
        }
        "user-agent" => {
            let useragent = headers.get("User-Agent").unwrap();
            let mut headers = Vec::new();
            headers.push("Content-Type: text/plain".to_string());
            headers.push(format!("Content-Length: {}", useragent.len()));
            HTTPResponse {
                code: HTTPStatusCode::OK,
                message: "OK".to_string(),
                headers: Some(headers),
                body: Some(useragent.to_string()),
            }
        }
        "files" => match request {
            "GET" => {
                let content =
                    fs::read_to_string(format!("{}/{}", directory, path.get(7..).unwrap()));
                if content.is_err() {
                    HTTPResponse {
                        code: HTTPStatusCode::NotFound,
                        message: "Not Found".to_string(),
                        headers: None,
                        body: None,
                    }
                } else {
                    let content = content.unwrap();
                    let mut headers = Vec::new();
                    headers.push("Content-Type: application/octet-stream".to_string());
                    headers.push(format!("Content-Length: {}", content.len()));
                    HTTPResponse {
                        code: HTTPStatusCode::OK,
                        message: "OK".to_string(),
                        headers: Some(headers),
                        body: Some(content.to_string()),
                    }
                }
            }
            "POST" => {
                let con_length = headers
                    .get("Content-Length")
                    .unwrap()
                    .parse::<usize>()
                    .unwrap();
                let mut body = vec![0; con_length];
                reader.read(&mut body).await?;
                fs::write(
                    format!("{}/{}", directory, path.get(7..).unwrap()),
                    body.as_bytes(),
                )
                .unwrap();
                HTTPResponse {
                    code: HTTPStatusCode::Created,
                    message: "Created".to_string(),
                    headers: None,
                    body: None,
                }
            }
            _ => HTTPResponse {
                code: HTTPStatusCode::BadRequest,
                message: "Bad Request".to_string(),
                headers: None,
                body: None,
            },
        },
        "" => HTTPResponse {
            code: HTTPStatusCode::OK,
            message: "OK".to_string(),
            headers: None,
            body: None,
        },
        _ => HTTPResponse {
            code: HTTPStatusCode::NotFound,
            message: "Not Found".to_string(),
            headers: None,
            body: None,
        },
    };
    reader
        .write_all(response.format().as_bytes())
        .await
        .unwrap();

    Ok(())
}

#[tokio::main]
async fn main() {
    let mut dir = String::new();
    for argument in env::args() {
        if dir == "--directory" {
            dir = argument;
            break;
        }
        dir = argument;
    }
    let listener = TcpListener::bind("127.0.0.1:4221").await.unwrap();

    loop {
        let (mut socket, _) = listener.accept().await.unwrap();
        let dir = dir.clone();

        tokio::spawn(async move {
            let mut reader: BufReader<&mut TcpStream> = BufReader::new(&mut socket);
            handle_connection(&mut reader, &dir).await.unwrap();
        });
    }
}
