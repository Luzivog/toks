mod context;
mod copilot_prime;
mod databases;
mod devin;
mod files;
mod others;
mod primary;
mod workbuddy;

pub(crate) use context::LocalContext;
pub(crate) use copilot_prime::{copilot, prime};
pub(crate) use databases::{goose, hermes, kilo, kiro, zcode, zed};
pub(crate) use devin::{devin_cli, devin_desktop};
pub(crate) use files::files;
pub(crate) use others::{crush, grok, kimi, trae};
pub(crate) use primary::{claude, codex, open_code};
pub(crate) use workbuddy::workbuddy;

use super::registry::{spec, LocalParticipation, LOCAL_ORDER};
use crate::{
    bucket_tz, filter_parsed_messages, get_home_dir_string, retain_for_requested_clients, scanner,
    sessions, ClientId, LocalParseOptions, ParsedMessage, ParsedMessages,
};
use std::collections::HashSet;
use std::time::Instant;

pub fn parse_local_clients(options: LocalParseOptions) -> Result<ParsedMessages, String> {
    let start = Instant::now();
    let home_dir = get_home_dir_string(&options.home_dir)?;
    let clients = options.clients.clone().unwrap_or_else(|| {
        let mut clients = ClientId::iter()
            .filter(ClientId::parse_local)
            .map(|client| client.as_str().to_string())
            .collect::<Vec<_>>();
        clients.push("synthetic".to_string());
        clients
    });
    let scan = scanner::scan_all_clients_with_scanner_settings(
        &home_dir,
        &clients,
        options.use_env_roots,
        &options.scanner_settings,
    );
    let headless_roots =
        scanner::headless_roots_with_env_strategy(&home_dir, options.use_env_roots);
    let mut context = LocalContext::new(
        scan,
        &home_dir,
        headless_roots,
        &clients,
        &options.scanner_settings,
    );

    for client in LOCAL_ORDER {
        let definition = spec(client);
        definition.assert_coherent();
        match definition.local {
            LocalParticipation::Parsed(parse) => parse(&mut context, definition),
            LocalParticipation::Disabled | LocalParticipation::ScannedButUnparsed => {}
        }
    }

    if context.include_synthetic {
        if let Some(path) = &context.scan.synthetic_db {
            context.append_without_count(sessions::synthetic::parse_octofriend_sqlite(path));
        }
    }

    if !context.include_all {
        let requested: HashSet<&str> = clients.iter().map(String::as_str).collect();
        context.messages.retain(|message| {
            retain_for_requested_clients(
                &message.client,
                &message.model_id,
                &message.provider_id,
                &requested,
            )
        });
    }
    if context.include_synthetic {
        for message in &mut context.messages {
            sessions::synthetic::normalize_synthetic_gateway_fields(
                &mut message.model_id,
                &mut message.provider_id,
            );
        }
    }
    rebucket_parsed_days(&mut context.messages, &options.scanner_settings);
    let messages = filter_parsed_messages(context.messages, &options);

    Ok(ParsedMessages {
        messages,
        counts: context.counts,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

fn rebucket_parsed_days(
    messages: &mut [ParsedMessage],
    scanner_settings: &scanner::ScannerSettings,
) {
    let timezone = bucket_tz::BucketTimezone::from_scanner_settings(scanner_settings);
    if !timezone.is_pinned() {
        return;
    }
    for message in messages {
        if message.timestamp <= 0 {
            continue;
        }
        let key = timezone.day_key(message.timestamp);
        if !key.is_empty() {
            message.date = key;
        }
    }
}
