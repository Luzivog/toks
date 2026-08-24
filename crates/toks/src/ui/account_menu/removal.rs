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
