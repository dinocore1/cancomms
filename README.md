# CanComms

Forward CAN messages via TCP tunnel.

This utility will bridge a local CAN socket interface to another computer using a TCP connection. 

To forward CAN messages from a local CAN interface run with:

```
$ cancomms forward -i <CAN interface> <host:port>
```

Then on another computer, listen for incoming TCP connection:

```
$ cancomms listen -i <CAN interface> -s 0.0.0.0:10023
```

You can create a virtual CAN interface easily with:

```
$ sudo ip link add dev vcan0 type vcan
$ sudo ip link set vcan0 up
```


## Build for ARM64

```
$ cargo install cross --git https://github.com/cross-rs/cross
$ cross build --release --target=aarch64-unknown-linux-gnu
```