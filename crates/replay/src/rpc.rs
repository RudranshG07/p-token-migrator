use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose, Engine as _};
use serde::Deserialize;
use serde_json::{json, Value};
use solana_sdk::{account::Account, pubkey::Pubkey};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Pluggable transport so tests can mock RPC responses without hitting the
/// network. Production uses [`HttpTransport`]; the offline test plugs in a
/// canned `Vec<(method, response)>` transport.
pub trait Transport: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Result<Value>;
}

pub struct HttpTransport {
    url: String,
    agent: ureq::Agent,
    next_id: AtomicU64,
}

impl HttpTransport {
    pub fn new(url: impl Into<String>) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(30))
            .build();
        Self {
            url: url.into(),
            agent,
            next_id: AtomicU64::new(1),
        }
    }
}

impl Transport for HttpTransport {
    fn call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let body = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Naive exponential backoff for 429s and transient transport errors.
        // Real callers should use a paid RPC; this is just so an occasional
        // 429 from a paid provider doesn't fail the whole run.
        let mut delay = Duration::from_millis(400);
        let mut attempt = 0;
        let resp_value = loop {
            attempt += 1;
            let send_result = self
                .agent
                .post(&self.url)
                .set("Content-Type", "application/json")
                .send_json(body.clone());
            match send_result {
                Ok(resp) => {
                    break resp
                        .into_json::<Value>()
                        .with_context(|| format!("decode RPC response method={}", method))?;
                }
                Err(ureq::Error::Status(code, _)) if code == 429 && attempt < 4 => {
                    std::thread::sleep(delay);
                    delay = delay.saturating_mul(2);
                    continue;
                }
                Err(e) => {
                    return Err(anyhow!(e)).with_context(|| {
                        format!("RPC POST {} method={}", self.url, method)
                    });
                }
            }
        };
        if let Some(err) = resp_value.get("error") {
            bail!("RPC {} error: {}", method, err);
        }
        resp_value
            .get("result")
            .cloned()
            .ok_or_else(|| anyhow!("RPC {} returned no result", method))
    }
}

pub struct RpcClient<T: Transport = HttpTransport> {
    pub transport: T,
}

impl RpcClient<HttpTransport> {
    pub fn http(url: impl Into<String>) -> Self {
        Self {
            transport: HttpTransport::new(url),
        }
    }
}

impl<T: Transport> RpcClient<T> {
    pub fn new(transport: T) -> Self {
        Self { transport }
    }

    pub fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        limit: usize,
    ) -> Result<Vec<SignatureRef>> {
        let params = json!([address.to_string(), { "limit": limit }]);
        let value = self
            .transport
            .call("getSignaturesForAddress", params)
            .context("getSignaturesForAddress")?;
        serde_json::from_value(value).context("decode getSignaturesForAddress")
    }

    pub fn get_transaction(&self, signature: &str) -> Result<Option<TxResponse>> {
        let params = json!([
            signature,
            { "encoding": "json", "maxSupportedTransactionVersion": 0, "commitment": "finalized" }
        ]);
        let value = self
            .transport
            .call("getTransaction", params)
            .context("getTransaction")?;
        if value.is_null() {
            return Ok(None);
        }
        let parsed: TxResponse = serde_json::from_value(value).context("decode getTransaction")?;
        Ok(Some(parsed))
    }

    pub fn get_multiple_accounts(&self, keys: &[Pubkey]) -> Result<Vec<Option<Account>>> {
        if keys.is_empty() {
            return Ok(vec![]);
        }
        let strs: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        let params = json!([strs, { "encoding": "base64", "commitment": "finalized" }]);
        let value = self
            .transport
            .call("getMultipleAccounts", params)
            .context("getMultipleAccounts")?;
        let raw: AccountsResponse =
            serde_json::from_value(value).context("decode getMultipleAccounts")?;
        let mut out = Vec::with_capacity(raw.value.len());
        for entry in raw.value {
            out.push(match entry {
                Some(info) => Some(info.try_into_account()?),
                None => None,
            });
        }
        Ok(out)
    }

    pub fn get_account_info(&self, key: &Pubkey) -> Result<Option<Account>> {
        let mut got = self.get_multiple_accounts(&[*key])?;
        Ok(got.pop().flatten())
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
pub struct SignatureRef {
    pub signature: String,
    pub slot: u64,
    #[serde(default)]
    pub err: Option<Value>,
    #[serde(default, rename = "blockTime")]
    pub block_time: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxResponse {
    pub slot: u64,
    pub transaction: TxObject,
    #[serde(default)]
    pub meta: Option<TxMeta>,
    #[serde(default, rename = "blockTime")]
    pub block_time: Option<i64>,
    #[serde(default)]
    pub version: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxObject {
    pub message: TxMessage,
    #[serde(default)]
    pub signatures: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxMessage {
    #[serde(rename = "accountKeys")]
    pub account_keys: Vec<String>,
    pub header: TxHeader,
    pub instructions: Vec<TxInstruction>,
    #[serde(rename = "recentBlockhash")]
    pub recent_blockhash: String,
    #[serde(default, rename = "addressTableLookups")]
    pub address_table_lookups: Vec<AddressTableLookup>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AddressTableLookup {
    #[serde(rename = "accountKey")]
    pub account_key: String,
    #[serde(default, rename = "writableIndexes")]
    pub writable_indexes: Vec<u8>,
    #[serde(default, rename = "readonlyIndexes")]
    pub readonly_indexes: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxHeader {
    #[serde(rename = "numRequiredSignatures")]
    pub num_required_signatures: u8,
    #[serde(rename = "numReadonlySignedAccounts")]
    pub num_readonly_signed_accounts: u8,
    #[serde(rename = "numReadonlyUnsignedAccounts")]
    pub num_readonly_unsigned_accounts: u8,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxInstruction {
    #[serde(rename = "programIdIndex")]
    pub program_id_index: u8,
    pub accounts: Vec<u8>,
    pub data: String, // bs58 in `json` encoding
}

#[derive(Debug, Clone, Deserialize)]
pub struct TxMeta {
    #[serde(default)]
    pub err: Option<Value>,
    #[serde(default, rename = "logMessages")]
    pub log_messages: Option<Vec<String>>,
    #[serde(default, rename = "computeUnitsConsumed")]
    pub compute_units_consumed: Option<u64>,
    #[serde(default)]
    pub fee: Option<u64>,
    #[serde(default, rename = "preBalances")]
    pub pre_balances: Option<Vec<u64>>,
    #[serde(default, rename = "postBalances")]
    pub post_balances: Option<Vec<u64>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccountsResponse {
    value: Vec<Option<AccountInfo>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccountInfo {
    lamports: u64,
    owner: String,
    executable: bool,
    #[serde(rename = "rentEpoch")]
    rent_epoch: u64,
    data: (String, String), // (base64 string, "base64")
}

impl AccountInfo {
    fn try_into_account(self) -> Result<Account> {
        if self.data.1 != "base64" && self.data.1 != "base64+zstd" {
            bail!("unsupported account data encoding: {}", self.data.1);
        }
        if self.data.1 == "base64+zstd" {
            bail!("base64+zstd account data not yet supported");
        }
        let bytes = general_purpose::STANDARD
            .decode(self.data.0)
            .context("decode account data base64")?;
        let owner = Pubkey::from_str(&self.owner).context("decode account owner pubkey")?;
        Ok(Account {
            lamports: self.lamports,
            data: bytes,
            owner,
            executable: self.executable,
            rent_epoch: self.rent_epoch,
        })
    }
}
