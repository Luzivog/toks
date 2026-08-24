use toks_core::accounts::AccountOrigin;

pub(super) fn confirmation_body(origin: AccountOrigin, provider: &str) -> String {
    let action = match origin {
        AccountOrigin::Managed => {
            "Its Toks-managed profile and saved credentials will be removed from this device."
        }
        AccountOrigin::Current => {
            "It will be hidden in Toks. Your current CLI credentials will not be changed."
        }
        AccountOrigin::Mixed => {
            "Toks-managed profiles will be removed and the current CLI account will be hidden. Your current CLI credentials will not be changed."
        }
        AccountOrigin::Unknown => {
            "It will be removed from Toks. Your provider credentials will not be changed."
        }
    };
    format!("{action} Your {provider} usage history will be kept.")
}

#[cfg(test)]
mod tests {
    use super::{confirmation_body, AccountOrigin};

    #[test]
    fn removal_copy_matches_capability() {
        assert!(confirmation_body(AccountOrigin::Managed, "Codex")
            .contains("saved credentials will be removed"));
        assert!(confirmation_body(AccountOrigin::Current, "Codex")
            .contains("credentials will not be changed"));
        assert!(confirmation_body(AccountOrigin::Mixed, "Claude Code")
            .contains("profiles will be removed"));
    }
}
