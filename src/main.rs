use anyhow::Context;
use async_std::net::TcpStream;
use async_std::prelude::*;
use bytes::BytesMut;
use clap::{Args, Parser, Subcommand};
use futures::pin_mut;
use futures::prelude::*;
use socketcan::{async_std::CanSocket, CanFrame};
use std::net::{SocketAddr, ToSocketAddrs};
use tracing::{debug, error, info};

mod frame;

#[derive(Parser)]
#[command(version, about)]
struct Cli {
    /// verbose logging
    #[arg(short, long, default_value_t = false)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Forward CAN messages from local interface and steam over TCP connection
    Forward(ForwardCmd),

    /// Listen for incoming TCP connection
    Listen {
        /// listen socket
        #[arg(short, long, default_value = "0.0.0.0:10023")]
        socket: String,
    },
}

#[derive(Args)]
struct ForwardCmd {
    /// CAN interface. i.e. can0
    interface: String,

    /// host:port to stream to. i.e. 192.168.2.10:1234
    dest: String,
}

async fn forward(cmd: ForwardCmd) -> anyhow::Result<()> {
    let can_socket = CanSocket::open(&cmd.interface)
        .with_context(|| format!("CAN interface: {}", cmd.interface))?;

    let addrs: Vec<SocketAddr> = cmd
        .dest
        .to_socket_addrs()
        .expect("unable to resolve domain")
        .collect();

    debug!("dest resolves to: {:?}", addrs);
    let socket = addrs[0];
    info!("sending to {}", socket);

    let tcp_stream = TcpStream::connect(socket).await?;
    let (mut tcp_read, mut tcp_write) = tcp_stream.split();
    let mut tcp_read_buf = BytesMut::with_capacity(1024);

    loop {
        let can_rx = can_socket.read_frame().fuse();
        let tcp_rx = frame::read_frame(&mut tcp_read_buf, &mut tcp_read).fuse();
        pin_mut!(can_rx, tcp_rx);

        futures::select! {
            f = can_rx => {
                match f {
                    Ok(f) => {
                        debug!("CAN received: {:?}", f);
                        if let Err(e) = frame::write_frame(&mut tcp_write, f).await {
                            error!("error sending to TCP: {}", e);
                        }

                    }
                    Err(e) => {
                        error!("CAN io error: {}", e);
                    }
                }
            },

            f = tcp_rx => {
                match f {
                    Ok(Some(f)) => {
                        debug!("Sending CAN frame: {:?}", f);
                        if let Err(e) = can_socket.write_frame(&f).await {
                            error!("error sending frame: {}", e);
                        }
                    },

                    _ => todo!()
                }

            }
        }
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let logging_level = match cli.verbose {
        true => tracing::Level::DEBUG,
        false => tracing::Level::INFO,
    };

    let subscriber = tracing_subscriber::fmt()
        .compact()
        .with_max_level(logging_level)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    match cli.command {
        Commands::Forward(cmd) => async_std::task::block_on(forward(cmd))?,

        Commands::Listen { socket } => {
            todo!()
        }
    }

    println!("Hello, world!");

    Ok(())
}
