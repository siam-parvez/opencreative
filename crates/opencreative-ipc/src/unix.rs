//! Unix domain socket transport.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::{Error, PROTOCOL_VERSION, Request, Response, Result};

/// How long a client waits for the daemon to answer one request.
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct Client {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Client {
    /// Connects and checks the protocol version. `Ok(None)` means no daemon
    /// is running.
    pub fn connect(path: &Path) -> Result<Option<Self>> {
        let stream = match UnixStream::connect(path) {
            Ok(stream) => stream,
            Err(e)
                if matches!(
                    e.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
                ) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        stream.set_read_timeout(Some(RESPONSE_TIMEOUT))?;
        let mut client = Self {
            reader: BufReader::new(stream.try_clone()?),
            writer: stream,
        };
        match client.request(&Request::Hello {
            version: PROTOCOL_VERSION,
        })? {
            Response::Hello { version } if version == PROTOCOL_VERSION => Ok(Some(client)),
            Response::Hello { version } => Err(Error::Version { daemon: version }),
            other => Err(Error::Unexpected(Box::new(other))),
        }
    }

    pub fn request(&mut self, request: &Request) -> Result<Response> {
        let mut line = serde_json::to_string(request)?;
        line.push('\n');
        self.writer.write_all(line.as_bytes())?;
        let mut reply = String::new();
        if self.reader.read_line(&mut reply)? == 0 {
            return Err(Error::Closed);
        }
        Ok(serde_json::from_str(&reply)?)
    }
}

/// The daemon's listening socket. The socket file is removed on drop.
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
}

impl Server {
    /// Binds the socket, readable only by the current user. Fails if another
    /// daemon answers on it; a stale socket file left by a crash is replaced.
    pub fn bind(path: &Path) -> Result<Self> {
        if path.exists() {
            if UnixStream::connect(path).is_ok() {
                return Err(Error::AlreadyRunning(path.to_owned()));
            }
            fs::remove_file(path)?;
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(Self {
            listener,
            path: path.to_owned(),
        })
    }

    /// Serves clients until the listener fails, one thread per connection.
    /// `hello` is answered here; every other request goes to `handler`.
    pub fn serve<H>(&self, handler: H) -> Result<()>
    where
        H: Fn(Request) -> Response + Send + Sync + 'static,
    {
        let handler = Arc::new(handler);
        for stream in self.listener.incoming() {
            let stream = match stream {
                Ok(stream) => stream,
                Err(e) => {
                    log::warn!("IPC accept failed: {e}");
                    continue;
                }
            };
            let handler = Arc::clone(&handler);
            thread::spawn(move || {
                if let Err(e) = handle(stream, handler.as_ref()) {
                    log::debug!("IPC client dropped: {e}");
                }
            });
        }
        Ok(())
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn handle(stream: UnixStream, handler: &dyn Fn(Request) -> Response) -> io::Result<()> {
    let mut writer = stream.try_clone()?;
    for line in BufReader::new(stream).lines() {
        let response = match serde_json::from_str::<Request>(&line?) {
            Ok(Request::Hello { .. }) => Response::Hello {
                version: PROTOCOL_VERSION,
            },
            Ok(request) => handler(request),
            Err(e) => Response::Error {
                message: format!("bad request: {e}"),
            },
        };
        let mut reply = serde_json::to_string(&response).expect("responses serialize");
        reply.push('\n');
        writer.write_all(reply.as_bytes())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Status;
    use opencreative_config::Config;

    fn temp_socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("oc-ipc-{}-{name}.sock", std::process::id()))
    }

    fn spawn_server(path: &Path) {
        let server = Server::bind(path).unwrap();
        thread::spawn(move || {
            server
                .serve(|req| match req {
                    Request::Status => Response::Status(Status {
                        device: None,
                        screen_on: true,
                        config: Config::default(),
                    }),
                    _ => Response::Ok,
                })
                .unwrap();
        });
    }

    #[test]
    fn no_daemon_means_none() {
        assert!(Client::connect(&temp_socket("absent")).unwrap().is_none());
    }

    #[test]
    fn request_roundtrip() {
        let path = temp_socket("roundtrip");
        spawn_server(&path);
        let mut client = Client::connect(&path).unwrap().expect("daemon is up");
        assert_eq!(
            client.request(&Request::SetScreen { on: false }).unwrap(),
            Response::Ok
        );
        assert!(matches!(
            client.request(&Request::Status).unwrap(),
            Response::Status(Status { device: None, .. })
        ));
    }

    #[test]
    fn second_daemon_is_refused_and_stale_socket_replaced() {
        let path = temp_socket("single");
        spawn_server(&path);
        assert!(matches!(Server::bind(&path), Err(Error::AlreadyRunning(_))));

        let stale = temp_socket("stale");
        drop(UnixListener::bind(&stale).unwrap()); // leaves the file, nobody listening
        assert!(stale.exists());
        let server = Server::bind(&stale).unwrap();
        drop(server);
        assert!(!stale.exists());
    }

    #[test]
    fn socket_is_private() {
        let path = temp_socket("perms");
        let _server = Server::bind(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
