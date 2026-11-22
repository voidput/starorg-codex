//! UEX market data in the real backend: the token (Settings -> Network) and "Fetch now",
//! over [`starorg_net::uex`].
//!
//! - Nothing is sent while "Download UEX market data" is off (`network-off`).
//! - The token lives in [`crate::tokens`] (keyring entry `uex`, or `uex.token`); `app.json`
//!   keeps only where it is, so the status never touches the keyring. The token is never
//!   returned, logged or put in an error.
//! - All three files land in the UEX folder `obtain_status` reports (`<cache>/mcp/uex`), the one
//!   WHERE TO GET, the Sources card and the MCP server read.

use super::convert::err;
use super::*;
use crate::tokens::Slot;
use starorg_net::uex::{self, UexError, UexTransport};

/// What "Fetch now" needs from outside; tests swap the transport.
pub struct UexDeps {
    pub transport: Arc<dyn UexTransport>,
    /// Held while a fetch runs: a second one is `busy`.
    pub running: Mutex<()>,
}

impl Default for UexDeps {
    fn default() -> Self {
        Self::with(Arc::new(uex::HttpTransport::new()))
    }
}

impl UexDeps {
    pub fn with(transport: Arc<dyn UexTransport>) -> Self {
        Self {
            transport,
            running: Mutex::new(()),
        }
    }
}

fn uex_off() -> AppError {
    err(
        ErrorCode::NetworkOff,
        "Turn on \"Download UEX market data\" under Settings -> Network first.",
    )
}

fn store_label(k: TokenStoreKind) -> &'static str {
    match k {
        TokenStoreKind::Keyring => "keyring",
        TokenStoreKind::File => "file",
    }
}

impl RealBackend {
    /// `<cache>/mcp/uex`.
    pub(super) fn uex_dir(&self) -> PathBuf {
        starorg_mcp::tools::uex_dir(&self.dirs().cache.join("mcp"))
    }

    pub(super) fn uex_token_view(&self) -> UexTokenStatus {
        let store = self.app_file().uex_token;
        UexTokenStatus {
            set: store.is_some(),
            store,
        }
    }

    pub(super) fn save_uex_token(&self, req: UexTokenRequest) -> CmdResult<UexTokenStatus> {
        let token = uex::clean_token(req.token.as_deref().unwrap_or(""))
            .map_err(|m| err(ErrorCode::Invalid, m))?;
        let tokens = self.starorg.tokens.clone();
        let mut a = self.app_file();
        match token {
            Some(t) => {
                let kind = tokens
                    .set(Slot::Uex, &t)
                    .map_err(|e| err(ErrorCode::Internal, format!("UEX token not saved: {e}")))?;
                a.uex_token = Some(kind);
                self.save_app_file(&a)?;
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Uex),
                    None,
                    &format!("UEX token saved ({})", store_label(kind)),
                );
            }
            None => {
                tokens
                    .delete(Slot::Uex)
                    .map_err(|e| err(ErrorCode::Internal, format!("UEX token not removed: {e}")))?;
                a.uex_token = None;
                self.save_app_file(&a)?;
                self.log(
                    LogLevel::Info,
                    Some(JobKind::Uex),
                    None,
                    "UEX token removed",
                );
            }
        }
        Ok(self.uex_token_view())
    }

    /// The saved token, when the app file says there is one.
    fn uex_token(&self) -> CmdResult<Option<String>> {
        if self.app_file().uex_token.is_none() {
            return Ok(None);
        }
        match self.starorg.tokens.get(Slot::Uex) {
            Ok(t) => Ok(t.map(|(t, _)| t)),
            Err(e) => Err(err(
                ErrorCode::Internal,
                format!("cannot read the UEX token: {e}"),
            )),
        }
    }

    pub(super) fn run_fetch_uex(&self) -> CmdResult<ObtainStatus> {
        if !self.settings().network.uex {
            return Err(uex_off());
        }
        let Ok(_running) = self.uex.running.try_lock() else {
            return Err(err(
                ErrorCode::Busy,
                "UEX market data is already being downloaded",
            ));
        };
        let token = self.uex_token()?;
        let job = self.job_start(JobKind::Uex, None, 1, "Downloading UEX market data");
        let started = std::time::Instant::now();
        let r = uex::fetch_market(
            self.uex.transport.as_ref(),
            &self.uex_dir(),
            token.as_deref(),
        );
        match r {
            Ok(rep) => {
                let msg = format!(
                    "Downloaded UEX market data: {} prices for {} items, {} for {} commodities, at {} terminals ({:.1} MB, {:.1} s)",
                    group(rep.prices),
                    group(rep.items),
                    group(rep.commodity_prices),
                    group(rep.commodities),
                    group(rep.terminals),
                    rep.bytes as f64 / 1_000_000.0,
                    started.elapsed().as_secs_f64()
                );
                self.log(LogLevel::Info, Some(JobKind::Uex), None, &msg);
                self.job_end(job, JobPhase::Done, &msg, None);
                Ok(self.obtain_status_now())
            }
            Err(e) => {
                let code = match e {
                    UexError::Token(_) => ErrorCode::Invalid,
                    _ => ErrorCode::Internal,
                };
                // UexError text never holds the token (starorg_net::uex).
                let msg = e.to_string();
                self.log(LogLevel::Warn, Some(JobKind::Uex), None, &msg);
                self.job_end(job, JobPhase::Failed, &msg, None);
                Err(err(code, msg))
            }
        }
    }

    pub(super) fn obtain_status_now(&self) -> ObtainStatus {
        obtain_status_in(&self.uex_dir())
    }
}

/// `24,146`.
fn group(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn groups_thousands() {
        assert_eq!(super::group(7), "7");
        assert_eq!(super::group(826), "826");
        assert_eq!(super::group(2438), "2,438");
        assert_eq!(super::group(1_234_567), "1,234,567");
    }
}
