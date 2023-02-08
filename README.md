# hermit-wasm

This is a POC of a Unikernel designed to run WebAssembly code.

The project has been done as part of [SUSE Hackweek 2023](https://hackweek.opensuse.org/22/projects/build-a-unikernel-that-runs-webassembly).
The code has been written over a week as a learning experiment, hence some
shortcuts have been taken 😅

The code has some limitations, that are described below.

For more details, checkout [this series](https://flavio.castelli.me/2023/02/07/building-a-unikernel-that-runs-webassembly---part-1/) of blog posts.

## Goal of the POC

The goal of this Hackweek project has been to learn about
[RustyHermit](https://github.com/hermitcore/rusty-hermit) and figure
out how hard it would be to create a Unikernel capable of running WebAssembly.

WASI support has not been a goal of this project. I instead targeted a portion
of the [SpiderLightning project](https://github.com/deislabs/spiderlightning)
APIs.

I wanted to be able to show the same WebAssembly module being run both
by the vanilla [`slight`](https://github.com/deislabs/spiderlightning#getting-started)
runtime and by this Unikernel.

## Bill of materials

* Unikernel: [RustyHermit](https://github.com/hermitcore/rusty-hermit)
* WebAssembly runtime: [wasmi](https://crates.io/crates/wasmi) - this runtime has
  been used because it's written in pure Rust and can be built into the unikernel.
  Other WebAssembly runtimes are currently assuming the availability of `libc`, hence
  they cannot be built as a RustyHermit application.
* [WIT definitions](https://github.com/WebAssembly/component-model/blob/main/design/mvp/WIT.md)
  are taken from the [SpiderLightning project](https://github.com/deislabs/spiderlightning).
  The WASMI bindings are generated using [this fork](https://github.com/flavio/wit-bindgen/tree/wasmi)
  of `wit-bindgen` that adds WASMI support.

## Requirements

* [rustup](https://www.rust-lang.org/tools/install)
* [NASM](https://nasm.us/)
* [QEMU](https://www.qemu.org/)
* A Redis server

> **Note:** Currently RustyHermit supports only the x86_64 platform.

## The demo application

The unikernel will run the SpiderLightning [`http-server-demo`](https://github.com/deislabs/spiderlightning/tree/main/examples/http-server-demo) example.

This code makes use of two SpiderLightning interfaces:
  * [KeyValue](https://github.com/deislabs/spiderlightning/blob/main/wit/keyvalue.wit)
  * [HttpServer](https://github.com/deislabs/spiderlightning/blob/main/wit/http-server.wit)

The WebAssembly module will start a HTTP server listening on `0.0.0.0:3000` of
the unikernel.

The web server exposes the following routes:

* `GET` `/hello`: this prints back a message
* `GET` `/foo`: this returns the value of the `my-container:key` key inside of the K/V store
* `PUT` `/bar`: this sets the value of the `my-container:key` key inside of the K/V store

> **Note:** the code runs a polished version of the example based on [this PR](https://github.com/deislabs/spiderlightning/pull/318).

The WebAssembly module can be found under the `/wasm` directory. The code has
then been compiled targeting the `wasm32-unknown-unknown` Rust target.

## Known limitations

The POC suffers from the following limitations.

### The WebAssembly module is embedded into the unikernel

I didn't find an easy way to get the `.wasm` file into the running VM.
Right now the code is embedded at compile time into the unikernel by using the
[`include_bytes`](https://doc.rust-lang.org/std/macro.include_bytes.html)
Rust macro.

### No TLS support

TLS support via openssl is of course not doable from within the unikernel. 

Unfortunately [rustls](https://github.com/rustls/rustls) depends on the
[`ring`](https://crates.io/crates/ring) crate, which does not compile when
targeting RustyHermit.

Because of that, it's not possible to connect to a TLS terminated Redis instances.
Moreover, the http server ran by the unikernel is not doing TLS termination.

### RustyHermit scheduler

The scheduler of RustyHermit seems to have some problems managing the different
threads ran by my application (connection pool towards Redis, workers for the
HTTP server, the `main` that handles the WebAssembly engine).

Because of that, the response time of the web server are fluctuating a lot.

## Usage

### Build the unikernel

The unikernel can be built using the following Makefile target:

```
make build
```

### Run the application

The unikernel must be run using QEMU, the
[uhyve](https://github.com/hermitcore/uhyve)
hypervisor cannot be used because it doesn't have network support yet.

The demo application needs to interact with a Redis server. This can be
started with the help of docker:

```console
docker run --name some-redis --net host redis
```

> **Note:** the container will have access to the network stack of the
> host. This is convenient because it will make the Redis server
> reachable by the unikernel at the `10.0.2.2` address.

Once Redis is running, the unikernel can be run using the following
Makefile target:

```console
make run
```

> **Note:** this has been tested only on a Linux host.

This will start QEMU using the ["user networking (SLIRP)"](https://wiki.qemu.org/Documentation/Networking#User_Networking_.28SLIRP.29)
stack. This is slower than using a `tap` device, but it works out of the box
and doesn't require root privileges.

Port 3000 on the host will be forwarded to port 3000 of the guest. This is the
port used by the web server of the unikernel.

> **Note:** The unikernel application has different cli flags. These can be set as kernel flags.
This is done inside of the `Makefile`, using QEMU `-append` flag.

### Demo

![A screencast of the unikernel application running the Spiderlightning http-server demo](https://flavio.castelli.me/images/unikernel-webassembly/demo.gif "It's alive!")

