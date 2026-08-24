#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathRoot {
    Home,
    ReasonixHome,
    XdgData,
    Config,
    EnvVar {
        var: &'static str,
        fallback_relative: &'static str,
    },
}

/// Join `home_dir` and a `/`-joined relative literal with native separators
/// throughout. `Path::join` only normalizes the junction; the relative half's
/// own `/` separators would survive untouched on Windows (#1048).
fn join_home(home_dir: &str, relative: &str) -> String {
    let mut path = std::path::PathBuf::from(home_dir);
    for component in std::path::Path::new(relative).components() {
        path.push(component.as_os_str());
    }
    path.to_string_lossy().into_owned()
}

impl PathRoot {
    pub fn resolve_with_env_strategy(&self, home_dir: &str, use_env_roots: bool) -> String {
        match self {
            PathRoot::Home => home_dir.to_string(),
            PathRoot::ReasonixHome => {
                if use_env_roots {
                    if let Some(state_home) =
                        clean_reasonix_env_dir("REASONIX_STATE_HOME", home_dir)
                    {
                        return state_home;
                    }
                    if let Some(home) = clean_reasonix_env_dir("REASONIX_HOME", home_dir) {
                        return home;
                    }
                }
                #[cfg(target_os = "windows")]
                {
                    if use_env_roots {
                        if let Some(config_dir) = dirs::config_dir() {
                            return config_dir.join("reasonix").to_string_lossy().into_owned();
                        }
                    }
                    return std::path::Path::new(home_dir)
                        .join("AppData")
                        .join("Roaming")
                        .join("reasonix")
                        .to_string_lossy()
                        .into_owned();
                }
                #[cfg(not(target_os = "windows"))]
                {
                    join_home(home_dir, ".reasonix")
                }
            }
            PathRoot::XdgData => {
                if use_env_roots {
                    std::env::var("XDG_DATA_HOME")
                        .unwrap_or_else(|_| join_home(home_dir, ".local/share"))
                } else {
                    join_home(home_dir, ".local/share")
                }
            }
            PathRoot::Config => {
                if use_env_roots {
                    if let Some(custom) =
                        crate::paths::renamed_env_var_os("TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR")
                    {
                        return custom.to_string_lossy().into_owned();
                    }

                    #[cfg(target_os = "linux")]
                    if let Ok(xdg_config_home) = std::env::var("XDG_CONFIG_HOME") {
                        return format!("{xdg_config_home}/toks");
                    }

                    // Match paths::get_config_dir() so default Windows scans
                    // read the same %APPDATA% root used by cache writers.
                    #[cfg(target_os = "windows")]
                    if let Some(dir) = dirs::config_dir() {
                        return dir.join("toks").to_string_lossy().into_owned();
                    }
                }

                #[cfg(target_os = "windows")]
                if !use_env_roots {
                    return std::path::Path::new(home_dir)
                        .join("AppData/Roaming/toks")
                        .to_string_lossy()
                        .into_owned();
                }

                join_home(home_dir, ".config/toks")
            }
            PathRoot::EnvVar {
                var,
                fallback_relative,
            } => {
                if use_env_roots {
                    let val = std::env::var(var).unwrap_or_default();
                    if val.trim().is_empty() {
                        join_home(home_dir, fallback_relative)
                    } else {
                        val
                    }
                } else {
                    join_home(home_dir, fallback_relative)
                }
            }
        }
    }

    pub fn resolve(&self, home_dir: &str) -> String {
        self.resolve_with_env_strategy(home_dir, true)
    }
}

fn clean_reasonix_env_dir(name: &str, home_dir: &str) -> Option<String> {
    let value = std::env::var(name).ok()?;
    let value = expand_reasonix_env_vars(value.trim());
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let path = if value == "~" {
        std::path::PathBuf::from(home_dir)
    } else if let Some(relative) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        std::path::PathBuf::from(join_home(home_dir, relative))
    } else {
        std::path::PathBuf::from(value)
    };

    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().ok()?.join(path)
    };
    Some(path.to_string_lossy().into_owned())
}

// Match Reasonix's config expansion for ${VAR} and ${VAR:-default}. This must
// happen before tilde and relative-path handling because either expansion may
// produce one of those forms.
fn expand_reasonix_env_vars(value: &str) -> String {
    let mut expanded = String::with_capacity(value.len());
    let mut remainder = value;

    while let Some(start) = remainder.find("${") {
        expanded.push_str(&remainder[..start]);
        let reference = &remainder[start + 2..];
        let Some(end) = reference.find('}') else {
            expanded.push_str(&remainder[start..]);
            return expanded;
        };

        let expression = &reference[..end];
        let (name, default) = expression
            .split_once(":-")
            .map_or((expression, None), |(name, default)| (name, Some(default)));
        let is_valid_name = name.chars().enumerate().all(|(index, character)| {
            (character == '_' || character.is_ascii_alphabetic())
                || (index > 0 && character.is_ascii_digit())
        });

        if is_valid_name && !name.is_empty() {
            match std::env::var(name) {
                Ok(env_value) if !env_value.is_empty() => expanded.push_str(&env_value),
                _ => expanded.push_str(default.unwrap_or_default()),
            }
        } else {
            expanded.push_str("${");
            expanded.push_str(expression);
            expanded.push('}');
        }
        remainder = &reference[end + 1..];
    }

    expanded.push_str(remainder);
    expanded
}

#[derive(Debug, Clone)]
pub struct ClientDef {
    pub id: &'static str,
    pub root: PathRoot,
    pub relative_path: &'static str,
    pub pattern: &'static str,
    pub headless: bool,
    pub parse_local: bool,
    pub submit_default: bool,
}

impl ClientDef {
    pub fn resolve_path_with_env_strategy(&self, home_dir: &str, use_env_roots: bool) -> String {
        let root = self.root.resolve_with_env_strategy(home_dir, use_env_roots);
        if self.relative_path.is_empty() {
            return root;
        }
        // Join component-by-component instead of hand-concatenating
        // "{root}/{relative}": a hardcoded `/` — and even `Path::join`, which
        // only normalizes the junction — leaves the relative half's own `/`
        // separators untouched on Windows, producing mixed-separator paths
        // (`C:\Users\me/.codex/sessions`) that reached user-facing
        // `clients --json` output (#1048). Pushing each component yields
        // native separators throughout on every platform.
        let mut path = std::path::PathBuf::from(&root);
        for component in std::path::Path::new(self.relative_path).components() {
            path.push(component.as_os_str());
        }
        path.to_string_lossy().into_owned()
    }

    pub fn resolve_path(&self, home_dir: &str) -> String {
        self.resolve_path_with_env_strategy(home_dir, true)
    }
}

macro_rules! define_clients {
    ( $( $variant:ident = $index:expr => { id: $id:expr, display: $display:expr, logo: $logo:expr, root: $root:expr, relative: $rel:expr, pattern: $pat:expr, headless: $hl:expr, parse_local: $pl:expr, submit_default: $sd:expr } ),+ $(,)? ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[repr(usize)]
        pub enum ClientId {
            $( $variant = $index ),+
        }

        impl ClientId {
            pub const COUNT: usize = [ $( $index ),+ ].len();
            pub const ALL: [ClientId; Self::COUNT] = [ $( ClientId::$variant ),+ ];

            pub fn data(&self) -> &'static ClientDef {
                &CLIENTS[*self as usize]
            }

            pub fn as_str(&self) -> &'static str {
                self.data().id
            }

            pub fn display_name(&self) -> &'static str {
                CLIENT_DISPLAY_NAMES[*self as usize]
            }

            pub fn logo_url(&self) -> Option<&'static str> {
                CLIENT_LOGO_URLS[*self as usize]
            }

            pub fn file_pattern(&self) -> &'static str {
                self.data().pattern
            }

            pub fn supports_headless(&self) -> bool {
                self.data().headless
            }

            pub fn parse_local(&self) -> bool {
                self.data().parse_local
            }

            pub fn submit_default(&self) -> bool {
                self.data().submit_default
            }

            pub fn iter() -> impl Iterator<Item = ClientId> {
                Self::ALL.iter().copied()
            }

            #[allow(clippy::should_implement_trait)]
            pub fn from_str(s: &str) -> Option<ClientId> {
                Self::ALL.iter().copied().find(|c| c.as_str() == s)
            }
        }

        pub const CLIENTS: [ClientDef; ClientId::COUNT] = [
            $( ClientDef {
                id: $id,
                root: $root,
                relative_path: $rel,
                pattern: $pat,
                headless: $hl,
                parse_local: $pl,
                submit_default: $sd,
            } ),+
        ];

        // Display metadata is generated from the same exhaustive registry but
        // kept out of public ClientDef so downstream struct literals remain
        // source-compatible.
        const CLIENT_DISPLAY_NAMES: [&str; ClientId::COUNT] = [ $( $display ),+ ];
        const CLIENT_LOGO_URLS: [Option<&str>; ClientId::COUNT] = [ $( $logo ),+ ];

        const _: () = {
            let mut i = 0;
            $(
                assert!($index == i, "ClientId indices must be sequential");
                i += 1;
                let _ = i;
            )+
        };
    };
}

define_clients!(
    OpenCode = 0 => {
        id: "opencode",
        display: "OpenCode",
        logo: Some("https://tokscope.ai/assets/logos/opencode.png"),root: PathRoot::XdgData,
        relative: "opencode/storage/message",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Claude = 1 => {
        id: "claude",
        display: "Claude Code",
        logo: Some("https://tokscope.ai/assets/logos/claude.jpg"),root: PathRoot::Home,
        relative: ".claude/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Codex = 2 => {
        id: "codex",
        display: "Codex CLI",
        logo: Some("https://tokscope.ai/assets/logos/openai.jpg"),root: PathRoot::EnvVar {
            var: "CODEX_HOME",
            fallback_relative: ".codex",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: true,
        parse_local: true,
        submit_default: true
    },
    Cursor = 3 => {
        id: "cursor",
        display: "Cursor IDE",
        logo: Some("https://tokscope.ai/assets/logos/cursor.jpg"),root: PathRoot::Config,
        relative: "cursor-cache",
        pattern: "usage*.csv",
        headless: false,
        parse_local: false,
        submit_default: true
    },
    Gemini = 4 => {
        id: "gemini",
        display: "Gemini CLI",
        logo: Some("https://tokscope.ai/assets/logos/gemini.png"),root: PathRoot::EnvVar {
            var: "GEMINI_CLI_HOME",
            fallback_relative: ".gemini",
        },
        relative: "tmp",
        pattern: "*.json|*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Amp = 5 => {
        id: "amp",
        display: "Amp",
        logo: Some("https://tokscope.ai/assets/logos/amp.png"),root: PathRoot::XdgData,
        relative: "amp/threads",
        pattern: "T-*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Droid = 6 => {
        id: "droid",
        display: "Droid",
        logo: Some("https://tokscope.ai/assets/logos/droid.png"),root: PathRoot::Home,
        relative: ".factory/sessions",
        pattern: "*.settings.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    OpenClaw = 7 => {
        id: "openclaw",
        display: "OpenClaw",
        logo: Some("https://tokscope.ai/assets/logos/openclaw.png"),root: PathRoot::Home,
        relative: ".openclaw/agents",
        pattern: "*.jsonl*",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Pi = 8 => {
        id: "pi",
        display: "Pi",
        logo: Some("https://tokscope.ai/assets/logos/pi.png"),root: PathRoot::Home,
        relative: ".pi/agent/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kimi = 9 => {
        id: "kimi",
        display: "Kimi CLI",
        logo: Some("https://tokscope.ai/assets/logos/kimi.png"),root: PathRoot::Home,
        relative: ".kimi/sessions",
        pattern: "wire.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Qwen = 10 => {
        id: "qwen",
        display: "Qwen CLI",
        logo: Some("https://tokscope.ai/assets/logos/qwen.png"),root: PathRoot::Home,
        relative: ".qwen/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    RooCode = 11 => {
        id: "roocode",
        display: "Roo Code",
        logo: Some("https://tokscope.ai/assets/logos/roocode.png"),root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/rooveterinaryinc.roo-cline/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    KiloCode = 12 => {
        id: "kilocode",
        display: "Kilo Code",
        logo: Some("https://tokscope.ai/assets/logos/kilocode.png"),root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/kilocode.kilo-code/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Mux = 13 => {
        id: "mux",
        display: "Mux",
        logo: Some("https://tokscope.ai/assets/logos/mux.png"),root: PathRoot::Home,
        relative: ".mux/sessions",
        pattern: "session-usage.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kilo = 14 => {
        id: "kilo",
        display: "Kilo CLI",
        logo: Some("https://tokscope.ai/assets/logos/kilocode.png"),root: PathRoot::XdgData,
        relative: "kilo/kilo.db",
        pattern: "kilo.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Crush = 15 => {
        id: "crush",
        display: "Crush",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/6b483d0f2de3717266dec8faed13acd067f90ff3/.github/assets/client-crush.png"),root: PathRoot::XdgData,
        relative: "crush/projects.json",
        pattern: "projects.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Hermes = 16 => {
        id: "hermes",
        display: "Hermes Agent",
        logo: Some("https://tokscope.ai/assets/logos/hermes.png"),root: PathRoot::EnvVar {
            var: "HERMES_HOME",
            fallback_relative: ".hermes",
        },
        relative: "state.db",
        pattern: "state.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Copilot = 17 => {
        id: "copilot",
        display: "Copilot CLI",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-copilot.jpg"),root: PathRoot::Home,
        relative: ".copilot/otel",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Goose = 18 => {
        id: "goose",
        display: "Goose",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-goose.png"),root: PathRoot::XdgData,
        relative: "goose/sessions/sessions.db",
        pattern: "sessions.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Codebuff = 19 => {
        id: "codebuff",
        display: "Codebuff",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-codebuff.png"),root: PathRoot::EnvVar {
            var: "CODEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        },
        relative: "projects",
        pattern: "chat-messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Antigravity = 20 => {
        id: "antigravity",
        display: "Antigravity",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-antigravity.png"),root: PathRoot::Config,
        relative: "antigravity-cache/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Zed = 21 => {
        id: "zed",
        display: "Zed Agent",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-zed.webp"),root: PathRoot::XdgData,
        relative: "zed/threads/threads.db",
        pattern: "threads.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Kiro = 22 => {
        id: "kiro",
        display: "Kiro",
        logo: None,root: PathRoot::Home,
        relative: ".kiro/sessions/cli",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Trae = 23 => {
        id: "trae",
        display: "Trae",
        logo: None,root: PathRoot::Config,
        relative: "trae-cache/sessions",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Warp = 24 => {
        id: "warp",
        display: "Warp",
        logo: None,root: PathRoot::Config,
        relative: "warp-cache",
        pattern: "usage*.json",
        headless: false,
        parse_local: true,
        submit_default: false
    },
    Cline = 25 => {
        id: "cline",
        display: "Cline",
        logo: None,root: PathRoot::Home,
        relative: ".config/Code/User/globalStorage/saoudrizwan.claude-dev/tasks",
        pattern: "ui_messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Gjc = 26 => {
        id: "gjc",
        display: "Gajae-Code",
        logo: None,root: PathRoot::EnvVar {
            var: "GJC_CODING_AGENT_DIR",
            fallback_relative: ".gjc/agent",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Grok = 27 => {
        id: "grok",
        display: "Grok Build",
        logo: None,root: PathRoot::EnvVar {
            var: "GROK_HOME",
            fallback_relative: ".grok",
        },
        relative: "sessions",
        pattern: "updates.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Jcode = 28 => {
        id: "jcode",
        display: "Jcode",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-jcode.png"),root: PathRoot::EnvVar {
            var: "JCODE_HOME",
            fallback_relative: ".jcode",
        },
        relative: "sessions",
        pattern: "session_*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    CommandCode = 29 => {
        id: "commandcode",
        display: "Command Code",
        logo: None,root: PathRoot::Home,
        relative: ".commandcode/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    MiMoCode = 30 => {
        id: "micode",
        display: "MiMo Code",
        logo: None,root: PathRoot::XdgData,
        relative: "mimocode",
        pattern: "*.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Antigravity CLI stores each conversation as a SQLite `.db` under
    // `~/.gemini/antigravity-cli/conversations/`. Unlike the IDE-backed
    // `Antigravity` client (which pulls usage from a running language server
    // over RPC and caches JSONL under the config dir), the CLI usage sits on
    // disk and is read directly — no RPC, no `antigravity sync` needed. Honors
    // `GEMINI_CLI_HOME` so a relocated Gemini home is picked up.
    AntigravityCli = 31 => {
        id: "antigravity-cli",
        display: "Antigravity CLI",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-antigravity.png"),root: PathRoot::EnvVar {
            var: "GEMINI_CLI_HOME",
            fallback_relative: ".gemini",
        },
        relative: "antigravity-cli/conversations",
        pattern: "*.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Junie = 32 => {
        id: "junie",
        display: "Junie",
        logo: Some("https://github.com/JetBrains.png"),root: PathRoot::Home,
        relative: ".junie/sessions",
        pattern: "events.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    Zcode = 33 => {
        id: "zcode",
        display: "ZCode",
        logo: None,root: PathRoot::Home,
        relative: ".zcode/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    OpenCodeReview = 34 => {
        id: "opencodereview",
        display: "OpenCodeReview",
        logo: None,root: PathRoot::Home,
        relative: ".opencodereview/sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    CodeBuddy = 35 => {
        id: "codebuddy",
        display: "CodeBuddy",
        logo: None,root: PathRoot::Home,
        relative: ".codebuddy/projects",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    WorkBuddy = 36 => {
        id: "workbuddy",
        display: "WorkBuddy",
        logo: None,root: PathRoot::Home,
        relative: ".workbuddy",
        pattern: "workbuddy.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    DevinCli = 37 => {
        id: "devin-cli",
        display: "Devin CLI",
        logo: None,root: PathRoot::XdgData,
        relative: "devin/cli/sessions.db",
        pattern: "sessions.db",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    DevinDesktop = 38 => {
        id: "devin-desktop",
        display: "Devin Desktop",
        logo: None,root: PathRoot::Home,
        relative: "Library/Application Support/Devin/User/acp-events",
        pattern: "*.ndjson",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Senpi (OmO Native) is a pi-mono descendant and writes the same session
    // JSONL under `<agent dir>/sessions/<encoded-cwd>/*.jsonl`. The agent dir
    // honors `SENPI_CODING_AGENT_DIR` and otherwise defaults to `~/.senpi/agent`,
    // mirroring the `gjc` layout.
    Senpi = 39 => {
        id: "senpi",
        display: "Senpi (OmO Native)",
        logo: None,root: PathRoot::EnvVar {
            var: "SENPI_CODING_AGENT_DIR",
            fallback_relative: ".senpi/agent",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Augment Code / Auggie CLI stores per-session JSON snapshots under
    // `~/.augment/sessions/<sessionId>.json` with per-turn token_usage on
    // exchange.response_nodes.
    Augment = 40 => {
        id: "augment",
        display: "Augment Code",
        logo: Some("https://github.com/augmentcode.png"),root: PathRoot::Home,
        relative: ".augment/sessions",
        pattern: "*.json",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Kimchi Coding uses the Pi session format under its own agent directory.
    // The launcher exposes KIMCHI_CODING_AGENT_DIR for relocated installs.
    Kimchi = 41 => {
        id: "kimchi",
        display: "Kimchi",
        logo: Some("https://github.com/getkimchi.png"),root: PathRoot::EnvVar {
            var: "KIMCHI_CODING_AGENT_DIR",
            fallback_relative: ".config/kimchi/harness",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Reasonix stores authoritative provider usage as daily append-only JSONL
    // records under `<state root>/stats/`. Transcript JSONL is intentionally
    // excluded: it lacks exact token counters and overlaps these records.
    Reasonix = 42 => {
        id: "reasonix",
        display: "Reasonix",
        logo: None,root: PathRoot::ReasonixHome,
        relative: "stats",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Prime Agent uses the Pi append-only JSONL session format. Root sessions
    // live in `<agent dir>/sessions`; RLM child sessions are discovered from
    // the sibling `session-artifacts` tree by the scanner.
    PrimeAgent = 43 => {
        id: "prime-agent",
        display: "Prime Agent",
        logo: Some("https://github.com/PrimeIntellect-ai.png"),root: PathRoot::EnvVar {
            var: "PRIME_AGENT_CODING_AGENT_DIR",
            fallback_relative: ".prime/agent",
        },
        relative: "sessions",
        pattern: "*.jsonl",
        headless: false,
        parse_local: true,
        submit_default: true
    },
    // Freebuff is a compile-time build variant of the Codebuff CLI, so it
    // writes to the same `~/.config/manicode*` tree and the same
    // `projects/<project>/chats/<chatId>/chat-messages.json` layout. The two
    // products are told apart per chat by the persisted root agent id, not by
    // location (see `sessions::freebuff`).
    Freebuff = 44 => {
        id: "freebuff",
        display: "Freebuff",
        logo: Some("https://raw.githubusercontent.com/junhoyeo/tokscope/main/.github/assets/client-freebuff.png"),root: PathRoot::EnvVar {
            var: "FREEBUFF_DATA_DIR",
            fallback_relative: ".config/manicode",
        },
        relative: "projects",
        pattern: "chat-messages.json",
        headless: false,
        parse_local: true,
        submit_default: true
    }
);

pub struct ClientCounts {
    counts: [i32; ClientId::COUNT],
}

impl ClientCounts {
    pub fn new() -> Self {
        Self {
            counts: [0; ClientId::COUNT],
        }
    }

    pub fn get(&self, client: ClientId) -> i32 {
        self.counts[client as usize]
    }

    pub fn set(&mut self, client: ClientId, value: i32) {
        self.counts[client as usize] = value;
    }

    pub fn add(&mut self, client: ClientId, value: i32) {
        self.counts[client as usize] += value;
    }
}

impl Default for ClientCounts {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod tests;
