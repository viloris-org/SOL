//! Example SCP layer shell panel.
//!
//! Demonstrates creating a simple top panel using layer shell.
//! This would normally be implemented in sol-shell.

use serde_json;
use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
};

// Import protocol types (simplified for example)
#[derive(serde::Serialize)]
#[serde(tag = "type")]
enum ClientMessage {
    Connect {
        app_id: String,
        pid: u32,
    },
    CreateSurface {
        surface_id: u32,
    },
    CreateLayerSurface {
        surface_id: u32,
        capability_token: Vec<u8>,
        layer: String,
        namespace: String,
        output_id: Option<u32>,
    },
    SetLayerAnchor {
        layer_id: u32,
        top: bool,
        bottom: bool,
        left: bool,
        right: bool,
    },
    SetLayerExclusiveZone {
        layer_id: u32,
        zone: i32,
    },
    SetLayerSize {
        layer_id: u32,
        width: i32,
        height: i32,
    },
    AckLayerConfigure {
        layer_id: u32,
        serial: u32,
    },
}

#[derive(serde::Deserialize, Debug)]
#[serde(tag = "type")]
enum CompositorMessage {
    Connected {
        session_id: u64,
        capability_tokens: std::collections::HashMap<String, Vec<u8>>,
    },
    Rejected {
        reason: String,
    },
    ConfigureLayerSurface {
        layer_id: u32,
        serial: u32,
        width: i32,
        height: i32,
    },
    #[serde(other)]
    Other,
}

fn send_message(stream: &mut UnixStream, msg: &ClientMessage) -> io::Result<()> {
    let json = serde_json::to_string(msg)?;
    let len = (json.len() as u32).to_be_bytes();
    stream.write_all(&len)?;
    stream.write_all(json.as_bytes())?;
    stream.flush()?;
    Ok(())
}

fn receive_message(stream: &mut UnixStream) -> io::Result<CompositorMessage> {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes)?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf)?;

    serde_json::from_slice(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn main() -> io::Result<()> {
    // Find compositor socket
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| "/run/user/1000".to_string());
    let socket_path = format!("{}/sol-compositor-0", runtime_dir);

    println!("Connecting to compositor at {}", socket_path);
    let mut stream = UnixStream::connect(&socket_path)?;

    // Connect as sol-shell
    println!("Connecting as sol-shell...");
    send_message(
        &mut stream,
        &ClientMessage::Connect {
            app_id: "sol-shell".to_string(),
            pid: std::process::id(),
        },
    )?;

    let response = receive_message(&mut stream)?;
    let layer_token = match response {
        CompositorMessage::Connected {
            capability_tokens, ..
        } => {
            println!("Connected! Capabilities: {:?}", capability_tokens.keys());
            capability_tokens
                .get("layer-shell")
                .expect("sol-shell should have layer-shell capability")
                .clone()
        }
        CompositorMessage::Rejected { reason } => {
            eprintln!("Connection rejected: {}", reason);
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
        }
        _ => {
            eprintln!("Unexpected response");
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response",
            ));
        }
    };

    // Create surface
    println!("Creating surface...");
    send_message(&mut stream, &ClientMessage::CreateSurface { surface_id: 1 })?;

    // Create layer surface at top
    println!("Creating top panel...");
    send_message(
        &mut stream,
        &ClientMessage::CreateLayerSurface {
            surface_id: 1,
            capability_token: layer_token,
            layer: "Top".to_string(),
            namespace: "panel".to_string(),
            output_id: None,
        },
    )?;

    let (layer_id, serial) = match receive_message(&mut stream)? {
        CompositorMessage::ConfigureLayerSurface {
            layer_id,
            serial,
            width,
            height,
        } => {
            println!(
                "Layer surface configured: id={}, size={}x{}",
                layer_id, width, height
            );
            (layer_id, serial)
        }
        msg => {
            eprintln!("Unexpected response: {:?}", msg);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected response",
            ));
        }
    };

    // Configure as horizontal panel at top
    println!("Configuring panel geometry...");

    // Anchor to top, stretch horizontally
    send_message(
        &mut stream,
        &ClientMessage::SetLayerAnchor {
            layer_id,
            top: true,
            bottom: false,
            left: true,
            right: true,
        },
    )?;

    // Reserve 32px for panel
    send_message(
        &mut stream,
        &ClientMessage::SetLayerExclusiveZone { layer_id, zone: 32 },
    )?;

    // Set size (width=0 means stretch)
    send_message(
        &mut stream,
        &ClientMessage::SetLayerSize {
            layer_id,
            width: 0,
            height: 32,
        },
    )?;

    // Acknowledge configuration
    send_message(
        &mut stream,
        &ClientMessage::AckLayerConfigure { layer_id, serial },
    )?;

    println!("✓ Panel created successfully!");
    println!("  Layer ID: {}", layer_id);
    println!("  Position: Top (full width)");
    println!("  Exclusive zone: 32px");
    println!();
    println!("In a real implementation, you would now:");
    println!("  1. Create a shared memory buffer");
    println!("  2. Render panel content (icons, clock, etc.)");
    println!("  3. Attach buffer and commit");
    println!("  4. Handle input events");

    Ok(())
}
