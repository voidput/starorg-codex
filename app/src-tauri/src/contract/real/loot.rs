//! The Loot screen (`loot_list`, `loot_detail`): the Sources card's tools of the install
//! ([`RealBackend::origin_tools`]), so the screen shares their dataset cache and memory
//! limit. Reads only what is cached ("Read game data" extracts it); the index is built once
//! per build and kept in that store's per-build cache.

use super::*;

impl RealBackend {
    /// The install's tools and the channel name, when `target` resolves.
    fn loot_tools(
        &self,
        target: Option<&ChannelRef>,
    ) -> Option<(Arc<starorg_mcp::tools::Tools>, String)> {
        let t = target?;
        let f = self.resolve(t).ok()?;
        let tools = self.origin_tools(&f).ok()?;
        Some((tools, t.channel.clone()))
    }

    pub(super) fn loot_list_of(&self, req: LootListRequest) -> LootList {
        match self.loot_tools(req.target.as_ref()) {
            Some((tools, ch)) => tools.loot_list(Some(&ch), req.view),
            None => starorg_mcp::loot::missing_list(req.view),
        }
    }

    pub(super) fn loot_detail_of(&self, req: LootDetailRequest) -> LootDetail {
        match self.loot_tools(req.target.as_ref()) {
            Some((tools, ch)) => {
                tools.loot_detail(Some(&ch), req.view, &req.id, req.filter.as_deref())
            }
            None => starorg_mcp::loot::missing_detail(req.view),
        }
    }
}
