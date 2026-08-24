use super::{
    CachePolicy as C, ClientParserDef as D, CountPolicy as N, LocalParticipation as L,
    MergePolicy as M, PricingPolicy as P, RequestPolicy as R, SpecialCachePolicy as S,
};
use crate::ingest::{local as l, priced as p};
use crate::{message_cache, sessions, ClientId as I};

macro_rules! row {
    ($id:ident, $scan:ident, $parser:expr, $priced:path, $local:expr, $identity:expr,
     $cache:expr, $pricing:ident, $merge:ident, $count:ident, $request:ident) => {
        D {
            client: I::$id,
            scan_bucket: I::$scan,
            parser: $parser,
            priced: $priced,
            local: $local,
            cache_identity: $identity,
            cache: $cache,
            pricing: P::$pricing,
            merge: M::$merge,
            count_bucket: if matches!(N::$count, N::None) {
                None
            } else {
                Some(I::$id)
            },
            count: N::$count,
            request: R::$request,
        }
    };
}

#[rustfmt::skip]
static REGISTRY: [D; I::COUNT] = [
    row!(OpenCode, OpenCode, None, p::open_code, L::Parsed(l::open_code), Some(I::OpenCode), C::Special(S::OpenCodeMixed), Reprice, OpenCodeSqliteFirst, Rows, ScannerSelected),
    row!(Claude, Claude, None, p::claude, L::Parsed(l::claude), Some(I::Claude), C::Special(S::ClaudeRetention), Reprice, ClaudeFirstWins, Rows, ScannerSelected),
    row!(Codex, Codex, Some(sessions::codex::parse_codex_file), p::codex, L::Parsed(l::codex), Some(I::Codex), C::Special(S::CodexIncremental), Reprice, CodexFirstWins, Rows, ScannerSelected),
    row!(Cursor, Cursor, Some(sessions::cursor::parse_cursor_file), p::cached_files, L::Disabled, Some(I::Cursor), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, None, ScannerSelected),
    row!(Gemini, Gemini, Some(sessions::gemini::parse_gemini_file), p::gemini, L::Parsed(l::files), Some(I::Gemini), C::Special(S::GeminiValidity), Reprice, Append, Rows, ScannerSelected),
    row!(Amp, Amp, Some(sessions::amp::parse_amp_file), p::cached_files, L::Parsed(l::files), Some(I::Amp), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(Droid, Droid, Some(sessions::droid::parse_droid_file), p::cached_files, L::Parsed(l::files), Some(I::Droid), C::Sampled(message_cache::SourceFingerprint::check_droid_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(OpenClaw, OpenClaw, Some(sessions::openclaw::parse_openclaw_transcript), p::cached_files, L::Parsed(l::files), Some(I::OpenClaw), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(Pi, Pi, Some(sessions::pi::parse_pi_file), p::cached_files, L::Parsed(l::files), Some(I::Pi), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(Kimi, Kimi, None, p::kimi, L::Parsed(l::kimi), Some(I::Kimi), C::Sampled(message_cache::SourceFingerprint::check_kimi_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(Qwen, Qwen, Some(sessions::qwen::parse_qwen_file), p::cached_files, L::Parsed(l::files), Some(I::Qwen), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(RooCode, RooCode, Some(sessions::roocode::parse_roocode_file), p::cached_files, L::Parsed(l::files), Some(I::RooCode), C::Sampled(message_cache::SourceFingerprint::check_roo_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(KiloCode, KiloCode, Some(sessions::kilocode::parse_kilocode_file), p::cached_files, L::Parsed(l::files), Some(I::KiloCode), C::Sampled(message_cache::SourceFingerprint::check_roo_path_samples_only), Reprice, Append, Messages, ScannerSelected),
    row!(Mux, Mux, Some(sessions::mux::parse_mux_file), p::cached_files, L::Parsed(l::files), Some(I::Mux), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Messages, ScannerSelected),
    row!(Kilo, Kilo, Some(sessions::kilo::parse_kilo_sqlite), p::kilo, L::Parsed(l::kilo), None, C::None, Reprice, Append, Messages, ScannerSelected),
    row!(Crush, Crush, None, p::crush, L::Parsed(l::crush), None, C::None, Reprice, Append, Messages, ScannerSelected),
    row!(Hermes, Hermes, Some(sessions::hermes::parse_hermes_sqlite), p::hermes, L::Parsed(l::hermes), None, C::None, FillNonPositive, Dedup, Messages, ScannerSelected),
    row!(Copilot, Copilot, Some(sessions::copilot::parse_copilot_file), p::copilot, L::Parsed(l::copilot), Some(I::Copilot), C::Special(S::CopilotMixed), Reprice, CopilotTiered, Rows, ScannerSelected),
    row!(Goose, Goose, Some(sessions::goose::parse_goose_sqlite), p::goose, L::Parsed(l::goose), None, C::None, Reprice, Append, Messages, ScannerSelected),
    row!(Codebuff, Codebuff, Some(sessions::codebuff::parse_codebuff_file), p::cached_files, L::Parsed(l::files), Some(I::Codebuff), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ExplicitOrAll),
    row!(Antigravity, Antigravity, Some(sessions::antigravity::parse_antigravity_file), p::uncached_files, L::Parsed(l::files), None, C::None, Reprice, Append, Rows, ScannerSelected),
    row!(Zed, Zed, Some(sessions::zed::parse_zed_sqlite), p::zed, L::Parsed(l::zed), Some(I::Zed), C::Sqlite, Reprice, Append, Messages, ScannerSelected),
    row!(Kiro, Kiro, Some(sessions::kiro::parse_kiro_file), p::kiro, L::Parsed(l::kiro), Some(I::Kiro), C::Special(S::KiroMixed), Reprice, KiroSuppressSnapshots, AdditiveMessages, ScannerSelected),
    row!(Trae, Trae, None, p::trae, L::Parsed(l::trae), None, C::None, Preserve, TraeLatest, Rows, ScannerSelected),
    row!(Warp, Warp, Some(sessions::warp::parse_warp_file), p::cached_files, L::Parsed(l::files), Some(I::Warp), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Messages, ScannerSelected),
    row!(Cline, Cline, Some(sessions::cline::parse_cline_file), p::cached_files, L::Parsed(l::files), Some(I::Cline), C::Sampled(message_cache::SourceFingerprint::check_cline_path_samples_only), Reprice, Dedup, Messages, ScannerSelected),
    row!(Gjc, Gjc, Some(sessions::gjc::parse_gjc_file), p::uncached_files, L::Parsed(l::files), None, C::None, FillNonPositive, Dedup, Rows, ScannerSelected),
    row!(Grok, Grok, Some(sessions::grok::parse_grok_file), p::grok, L::Parsed(l::grok), Some(I::Grok), C::Sampled(message_cache::SourceFingerprint::check_grok_path_samples_only), Reprice, GrokPreferUnified, Messages, ScannerSelected),
    row!(Jcode, Jcode, Some(sessions::jcode::parse_jcode_file), p::cached_files, L::Parsed(l::files), Some(I::Jcode), C::Sampled(message_cache::SourceFingerprint::check_jcode_path_samples_only), Reprice, Dedup, Messages, ScannerSelected),
    row!(CommandCode, CommandCode, Some(sessions::commandcode::parse_commandcode_file), p::uncached_files, L::Parsed(l::files), None, C::None, Reprice, Append, Rows, ScannerSelected),
    row!(MiMoCode, MiMoCode, Some(sessions::micode::parse_micode_sqlite), p::micode, L::ScannedButUnparsed, Some(I::MiMoCode), C::Sqlite, FillNonAuthoritative, MiMoPreferAuthoritative, None, ScannerSelected),
    row!(AntigravityCli, AntigravityCli, Some(sessions::antigravity_cli::parse_antigravity_cli_file), p::uncached_files, L::Parsed(l::files), None, C::None, Reprice, Append, Rows, ScannerSelected),
    row!(Junie, Junie, Some(sessions::junie::parse_junie_file), p::uncached_files, L::Parsed(l::files), None, C::None, FillNonPositive, Dedup, Messages, ScannerSelected),
    row!(Zcode, Zcode, None, p::zcode, L::Parsed(l::zcode), Some(I::Zcode), C::Special(S::ZcodeMixed), Reprice, ZcodeSqliteFirst, Messages, ScannerSelected),
    row!(OpenCodeReview, OpenCodeReview, Some(sessions::opencodereview::parse_opencodereview_file), p::cached_files, L::Parsed(l::files), Some(I::OpenCodeReview), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Messages, ScannerSelected),
    row!(CodeBuddy, CodeBuddy, Some(sessions::codebuddy::parse_codebuddy_file), p::cached_files, L::Parsed(l::files), Some(I::CodeBuddy), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Dedup, Messages, ScannerSelected),
    row!(WorkBuddy, WorkBuddy, Some(sessions::workbuddy::parse_workbuddy_file), p::workbuddy, L::Parsed(l::workbuddy), Some(I::WorkBuddy), C::Special(S::WorkBuddyMixed), Reprice, WorkBuddyDetailedFirst, Messages, ScannerSelected),
    row!(DevinCli, DevinCli, Some(sessions::devin::parse_devin_cli_sqlite), p::devin_cli, L::Parsed(l::devin_cli), Some(I::DevinCli), C::Sqlite, Reprice, DevinCliPrecedence, Messages, ExplicitOrSynthetic),
    row!(DevinDesktop, DevinDesktop, None, p::devin_desktop, L::Parsed(l::devin_desktop), Some(I::DevinDesktop), C::Special(S::DevinDesktopSnapshot), Reprice, DevinDesktopAfterCli, RawBeforeMerge, ExplicitOrSynthetic),
    row!(Senpi, Senpi, Some(sessions::senpi::parse_senpi_file), p::cached_files, L::Parsed(l::files), Some(I::Senpi), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ScannerSelected),
    row!(Augment, Augment, Some(sessions::augment::parse_augment_file), p::cached_files, L::Parsed(l::files), Some(I::Augment), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Dedup, Rows, ScannerSelected),
    row!(Kimchi, Kimchi, Some(sessions::kimchi::parse_kimchi_file), p::cached_files, L::Parsed(l::files), Some(I::Kimchi), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Dedup, Rows, ScannerSelected),
    row!(Reasonix, Reasonix, Some(sessions::reasonix::parse_reasonix_file), p::cached_files, L::Parsed(l::files), Some(I::Reasonix), C::Sampled(message_cache::SourceFingerprint::check_reasonix_path_samples_only), Reprice, Append, SaturatingRawMessages, ScannerSelected),
    row!(PrimeAgent, PrimeAgent, None, p::prime, L::Parsed(l::prime), Some(I::PrimeAgent), C::Special(S::PrimeAccounting), Reprice, PrimeReconcile, Rows, ScannerSelected),
    row!(Freebuff, Codebuff, Some(sessions::freebuff::parse_freebuff_file), p::cached_files, L::Parsed(l::files), Some(I::Freebuff), C::Sampled(message_cache::SourceFingerprint::check_path_samples_only), Reprice, Append, Rows, ExplicitOrAll),
];

#[rustfmt::skip]
pub(crate) const PRICED_ORDER: [I; I::COUNT] = [I::OpenCode, I::MiMoCode, I::Claude, I::Codex, I::Copilot, I::Gemini, I::Cursor, I::Warp, I::Grok, I::Jcode, I::Amp, I::Codebuff, I::Freebuff, I::Droid, I::OpenClaw, I::Pi, I::PrimeAgent, I::Kimchi, I::Reasonix, I::Senpi, I::Augment, I::CommandCode, I::Gjc, I::Junie, I::Zcode, I::OpenCodeReview, I::Kimi, I::Qwen, I::RooCode, I::KiloCode, I::Cline, I::Mux, I::Kilo, I::Hermes, I::Goose, I::DevinCli, I::Zed, I::Kiro, I::Crush, I::Antigravity, I::AntigravityCli, I::Trae, I::CodeBuddy, I::DevinDesktop, I::WorkBuddy];

#[rustfmt::skip]
pub(crate) const LOCAL_ORDER: [I; 43] = [I::OpenCode, I::Claude, I::Codex, I::Copilot, I::Gemini, I::Amp, I::Codebuff, I::Freebuff, I::Droid, I::OpenClaw, I::Pi, I::PrimeAgent, I::Kimchi, I::Reasonix, I::Senpi, I::Augment, I::CommandCode, I::Gjc, I::Junie, I::Zcode, I::OpenCodeReview, I::Kimi, I::Qwen, I::RooCode, I::KiloCode, I::Cline, I::Mux, I::Kilo, I::Hermes, I::Goose, I::Zed, I::Kiro, I::Crush, I::Antigravity, I::AntigravityCli, I::Trae, I::Warp, I::DevinCli, I::DevinDesktop, I::CodeBuddy, I::WorkBuddy, I::Grok, I::Jcode];

pub(crate) fn spec(client: I) -> &'static D {
    let definition = &REGISTRY[client as usize];
    debug_assert_eq!(
        definition.client, client,
        "registry row index must match ClientId"
    );
    definition
}
