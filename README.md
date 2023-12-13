# wacker

Like Docker, but for WASM.

![CAUTION: Rustaceans are doing experiments](docs/caution.png)

## Installation

You can download binaries directly from the [Releases](https://github.com/iawia002/wacker/releases) page.

Or you can install it using `cargo`:

```
cargo install wacker
cargo install wacker-cli
```

## Getting started

Start the wacker daemon:

```
$ wackerd
[2023-11-22T07:25:31Z INFO  wackerd] server listening on "/Users/user/.wacker/wacker.sock"
```

Run a WebAssembly module:

```
$ wacker run hello.wasm
$ wacker run time.wasm
```

Where `hello.wasm` is a simple WASM program that prints out `Hello World!` and exits, and `time.wasm` is a long-running program that constantly prints out the current time.

List running modules:

```
$ wacker list
ID              PATH         STATUS
hello-w0AqXnf   hello.wasm   Finished
time-xhQVmjU    time.wasm    Running
```

Fetch the logs:

```
$ wacker logs hello-w0AqXnf
Hello World!

$ wacker logs -f --tail 5 time-xhQVmjU
current time: 2023-11-22 07:42:34
current time: 2023-11-22 07:42:35
current time: 2023-11-22 07:42:36
current time: 2023-11-22 07:42:37
current time: 2023-11-22 07:42:38
```

And you can also stop/restart/delete the module:

```
$ wacker stop time-xhQVmjU
$ wacker restart time-xhQVmjU
$ wacker delete/rm time-xhQVmjU
```

Usage for wacker cli:

```
$ wacker -h
wacker client

Usage: wacker <COMMAND>

Commands:
  run      Runs a WebAssembly module
  list     Lists running WebAssembly modules [aliases: ps]
  stop     Stops a WebAssembly module
  restart  Restarts a WebAssembly module
  delete   Deletes a WebAssembly module [aliases: rm]
  logs     Fetches logs of a module [aliases: log]
  help     Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```
