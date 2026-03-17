use crate::models::*;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

#[derive(uniffi::Object)]
pub struct ApiClient {
    host: String,
    port: u16,
    token: String,
}

#[uniffi::export]
impl ApiClient {
    #[uniffi::constructor]
    pub fn new(host: String, port: u16, token: String) -> Self {
        Self { host, port, token }
    }

    #[uniffi::constructor]
    pub fn from_connection(conn: ServerConnection) -> Self {
        Self {
            host: conn.host,
            port: conn.port,
            token: conn.token,
        }
    }

    pub fn list_workspaces(&self) -> Result<Vec<WorkspaceInfo>, ApiError> {
        let body = self.get("/api/workspaces")?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn get_workspace(&self, name: String) -> Result<WorkspaceInfo, ApiError> {
        let body = self.get(&format!("/api/workspaces/{name}"))?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn create_workspace(&self, req: CreateWorkspaceReq) -> Result<WorkspaceInfo, ApiError> {
        let payload = serde_json::to_string(&req).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        let body = self.post("/api/workspaces", &payload)?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn delete_workspace(&self, name: String) -> Result<(), ApiError> {
        self.delete(&format!("/api/workspaces/{name}"))?;
        Ok(())
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectInfo>, ApiError> {
        let body = self.get("/api/projects")?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn get_project(&self, name: String) -> Result<ProjectDetail, ApiError> {
        let body = self.get(&format!("/api/projects/{name}"))?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn create_project(&self, project: ProjectDetail) -> Result<ProjectDetail, ApiError> {
        let payload = serde_json::to_string(&project).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        let body = self.post("/api/projects", &payload)?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn update_project(&self, name: String, project: ProjectDetail) -> Result<ProjectDetail, ApiError> {
        let payload = serde_json::to_string(&project).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })?;
        let body = self.put(&format!("/api/projects/{name}"), &payload)?;
        serde_json::from_str(&body).map_err(|e| ApiError::Parse {
            message: e.to_string(),
        })
    }

    pub fn delete_project(&self, name: String) -> Result<(), ApiError> {
        self.delete(&format!("/api/projects/{name}"))?;
        Ok(())
    }

    pub fn acp_send(&self, workspace: String, message: String) -> Result<String, ApiError> {
        let payload = serde_json::to_string(&AcpSendRequest { message }).map_err(|e| {
            ApiError::Parse {
                message: e.to_string(),
            }
        })?;
        self.post(&format!("/acp/{workspace}"), &payload)
    }
}

impl ApiClient {
    fn connect(&self) -> Result<TcpStream, ApiError> {
        let addr = format!("{}:{}", self.host, self.port);
        let stream = TcpStream::connect_timeout(
            &addr.parse().map_err(|e| ApiError::Network {
                message: format!("invalid address: {e}"),
            })?,
            Duration::from_secs(10),
        )
        .map_err(|e| ApiError::Network {
            message: e.to_string(),
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();
        Ok(stream)
    }

    fn request(&self, method: &str, path: &str, body: Option<&str>) -> Result<(u16, String), ApiError> {
        let mut stream = self.connect()?;

        let content_length = body.map(|b| b.len()).unwrap_or(0);
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\n\
             Host: {}:{}\r\n\
             Authorization: Bearer {}\r\n\
             Content-Type: application/json\r\n\
             Content-Length: {content_length}\r\n\
             Connection: close\r\n\
             \r\n",
            self.host, self.port, self.token
        );
        if let Some(b) = body {
            request.push_str(b);
        }

        stream
            .write_all(request.as_bytes())
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;

        let mut reader = BufReader::new(&stream);
        let mut status_line = String::new();
        reader
            .read_line(&mut status_line)
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;

        let status = parse_status(&status_line)?;

        let mut content_len: usize = 0;
        let mut chunked = false;
        loop {
            let mut header = String::new();
            reader
                .read_line(&mut header)
                .map_err(|e| ApiError::Network {
                    message: e.to_string(),
                })?;
            let trimmed = header.trim();
            if trimmed.is_empty() {
                break;
            }
            let lower = trimmed.to_lowercase();
            if let Some(val) = lower.strip_prefix("content-length:") {
                content_len = val.trim().parse().unwrap_or(0);
            }
            if lower.contains("transfer-encoding") && lower.contains("chunked") {
                chunked = true;
            }
        }

        let response_body = if chunked {
            read_chunked(&mut reader)?
        } else if content_len > 0 {
            let mut buf = vec![0u8; content_len];
            reader
                .read_exact(&mut buf)
                .map_err(|e| ApiError::Network {
                    message: e.to_string(),
                })?;
            String::from_utf8_lossy(&buf).into_owned()
        } else {
            let mut buf = String::new();
            let _ = reader.read_to_string(&mut buf);
            buf
        };

        if status == 401 || status == 403 {
            return Err(ApiError::AuthFailed);
        }

        if status >= 400 {
            return Err(ApiError::Server {
                status,
                message: response_body,
            });
        }

        Ok((status, response_body))
    }

    fn get(&self, path: &str) -> Result<String, ApiError> {
        self.request("GET", path, None).map(|(_, b)| b)
    }

    fn post(&self, path: &str, body: &str) -> Result<String, ApiError> {
        self.request("POST", path, Some(body)).map(|(_, b)| b)
    }

    fn put(&self, path: &str, body: &str) -> Result<String, ApiError> {
        self.request("PUT", path, Some(body)).map(|(_, b)| b)
    }

    fn delete(&self, path: &str) -> Result<String, ApiError> {
        self.request("DELETE", path, None).map(|(_, b)| b)
    }
}

fn parse_status(line: &str) -> Result<u16, ApiError> {
    let parts: Vec<&str> = line.splitn(3, ' ').collect();
    if parts.len() < 2 {
        return Err(ApiError::Network {
            message: "malformed HTTP response".into(),
        });
    }
    parts[1].parse().map_err(|_| ApiError::Network {
        message: "invalid status code".into(),
    })
}

fn read_chunked(reader: &mut BufReader<&TcpStream>) -> Result<String, ApiError> {
    let mut body = String::new();
    loop {
        let mut size_line = String::new();
        reader
            .read_line(&mut size_line)
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;
        let size = usize::from_str_radix(size_line.trim(), 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let mut buf = vec![0u8; size];
        reader
            .read_exact(&mut buf)
            .map_err(|e| ApiError::Network {
                message: e.to_string(),
            })?;
        body.push_str(&String::from_utf8_lossy(&buf));
        let mut crlf = [0u8; 2];
        let _ = reader.read_exact(&mut crlf);
    }
    Ok(body)
}

#[uniffi::export]
pub fn parse_qr_connection(qr_data: String) -> Result<ServerConnection, ApiError> {
    serde_json::from_str(&qr_data).map_err(|e| ApiError::Parse {
        message: format!("invalid QR data: {e}"),
    })
}

#[uniffi::export]
pub fn parse_qr_token(qr_data: String) -> Result<String, ApiError> {
    let trimmed = qr_data.trim();
    if trimmed.is_empty() {
        return Err(ApiError::Parse {
            message: "empty QR data".into(),
        });
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_ok() {
        assert_eq!(parse_status("HTTP/1.1 200 OK\r\n").unwrap(), 200);
    }

    #[test]
    fn parse_status_created() {
        assert_eq!(parse_status("HTTP/1.1 201 Created\r\n").unwrap(), 201);
    }

    #[test]
    fn parse_status_not_found() {
        assert_eq!(parse_status("HTTP/1.1 404 Not Found\r\n").unwrap(), 404);
    }

    #[test]
    fn parse_status_malformed() {
        assert!(parse_status("garbage").is_err());
    }

    #[test]
    fn parse_qr_connection_valid() {
        let json = r#"{"host":"192.168.1.100","port":9099,"token":"abc123"}"#;
        let conn = parse_qr_connection(json.to_string()).unwrap();
        assert_eq!(conn.host, "192.168.1.100");
        assert_eq!(conn.port, 9099);
        assert_eq!(conn.token, "abc123");
    }

    #[test]
    fn parse_qr_connection_invalid() {
        assert!(parse_qr_connection("not json".to_string()).is_err());
    }

    #[test]
    fn parse_qr_token_valid() {
        let token = parse_qr_token("abc:123:sig".to_string()).unwrap();
        assert_eq!(token, "abc:123:sig");
    }

    #[test]
    fn parse_qr_token_trims_whitespace() {
        let token = parse_qr_token("  tok  ".to_string()).unwrap();
        assert_eq!(token, "tok");
    }

    #[test]
    fn parse_qr_token_empty_fails() {
        assert!(parse_qr_token("".to_string()).is_err());
        assert!(parse_qr_token("  ".to_string()).is_err());
    }

    #[test]
    fn client_construction() {
        let client = ApiClient::new("localhost".into(), 9099, "token".into());
        assert_eq!(client.host, "localhost");
        assert_eq!(client.port, 9099);
    }

    #[test]
    fn client_from_connection() {
        let conn = ServerConnection {
            host: "10.0.0.1".into(),
            port: 9099,
            token: "secret".into(),
        };
        let client = ApiClient::from_connection(conn);
        assert_eq!(client.host, "10.0.0.1");
        assert_eq!(client.token, "secret");
    }
}
