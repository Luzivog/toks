mod table;

use super::{local::LocalContext, priced::PricedContext};
use crate::{message_cache, ClientId, UnifiedMessage};
use std::path::Path;

pub(crate) use table::{spec, LOCAL_ORDER, PRICED_ORDER};

pub(crate) type FileParser = fn(&Path) -> Vec<UnifiedMessage>;
pub(crate) type FingerprintFn = for<'a> fn(
    &Path,
    Option<&'a message_cache::SourceFingerprint>,
) -> Option<message_cache::FingerprintStatus>;
pub(crate) type PricedHandler = for<'a> fn(&mut PricedContext<'a>, &'static ClientParserDef);
pub(crate) type LocalHandler = for<'a> fn(&mut LocalContext<'a>, &'static ClientParserDef);

#[derive(Clone, Copy)]
pub(crate) enum LocalParticipation {
    Parsed(LocalHandler),
    Disabled,
    ScannedButUnparsed,
}

#[derive(Clone, Copy)]
pub(crate) enum CachePolicy {
    None,
    Sampled(FingerprintFn),
    Sqlite,
    Special(SpecialCachePolicy),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpecialCachePolicy {
    OpenCodeMixed,
    ClaudeRetention,
    CodexIncremental,
    CopilotMixed,
    GeminiValidity,
    PrimeAccounting,
    ZcodeMixed,
    KiroMixed,
    DevinDesktopSnapshot,
    WorkBuddyMixed,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum PricingPolicy {
    Reprice,
    FillNonPositive,
    FillNonAuthoritative,
    Preserve,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergePolicy {
    Append,
    Dedup,
    OpenCodeSqliteFirst,
    MiMoPreferAuthoritative,
    ClaudeFirstWins,
    CodexFirstWins,
    CopilotTiered,
    GrokPreferUnified,
    PrimeReconcile,
    ZcodeSqliteFirst,
    KiroSuppressSnapshots,
    DevinCliPrecedence,
    DevinDesktopAfterCli,
    TraeLatest,
    WorkBuddyDetailedFirst,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum CountPolicy {
    None,
    Rows,
    Messages,
    SaturatingRawMessages,
    RawBeforeMerge,
    AdditiveMessages,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestPolicy {
    ScannerSelected,
    ExplicitOrAll,
    ExplicitOrSynthetic,
}

pub(crate) struct ClientParserDef {
    pub(crate) client: ClientId,
    pub(crate) scan_bucket: ClientId,
    pub(crate) parser: Option<FileParser>,
    pub(crate) priced: PricedHandler,
    pub(crate) local: LocalParticipation,
    pub(crate) cache_identity: Option<ClientId>,
    pub(crate) cache: CachePolicy,
    pub(crate) pricing: PricingPolicy,
    pub(crate) merge: MergePolicy,
    pub(crate) count_bucket: Option<ClientId>,
    pub(crate) count: CountPolicy,
    pub(crate) request: RequestPolicy,
}

impl ClientParserDef {
    pub(crate) fn assert_coherent(&self) {
        let identity_matches = match self.cache {
            CachePolicy::None => self.cache_identity.is_none(),
            CachePolicy::Sampled(_) | CachePolicy::Sqlite | CachePolicy::Special(_) => {
                self.cache_identity == Some(self.client)
            }
        };
        debug_assert!(
            identity_matches,
            "registry cache identity must match its client"
        );

        let special_matches = match self.cache {
            CachePolicy::Special(policy) => match policy {
                SpecialCachePolicy::OpenCodeMixed => self.merge == MergePolicy::OpenCodeSqliteFirst,
                SpecialCachePolicy::ClaudeRetention => self.merge == MergePolicy::ClaudeFirstWins,
                SpecialCachePolicy::CodexIncremental => self.merge == MergePolicy::CodexFirstWins,
                SpecialCachePolicy::CopilotMixed => self.merge == MergePolicy::CopilotTiered,
                SpecialCachePolicy::GeminiValidity => self.client == ClientId::Gemini,
                SpecialCachePolicy::PrimeAccounting => self.merge == MergePolicy::PrimeReconcile,
                SpecialCachePolicy::ZcodeMixed => self.merge == MergePolicy::ZcodeSqliteFirst,
                SpecialCachePolicy::KiroMixed => self.merge == MergePolicy::KiroSuppressSnapshots,
                SpecialCachePolicy::DevinDesktopSnapshot => {
                    self.merge == MergePolicy::DevinDesktopAfterCli
                }
                SpecialCachePolicy::WorkBuddyMixed => {
                    self.merge == MergePolicy::WorkBuddyDetailedFirst
                }
            },
            CachePolicy::None | CachePolicy::Sampled(_) | CachePolicy::Sqlite => true,
        };
        debug_assert!(
            special_matches,
            "registry special cache and merge policies must agree"
        );
    }
}
