use super::super::cost::lookup_result_if_usable;
use super::super::{LookupResult, PricingLookup};

impl PricingLookup {
    pub(in crate::pricing::lookup) fn exact_match_cursor(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        if let Some(key) = self.cursor_lower.get(model_id) {
            return lookup_result_if_usable(self.cursor.get(key).unwrap(), "Cursor", key);
        }
        if let Some(model_part) = model_id.split('/').next_back() {
            if model_part != model_id {
                if let Some(key) = self.cursor_lower.get(model_part) {
                    return lookup_result_if_usable(self.cursor.get(key).unwrap(), "Cursor", key);
                }
            }
        }
        None
    }

    pub(in crate::pricing::lookup) fn exact_match_sakana(
        &self,
        model_id: &str,
    ) -> Option<LookupResult> {
        if let Some(key) = self.sakana_lower.get(model_id) {
            return lookup_result_if_usable(self.sakana.get(key).unwrap(), "Sakana", key);
        }
        if let Some(model_part) = model_id.split('/').next_back() {
            if model_part != model_id {
                if let Some(key) = self.sakana_lower.get(model_part) {
                    return lookup_result_if_usable(self.sakana.get(key).unwrap(), "Sakana", key);
                }
            }
        }
        None
    }
}
