use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct TransferKey {
    data_dir: PathBuf,
    api_url: String,
    target: TransferTarget,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum TransferTarget {
    Sha256([u8; 32]),
    Md5([u8; 16]),
    HttpUrl(String),
    Ipfs(Vec<String>),
}

pub(super) fn transfer_keys(request: &ChartDownloadRequest) -> Vec<TransferKey> {
    let (api_url, targets) = match &request.action {
        MissingChartAction::Http { api_url, md5, sha256 } => {
            let mut targets = Vec::with_capacity(3);
            if let Some(hash) = request.identity.sha256 {
                targets.push(TransferTarget::Sha256(hash));
            }
            if let Some(hash) = request.identity.md5 {
                targets.push(TransferTarget::Md5(hash));
            }
            if let Ok(mut url) = expand_http_api_url(api_url, md5, sha256) {
                url.set_fragment(None);
                targets.push(TransferTarget::HttpUrl(url.to_string()));
            }
            (api_url, targets)
        }
        MissingChartAction::Ipfs { api_url, cids } => {
            (api_url, vec![TransferTarget::Ipfs(cids.clone())])
        }
        _ => return Vec::new(),
    };
    targets
        .into_iter()
        .map(|target| TransferKey {
            data_dir: request.data_dir.clone(),
            api_url: api_url.clone(),
            target,
        })
        .collect()
}
