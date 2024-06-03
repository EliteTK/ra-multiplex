use std::path::Path;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::{fs, io, net};

use pin_project::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{tcp, unix, TcpListener, TcpStream, ToSocketAddrs, UnixListener, UnixStream};

pub enum SocketAddr {
    Ip(net::SocketAddr),
    Unix(tokio::net::unix::SocketAddr),
}

impl From<net::SocketAddr> for SocketAddr {
    fn from(val: net::SocketAddr) -> Self {
        SocketAddr::Ip(val)
    }
}

impl From<tokio::net::unix::SocketAddr> for SocketAddr {
    fn from(val: tokio::net::unix::SocketAddr) -> Self {
        SocketAddr::Unix(val)
    }
}

#[pin_project(project = OwnedReadHalfProj)]
pub enum OwnedReadHalf {
    Tcp(#[pin] tcp::OwnedReadHalf),
    Unix(#[pin] unix::OwnedReadHalf),
}

impl AsyncRead for OwnedReadHalf {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            OwnedReadHalfProj::Tcp(tcp) => tcp.poll_read(cx, buf),
            OwnedReadHalfProj::Unix(unix) => unix.poll_read(cx, buf),
        }
    }
}

#[pin_project(project = OwnedWriteHalfProj)]
pub enum OwnedWriteHalf {
    Tcp(#[pin] tcp::OwnedWriteHalf),
    Unix(#[pin] unix::OwnedWriteHalf),
}

impl AsyncWrite for OwnedWriteHalf {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.project() {
            OwnedWriteHalfProj::Tcp(tcp) => tcp.poll_write(cx, buf),
            OwnedWriteHalfProj::Unix(unix) => unix.poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match self.project() {
            OwnedWriteHalfProj::Tcp(tcp) => tcp.poll_write_vectored(cx, bufs),
            OwnedWriteHalfProj::Unix(unix) => unix.poll_write_vectored(cx, bufs),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            OwnedWriteHalfProj::Tcp(tcp) => tcp.poll_flush(cx),
            OwnedWriteHalfProj::Unix(unix) => unix.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            OwnedWriteHalfProj::Tcp(tcp) => tcp.poll_shutdown(cx),
            OwnedWriteHalfProj::Unix(unix) => unix.poll_shutdown(cx),
        }
    }
}

#[pin_project(project = StreamProj)]
pub enum Stream {
    Tcp(#[pin] TcpStream),
    Unix(#[pin] UnixStream),
}

impl Stream {
    pub async fn connect_tcp<A: ToSocketAddrs>(addr: A) -> io::Result<Stream> {
        Ok(Stream::Tcp(TcpStream::connect(addr).await?))
    }

    pub async fn connect_unix<P: AsRef<Path>>(addr: P) -> io::Result<Stream> {
        Ok(Stream::Unix(UnixStream::connect(addr).await?))
    }

    pub fn into_split(self) -> (OwnedReadHalf, OwnedWriteHalf) {
        match self {
            Stream::Tcp(tcp) => {
                let (read, write) = tcp.into_split();
                (OwnedReadHalf::Tcp(read), OwnedWriteHalf::Tcp(write))
            }
            Stream::Unix(unix) => {
                let (read, write) = unix.into_split();
                (OwnedReadHalf::Unix(read), OwnedWriteHalf::Unix(write))
            }
        }
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            StreamProj::Tcp(tcp) => tcp.poll_read(cx, buf),
            StreamProj::Unix(unix) => unix.poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.project() {
            StreamProj::Tcp(tcp) => tcp.poll_write(cx, buf),
            StreamProj::Unix(unix) => unix.poll_write(cx, buf),
        }
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<Result<usize, io::Error>> {
        match self.project() {
            StreamProj::Tcp(tcp) => tcp.poll_write_vectored(cx, bufs),
            StreamProj::Unix(unix) => unix.poll_write_vectored(cx, bufs),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            StreamProj::Tcp(tcp) => tcp.poll_flush(cx),
            StreamProj::Unix(unix) => unix.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.project() {
            StreamProj::Tcp(tcp) => tcp.poll_shutdown(cx),
            StreamProj::Unix(unix) => unix.poll_shutdown(cx),
        }
    }
}

pub enum Listener {
    Tcp(TcpListener),
    Unix(UnixListener),
}

impl Listener {
    pub async fn bind_tcp(addr: net::SocketAddr) -> io::Result<Listener> {
        Ok(Listener::Tcp(TcpListener::bind(addr).await?))
    }

    pub fn bind_unix<T: AsRef<Path>>(addr: T) -> io::Result<Listener> {
        match fs::remove_file(&addr) {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e),
        }
        Ok(Listener::Unix(UnixListener::bind(addr)?))
    }

    pub async fn accept(&self) -> io::Result<(Stream, SocketAddr)> {
        match self {
            Listener::Tcp(tcp) => {
                let (stream, addr) = tcp.accept().await?;
                Ok((Stream::Tcp(stream), addr.into()))
            }
            Listener::Unix(unix) => {
                let (stream, addr) = unix.accept().await?;
                Ok((Stream::Unix(stream), addr.into()))
            }
        }
    }
}
