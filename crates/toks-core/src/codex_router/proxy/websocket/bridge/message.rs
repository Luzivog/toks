use axum::extract::ws::{CloseFrame as ClientClose, Message as ClientMessage};
use tokio_tungstenite::tungstenite::{
    protocol::frame::CloseFrame as ServerClose, Message as ServerMessage,
};

pub(super) fn to_server(message: ClientMessage) -> ServerMessage {
    match message {
        ClientMessage::Text(value) => ServerMessage::Text(value.as_str().into()),
        ClientMessage::Binary(value) => ServerMessage::Binary(value),
        ClientMessage::Ping(value) => ServerMessage::Ping(value),
        ClientMessage::Pong(value) => ServerMessage::Pong(value),
        ClientMessage::Close(frame) => ServerMessage::Close(frame.map(|frame| ServerClose {
            code: frame.code.into(),
            reason: frame.reason.as_str().into(),
        })),
    }
}

pub(super) fn to_client(message: ServerMessage) -> ClientMessage {
    match message {
        ServerMessage::Text(value) => ClientMessage::Text(value.as_str().into()),
        ServerMessage::Binary(value) => ClientMessage::Binary(value),
        ServerMessage::Ping(value) => ClientMessage::Ping(value),
        ServerMessage::Pong(value) => ClientMessage::Pong(value),
        ServerMessage::Close(frame) => ClientMessage::Close(frame.map(|frame| ClientClose {
            code: frame.code.into(),
            reason: frame.reason.as_str().into(),
        })),
        ServerMessage::Frame(_) => ClientMessage::Close(None),
    }
}
