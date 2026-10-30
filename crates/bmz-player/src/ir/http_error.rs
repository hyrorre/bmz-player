use std::fmt;

use reqwest::{StatusCode, header};

#[derive(Debug)]
pub(crate) struct IrHttpResponseError {
    summary: String,
    status: StatusCode,
    retry_after_seconds: Option<u64>,
    moderation_code: Option<IrModerationCode>,
}

/// BMZ IR がアカウント・譜面の制限で送信を拒否したときに返す `data.code`。
///
/// どれもサーバー側の判断で、時間を置いて同じ内容を再送しても受理されない。
/// 送信キューはこれを恒久拒否として扱い、自動再試行や再ログインを繰り返さない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrModerationCode {
    AccountSuspended,
    AccountBanned,
    ChartHidden,
    ChartBlocked,
}

impl IrModerationCode {
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "ACCOUNT_SUSPENDED" => Some(Self::AccountSuspended),
            "ACCOUNT_BANNED" => Some(Self::AccountBanned),
            "CHART_HIDDEN" => Some(Self::ChartHidden),
            "CHART_BLOCKED" => Some(Self::ChartBlocked),
            _ => None,
        }
    }

    pub const fn as_code(self) -> &'static str {
        match self {
            Self::AccountSuspended => "ACCOUNT_SUSPENDED",
            Self::AccountBanned => "ACCOUNT_BANNED",
            Self::ChartHidden => "CHART_HIDDEN",
            Self::ChartBlocked => "CHART_BLOCKED",
        }
    }

    /// `http_response_error` が summary の末尾に付けた `[CODE]` を、保存済みの
    /// エラー文字列（送信キューの last_error、送信結果のメッセージ）から読む。
    pub fn from_error_message(message: &str) -> Option<Self> {
        let start = message.rfind('[')?;
        let code = message[start + 1..].trim_end().strip_suffix(']')?;
        Self::from_code(code)
    }

    /// アカウント単位の制限。同じアカウントの他のjobも同じ理由で拒否される。
    pub const fn is_account_level(self) -> bool {
        matches!(self, Self::AccountSuspended | Self::AccountBanned)
    }
}

impl fmt::Display for IrHttpResponseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.summary)
    }
}

impl std::error::Error for IrHttpResponseError {}

pub(crate) fn retry_after_seconds_from_error(error: &anyhow::Error) -> Option<u64> {
    error.chain().find_map(|cause| {
        cause.downcast_ref::<IrHttpResponseError>().and_then(|error| error.retry_after_seconds)
    })
}

pub(crate) fn moderation_code_from_error(error: &anyhow::Error) -> Option<IrModerationCode> {
    error.chain().find_map(|cause| {
        cause.downcast_ref::<IrHttpResponseError>().and_then(|error| error.moderation_code)
    })
}

pub(crate) fn status_code_from_error(error: &anyhow::Error) -> Option<StatusCode> {
    error
        .chain()
        .find_map(|cause| cause.downcast_ref::<IrHttpResponseError>().map(|error| error.status))
}

pub(crate) fn http_response_error(
    label: &str,
    status: StatusCode,
    body: &str,
    retry_after: Option<&str>,
) -> anyhow::Error {
    let moderation_code =
        (status == StatusCode::FORBIDDEN).then(|| moderation_code_from_body(body)).flatten();
    let mut summary =
        format!("{label} failed: {}", response_error_summary(status, body, retry_after));
    if let Some(code) = moderation_code {
        // 送信キューの last_error に残し、後から制限による拒否だと判別できるようにする。
        summary.push_str(" [");
        summary.push_str(code.as_code());
        summary.push(']');
    }
    anyhow::Error::new(IrHttpResponseError {
        summary,
        status,
        retry_after_seconds: retry_after.and_then(parse_retry_after_seconds),
        moderation_code,
    })
}

/// `{"data":{"code":"ACCOUNT_BANNED"}}` の形のエラーbodyから制限コードを読む。
/// 未知のコードや他のproviderの403は通常の認証エラーとして扱う。
fn moderation_code_from_body(body: &str) -> Option<IrModerationCode> {
    #[derive(serde::Deserialize)]
    struct ErrorBody {
        data: Option<ErrorData>,
    }
    #[derive(serde::Deserialize)]
    struct ErrorData {
        code: Option<String>,
    }
    let code = serde_json::from_str::<ErrorBody>(body).ok()?.data?.code?;
    IrModerationCode::from_code(&code)
}

pub(crate) fn parse_retry_after_seconds(value: &str) -> Option<u64> {
    value.trim().parse().ok()
}

/// Error bodies can contain credentials or upstream diagnostics. Only copy a
/// short, structured message into logs and omit every other body verbatim.
pub(crate) fn response_error_summary(
    status: StatusCode,
    body: &str,
    retry_after: Option<&str>,
) -> String {
    const MAX_MESSAGE_CHARS: usize = 200;

    #[derive(serde::Deserialize)]
    struct ErrorBody {
        #[serde(rename = "statusMessage")]
        status_message: Option<String>,
        message: Option<String>,
        error: Option<String>,
    }

    let message = serde_json::from_str::<ErrorBody>(body)
        .ok()
        .and_then(|body| body.status_message.or(body.message).or(body.error))
        .map(|message| message.trim().to_string())
        .filter(|message| !message.is_empty());
    let mut summary = match message {
        Some(message) => {
            let truncated: String = message.chars().take(MAX_MESSAGE_CHARS).collect();
            format!("{status} {truncated}")
        }
        None => format!("{status} (response body omitted, {} bytes)", body.len()),
    };
    if let Some(retry_after) = retry_after.filter(|value| !value.trim().is_empty()) {
        summary.push_str(" (retry after ");
        summary.push_str(retry_after.trim());
        summary.push('s');
        summary.push(')');
    }
    summary
}

pub(crate) fn retry_after_header(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get(header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
