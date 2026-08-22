use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddr, TcpStream},
    time::Duration,
};

use anyhow::{ensure, Context, Result};

use crate::{accounts::AccountId, rotation::RotationRuntimeStore};

use super::{BankedResetConsumed, ROUTER_PORT};

const ACK_PATH: &str = "/banked-reset-consumed";

pub(super) fn notify_router(account: &AccountId) -> Result<()> {
    let address = SocketAddr::from((Ipv4Addr::LOCALHOST, ROUTER_PORT));
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))
        .context("connecting to the Toks router")?;
    stream.set_read_timeout(Some(Duration::from_secs(1)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let body = serde_json::to_vec(&BankedResetConsumed {
        account_id: account.clone(),
    })?;
    write!(
        stream,
        "POST {ACK_PATH} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    stream.write_all(&body)?;
    let mut response = Vec::with_capacity(256);
    stream.read_to_end(&mut response)?;
    ensure!(
        response.starts_with(b"HTTP/1.1 204"),
        "Toks router rejected the reset acknowledgement"
    );
    Ok(())
}

pub(super) fn update_stored_runtime(account: &AccountId) -> Result<()> {
    let store = RotationRuntimeStore::discover()?;
    let mut runtime = store.load()?;
    if runtime.banked_reset_consumed(account) {
        store.save(&runtime)?;
    }
    Ok(())
}
