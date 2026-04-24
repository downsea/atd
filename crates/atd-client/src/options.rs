use atd_protocol::{ToolTier, ToolVisibility};

#[derive(Debug, Clone, Default)]
pub struct DiscoverFilter {
    pub tier: Option<ToolTier>,
    pub visibility: Option<ToolVisibility>,
    pub domain: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct CallOptions {
    pub dry_run: bool,
    pub preferred_binding: Option<atd_protocol::BindingProtocol>,
}
