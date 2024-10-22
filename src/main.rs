use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use futures::prelude::*;
use futures::StreamExt;
use socketcan::tokio::CanSocket;
use socketcan::{EmbeddedFrame, Frame};
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::{FramedRead, FramedWrite};
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
    Listen(ListenArgs),
}

#[derive(Args)]
struct ForwardCmd {
    /// CAN interface.
    #[arg(short, long, default_value = "can0")]
    interface: String,

    /// host:port to stream to. i.e. 192.168.2.10:1234
    dest: String,
}

#[derive(Args)]
struct ListenArgs {
    /// CAN interface
    #[arg(short, long, default_value = "vcan0")]
    interface: String,

    /// listen socket
    #[arg(short, long, default_value = "0.0.0.0:10023")]
    socket: String,
}

async fn pump_frames(mut tcp_stream: TcpStream, can_socket: &mut CanSocket) -> anyhow::Result<()> {
    let (tcp_read, tcp_write) = tcp_stream.split();
    let mut tcp_reader = FramedRead::new(tcp_read, frame::CanFrameCodec);
    let mut tcp_writer = FramedWrite::new(tcp_write, frame::CanFrameCodec);

    loop {
        tokio::select! {
            f = can_socket.next() => {
                match f {
                    Some(Ok(f)) => {
                        // debug!("CAN => TCP [{:x}]", f.id_word());
                        if f.id_word() & 0x520 > 0 || f.id_word() & 0x5a0 > 0 {
                            debug!("CAN => TCP UDS {:02x?} DATA {:02x?}", f.id_word(), f.data());
                        }
                        
                        if let Err(e) = tcp_writer.send(f).await {
                            error!("error sending to TCP: {}", e);
                        }
                        if let Err(e) = tcp_writer.flush().await {
                            error!("error flushing TCP: {}", e);
                        }
                    }

                    Some(Err(e)) => {
                        error!("CAN io error: {}", e);
                    }

                    None => todo!()
                }
            },

            f = tcp_reader.next() => {
                match f {
                    Some(Ok(f)) => {
                        // debug!("TCP => CAN [{:x}]", f.id_word());
                        if f.id_word() & 0x520 > 0 || f.id_word() & 0x5a0 > 0 {
                            debug!("TCP => CAN UDS {:02x?} DATA {:02x?}", f.id_word(), f.data());
                        }
                        if let Err(e) = can_socket.send(f).await {
                            error!("error sending frame: {}", e);
                        }
                        if let Err(e) = can_socket.flush().await {
                            error!("error flushing CAN socket: {}", e);
                        }
                        tokio::time::sleep(Duration::from_millis(10)).await;
                    }

                    Some(Err(e)) => {
                        error!("{}", e);
                    }

                    None => todo!()
                }
            }
        }
    }
}

async fn forward(cmd: ForwardCmd) -> anyhow::Result<()> {
    let mut can_socket = CanSocket::open(&cmd.interface)
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
    info!("connected!");
    pump_frames(tcp_stream, &mut can_socket).await?;

    Ok(())
}

fn create_vcan(name: &str) -> anyhow::Result<()> {
    info!("try creating a new vcan interface with: \n$ ip link add dev {name} type vcan\n$ ip link set {name} up");
    std::process::Command::new("ip")
        .arg("link")
        .arg("add")
        .arg("dev")
        .arg(name)
        .arg("type")
        .arg("vcan")
        .output()?;

    std::process::Command::new("ip")
        .arg("link")
        .arg("set")
        .arg(name)
        .arg("up")
        .output()?;

    Ok(())
}

async fn listen(cmd: ListenArgs) -> anyhow::Result<()> {
    let mut can_socket = match CanSocket::open(&cmd.interface) {
        Ok(s) => s,
        Err(e) => {
            error!("unable to open CAN socket: {}: {}", cmd.interface, e);
            create_vcan(&cmd.interface)?;
            CanSocket::open(&cmd.interface)?
        }
    };

    let tcp_listener = TcpListener::bind(&cmd.socket).await?;
    info!("listening on: {}", cmd.socket);
    loop {
        let (tcp_stream, addr) = tcp_listener.accept().await?;
        info!("incoming connection from: {}", addr);

        pump_frames(tcp_stream, &mut can_socket).await?;
    }

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
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
        Commands::Forward(cmd) => forward(cmd).await?,

        Commands::Listen(cmd) => listen(cmd).await?,
    }

    Ok(())
}
