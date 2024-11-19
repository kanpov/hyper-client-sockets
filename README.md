## Hyper Client Sockets

Before hyper v1, hyperlocal was the most convenient solution to use Unix sockets for both client and server. With hyper v1, server socket support is no longer needed (just use `UnixListener` or `VsockListener` instead of `TcpListener`), yet hyperlocal still has it and hasn't received a release since several years.

This library provides hyper v1 client support for:

- Unix (`AF_UNIX`) sockets (`HyperUnixStream` implementing hyper traits)
- VSock (`AF_VSOCK`) sockets (`HyperVsockStream` implementing hyper traits)
- Firecracker Unix sockets that need `CONNECT` commands in order to establish a tunnel (`HyperFirecrackerStream` implementing hyper traits)

Additionally, the library supports different async I/O backends:

- Tokio using `tokio-backend`
- `async-io` stack using `async-io-backend`

The backend to use when connecting can be specified with the `Backend` enum. Compiling with both backends enabled is supported but not recommended, and leaving the choice of backend to the user is recommended for library developers.

Lastly, the `connector` feature provides compatibility with `hyper-util`'s `Client` connection pool.
