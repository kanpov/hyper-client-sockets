## Hyper Client Sockets

Before hyper v1, hyperlocal was the most convenient solution to use Unix sockets for both client and server. With hyper v1, server socket support is no longer needed (just use `UnixListener` or `VsockListener` instead of `TcpListener`), yet hyperlocal still has it and hasn't received a release since several years.

This library provides hyper v1 client support for:

- Unix (`AF_UNIX`) sockets
- VSock (`AF_VSOCK`) sockets (most commonly used in virtualized environments)
- Firecracker Unix sockets that need `CONNECT` commands in order to establish a tunnel

Plus, there is more panic-safety and more convenient extension traits for URIs. **Windows is not supported and Windows support is currently not planned**.

Both raw hyper (`handshake` and `SendRequest` + `Connection` with no pooling) and `hyper-util` (`Client`) are supported, though the latter is recommended as it has connection pooling.
